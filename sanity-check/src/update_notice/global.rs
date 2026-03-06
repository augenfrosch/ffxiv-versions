use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use regex::Regex;
use url::Url;

use super::{UpdateNoticeInfo, UpdateNoticeType};

pub async fn parse_update_notice(
	response_text: &str,
	client: reqwest::Client,
) -> Result<UpdateNoticeInfo> {
	use scraper::{Html, Selector};
	let (datetime, link_href, hotfix) = {
		let html = Html::parse_document(response_text);

		let selector = Selector::parse("article > header > time > script")
			.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
		let mut selection = html.select(&selector);
		let datetime = selection.next().context("Selection is empty")?.inner_html();
		ensure!(selection.next() == None);
		let re = Regex::new(r"dst_strftime\((?<timestamp>\d+), '.+?'\);")?; // TODO: don't compile each call
		let datetime: DateTime<Utc> = DateTime::from_timestamp_secs(
			re.captures(&datetime).context("Missing timestamp")?["timestamp"].parse()?,
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
		let re = Regex::new("FINAL FANTASY XIV Hot[fF]ix(es)?")?; // TODO: see above 
		let hotfix = selection
			.next()
			.map(|element_ref| element_ref.text().any(|text| re.is_match(text)))
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
			update_notice_type: UpdateNoticeType::NamedPatch {
				patch_note_url: Some(patch_note_url),
				patch_name: None,
				game_version: None,
			},
		})
	} else {
		bail!("Failed to parse update notice")
	}
}
