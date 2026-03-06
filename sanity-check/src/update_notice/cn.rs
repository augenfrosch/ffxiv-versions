use anyhow::Result;
use chrono::{DateTime, FixedOffset};
use regex::Regex;

use super::{UpdateNoticeInfo, UpdateNoticeType};

pub fn parse_update_notice(response_text: &str) -> Result<UpdateNoticeInfo> {
	use serde::{Deserialize, Deserializer};
	#[derive(Deserialize)]
	#[serde(rename_all = "PascalCase")]
	struct NewsDetailData {
		#[serde(deserialize_with = "deserialize_datetime")]
		publish_date: DateTime<FixedOffset>,
		content: String,
	}
	#[derive(Deserialize)]
	#[serde(rename_all = "PascalCase")]
	struct NewsDetail {
		data: NewsDetailData,
	}
	fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime<FixedOffset>, D::Error>
	where
		D: Deserializer<'de>,
	{
		const OFFSET: FixedOffset =
			FixedOffset::east_opt(8 * (60 * 60)).expect("Offset seconds OOB");
		const DATETIME_FORMAT: &str = "%Y/%m/%d %H:%M:%S";
		let s = String::deserialize(deserializer)?;
		let ndt = chrono::NaiveDateTime::parse_from_str(&s, DATETIME_FORMAT)
			.map_err(|_| serde::de::Error::custom("DateTime could not be deserialized"))?;
		let dt: DateTime<FixedOffset> = DateTime::from_naive_utc_and_offset(ndt, OFFSET);
		Ok(dt)
	}

	let news_detail: NewsDetail = serde_json::from_str(response_text)?;
	let re = Regex::new(
		r"Ver.(?<game_version>\d{4}.\d{2}.\d{2}.\d{4}.\d{4})（(?<patch>\d.\d+)(\+\d.\d+)?版本）",
	)?; // TODO: see above
	match re.captures(&news_detail.data.content) {
		Some(captures) => Ok(UpdateNoticeInfo {
			datetime: news_detail.data.publish_date.to_utc(),
			update_notice_type: UpdateNoticeType::NamedPatch {
				patch_note_url: None,
				patch_name: Some(captures["patch"].to_owned()),
				game_version: Some(captures["game_version"].parse()?),
			},
		}),
		None => Ok(UpdateNoticeInfo {
			datetime: news_detail.data.publish_date.to_utc(),
			update_notice_type: UpdateNoticeType::Hotfix,
		}),
	}
}
