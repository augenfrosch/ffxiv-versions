use anyhow::{Context, Result, ensure};
use chrono::{FixedOffset, NaiveDateTime};
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use super::{UpdateNoticeInfo, UpdateNoticeType};

const OFFSET: FixedOffset = FixedOffset::east_opt(9 * (60 * 60)).expect("Offset seconds OOB");
const DATETIME_FORMAT: &str = "%Y-%m-%dT%H:%M";

#[derive(Debug)]
pub struct Regexes {
	patch_name_re: Regex,
}

impl Regexes {
	pub fn compile_all() -> Result<Self> {
		Ok(Self {
			patch_name_re: Regex::new(r"\[V(?<patch>\d.\d+) 패치노트 바로( )?가기\]")?,
		})
	}
}

pub fn parse_update_notice(response_text: &str, regexes: &Regexes) -> Result<UpdateNoticeInfo> {
	let html = Html::parse_document(response_text);

	let selector = Selector::parse(".view_summary time")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	let datetime_text = selection
		.next()
		.context("Selection is empty")?
		.attr("datetime")
		.context("`time` element missing `datetime` attribute")?;
	ensure!(selection.next() == None);

	let date_time = {
		let naive_datetime = NaiveDateTime::parse_from_str(datetime_text, DATETIME_FORMAT)
			.context("Failed to parse DateTime")?;

		naive_datetime
			.and_local_timezone(OFFSET)
			.latest()
			.context("Could not convert datetime using time zone")?
	};

	let selector = Selector::parse(".view_contents a")
		.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
	let mut selection = html.select(&selector);
	let link_element_ref = selection.next();
	ensure!(selection.next() == None);

	if let Some(link_element_ref) = link_element_ref {
		let href_attr = link_element_ref
			.attr("href")
			.context("Patch note link is missing href attribute")?;
		let mut patch_note_url = Url::parse("https://www.ff14.co.kr")?;
		patch_note_url.set_path(href_attr);
		let inner_html = link_element_ref.inner_html();
		let patch_name = regexes
			.patch_name_re
			.captures(&inner_html)
			.context("Missing patch note link text")?["patch"]
			.to_owned();

		Ok(UpdateNoticeInfo {
			datetime: date_time.to_utc(),
			update_notice_type: UpdateNoticeType::NamedPatchKr {
				patch_note_url,
				patch_name,
			},
		})
	} else {
		// Additional marker: "Hot-fix" in the text
		Ok(UpdateNoticeInfo {
			datetime: date_time.to_utc(),
			update_notice_type: UpdateNoticeType::Hotfix,
		})
	}
}
