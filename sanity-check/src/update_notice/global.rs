use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use super::{UpdateNoticeInfo, UpdateNoticeType};

#[derive(Debug)]
pub struct Regexes {
	timestamp_re: Regex,
	hotfix_re: Regex,
}

impl Regexes {
	pub fn compile_all() -> Result<Self> {
		Ok(Self {
			timestamp_re: Regex::new(r"dst_strftime\((?<timestamp>\d+), '.+?'\);")?,
			hotfix_re: Regex::new("FINAL FANTASY XIV Hot[fF]ix(es)?")?,
		})
	}
}

pub async fn parse_update_notice(
	response_text: &str,
	regexes: &Regexes,
	client: reqwest::Client,
) -> Result<UpdateNoticeInfo> {
	let (datetime, link_href, hotfix) = {
		let html = Html::parse_document(response_text);

		let selector = Selector::parse("article > header > time > script")
			.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
		let mut selection = html.select(&selector);
		let datetime = selection.next().context("Selection is empty")?.inner_html();
		ensure!(selection.next() == None);
		let datetime: DateTime<Utc> = DateTime::from_timestamp_secs(
			regexes
				.timestamp_re
				.captures(&datetime)
				.context("Missing timestamp")?["timestamp"]
				.parse()?,
		)
		.context("Timestamp out of range for DateTime")?;

		let selector = Selector::parse("article > div:first-of-type > a")
			.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
		let mut selection = html.select(&selector);
		let link_href = selection
			.next()
			.and_then(|element_ref| element_ref.attr("href"))
			.map(std::borrow::ToOwned::to_owned);
		// ensure!(selection.next() == None); // Doesn't hold true for global's post early access hotfix (2024.07.06.0000.0000)

		let selector = Selector::parse("article > div:first-of-type")
			.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
		let mut selection = html.select(&selector);
		let hotfix = selection
			.next()
			.map(|element_ref| {
				element_ref
					.text()
					.any(|text| regexes.hotfix_re.is_match(text))
			})
			.context("Selection is empty")?;
		ensure!(selection.next() == None);

		(datetime, link_href, hotfix)
	};

	if hotfix {
		Ok(UpdateNoticeInfo {
			datetime,
			update_notice_type: UpdateNoticeType::Hotfix,
		})
	} else if let Some(href) = link_href {
		let url = Url::parse(&href)?;
		let response = client.get(url).send().await?;
		let patch_note_url = Url::parse(
			response
				.headers()
				.get("Location")
				.context("Redirect missing `Location` header")?
				.to_str()?,
		)?;

		Ok(UpdateNoticeInfo {
			datetime,
			update_notice_type: UpdateNoticeType::NamedPatchGlobal { patch_note_url },
		})
	} else {
		bail!("Failed to parse update notice")
	}
}
