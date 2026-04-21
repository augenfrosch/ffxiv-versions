use anyhow::{Context, Error, Result, ensure};
use chrono::{Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
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
				r"(?<month>\d{1,2})/(?<day>\d{1,2}) (?<start_time>\d{1,2}:\d{2}) [~～] (?<end_time>\d{1,2}:\d{2})",
			)?,
			patch_name_re: Regex::new(r"(?<patch>\d.\d+)( )?版本")?,
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
	let naive_datetime = NaiveDateTime::parse_from_str(
		datetime_text.next().context("Missing datetime text")?,
		DATETIME_FORMAT,
	)
	.context("Failed to parse DateTime")?;
	ensure!(datetime_text.next() == None);
	let selector = Selector::parse(".content > .article .notice")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let maintenance_end_time = html
		.select(&selector)
		.next()
		.and_then(|element_ref| element_ref.text().next())
		.and_then(|notice_text| regexes.maintenance_time_re.captures(notice_text))
		.map(|captures| {
			let day = captures["day"].parse()?;
			let month = captures["month"].parse()?;
			let year = if month >= naive_datetime.month() {
				naive_datetime.year()
			} else {
				naive_datetime.year() + 1
			};
			Ok::<NaiveDateTime, Error>(NaiveDateTime::new(
				NaiveDate::from_ymd_opt(year, month, day).context("Invalid date")?,
				NaiveTime::parse_from_str(&captures["end_time"], "%H:%M")?,
			))
		})
		.transpose()?
		.context("Missing maintenance end time")?;
	let date_time = maintenance_end_time
		.and_local_timezone(OFFSET)
		.latest()
		.context("Could not convert datetime using time zone")?;

	let selector = Selector::parse(".content > .article b")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	let patch_name = selection
		.next_back()
		.and_then(|bold_element| bold_element.text().next())
		.and_then(|text| regexes.patch_name_re.captures(text))
		.map(|captures| captures["patch"].to_owned());

	let selector = Selector::parse(".content > .article a")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	let patch_note_url = selection
		.next()
		.and_then(|element_ref| element_ref.attr("href"))
		.map(Url::parse)
		.transpose()?;
	ensure!(selection.next() == None);

	if let Some(patch_name) = patch_name {
		Ok(UpdateNoticeInfo {
			datetime: date_time.to_utc(),
			update_notice_type: UpdateNoticeType::NamedPatchTw {
				patch_note_url,
				patch_name,
			},
		})
	} else {
		Ok(UpdateNoticeInfo {
			datetime: date_time.to_utc(),
			update_notice_type: UpdateNoticeType::Hotfix,
		})
	}
}
