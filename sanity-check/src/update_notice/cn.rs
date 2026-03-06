use anyhow::Result;
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use regex::Regex;
use serde::{Deserialize, Deserializer};

use super::{UpdateNoticeInfo, UpdateNoticeType};

#[derive(Debug)]
pub struct Regexes {
	game_version_re: Regex,
}

impl Regexes {
	pub fn compile_all() -> Result<Self> {
		Ok(Self {
			game_version_re: Regex::new(
				r"Ver.(?<game_version>\d{4}.\d{2}.\d{2}.\d{4}.\d{4})（(?<patch>\d.\d+)(\+\d.\d+)?版本）",
			)?,
		})
	}
}

pub fn parse_update_notice(response_text: &str, regexes: &Regexes) -> Result<UpdateNoticeInfo> {
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
		let ndt = NaiveDateTime::parse_from_str(&s, DATETIME_FORMAT)
			.map_err(|_| serde::de::Error::custom("DateTime could not be deserialized"))?;
		let dt: DateTime<FixedOffset> =
			ndt.and_local_timezone(OFFSET)
				.latest()
				.ok_or(serde::de::Error::custom(
					"DateTime could not be deserialized",
				))?;
		Ok(dt)
	}

	let news_detail: NewsDetail = serde_json::from_str(response_text)?;
	match regexes.game_version_re.captures(&news_detail.data.content) {
		Some(captures) => Ok(UpdateNoticeInfo {
			datetime: news_detail.data.publish_date.to_utc(),
			update_notice_type: UpdateNoticeType::NamedPatchCn {
				patch_name: captures["patch"].to_owned(),
				game_version: captures["game_version"].parse()?,
			},
		}),
		None => Ok(UpdateNoticeInfo {
			datetime: news_detail.data.publish_date.to_utc(),
			update_notice_type: UpdateNoticeType::Hotfix,
		}),
	}
}
