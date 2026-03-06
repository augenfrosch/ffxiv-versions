use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use super::{UpdateNoticeInfo, UpdateNoticeType};

#[derive(Debug)]
pub struct Regexes {
	maintenance_time_re: Regex,
	patch_name_re: Regex,
}

impl Regexes {
	pub fn compile_all() -> Result<Self> {
		Ok(Self {
			maintenance_time_re: Regex::new(
				r"(?<month>\d{1,2})/(?<day>\d{1,2}) (?<start_time>\d{1,2}:\d{2}) ~ (?<end_time>\d{1,2}:\d{2})",
			)?,
			patch_name_re: Regex::new(r"patch_(?<patch>\d.\d+)_notes.html")?,
		})
	}
}

pub fn parse_update_notice(response_text: &str, regexes: &Regexes) -> Result<UpdateNoticeInfo> {
	const OFFSET: FixedOffset = FixedOffset::east_opt(8 * (60 * 60)).expect("Offset seconds OOB");
	const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M";

	let html = Html::parse_document(response_text);

	// The TW update notices are also their maintenance notices and (unlike CN & KR) they apparently
	// also don't make a new one announcing the end of the maintenance and instead update the first one (this makes getting the date much more involved)
	let selector = Selector::parse(".content > .news_title > .news_info1 > .news_info11-2 > .Date")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	let mut datetime_text = selection.next().context("Selection is empty")?.text();
	ensure!(selection.next() == None);
	let mut naive_datetime = NaiveDateTime::parse_from_str(
		datetime_text.next().context("Missing datetime text")?,
		DATETIME_FORMAT,
	)
	.context("Failed to parse DateTime")?;
	ensure!(datetime_text.next() == None);
	let selector = Selector::parse(".content > .article .notice")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	while let Some(captures) = selection
		.next()
		.and_then(|element_ref| element_ref.text().next())
		.and_then(|notice_text| regexes.maintenance_time_re.captures(notice_text))
	{
		let day = captures["day"].parse()?;
		let month = captures["month"].parse()?;
		let year = if month >= naive_datetime.month() {
			naive_datetime.year()
		} else {
			naive_datetime.year() + 1
		};
		let end_time = NaiveTime::parse_from_str(&captures["end_time"], "%H:%M")?;

		naive_datetime = NaiveDateTime::new(
			NaiveDate::from_ymd_opt(year, month, day).context("Invalid date")?,
			end_time,
		);
	}
	let date_time: DateTime<FixedOffset> =
		DateTime::from_naive_utc_and_offset(naive_datetime, OFFSET);

	let selector = Selector::parse(".content > .article a")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	let link_element_ref = selection.next();
	ensure!(selection.next() == None);

	if let Some(link_element_ref) = link_element_ref {
		let href_attr = link_element_ref
			.attr("href")
			.context("Patch note link is missing href attribute")?;
		let patch_note_url = Url::parse(href_attr)?;
		// The patch name is also seen in the text but the pages layout is currently quite volatile
		let patch_name = regexes
			.patch_name_re
			.captures(href_attr)
			.context("Missing patch name in URL")?["patch"]
			.to_owned();

		Ok(UpdateNoticeInfo {
			datetime: date_time.to_utc(),
			update_notice_type: UpdateNoticeType::NamedPatchKrTw {
				patch_note_url,
				patch_name,
			},
		})
	} else {
		// There is currently no example for a hotfix update notice for TW
		bail!("Failed to parse update notice")
	}
}
