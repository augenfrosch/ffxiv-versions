use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use url::Url;

mod release_date_format;

const VERSION_DATE_FORMAT: &str = "%Y.%m.%d";
const RELEASE_DATE_FORMAT: &str = "%Y-%m-%d";

#[derive(Debug, PartialEq)]
pub struct GameVersion {
	pub date: NaiveDate,
	pub part: u32,
	pub revision: u32,
	// HIST patches are currently not accounted for
}

impl Serialize for GameVersion {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let GameVersion {
			date,
			part,
			revision,
		} = self;
		let s = format!(
			"{date}.{part:04}.{revision:04}",
			date = date.format(VERSION_DATE_FORMAT),
		);
		serializer.serialize_str(&s)
	}
}

impl<'de> Deserialize<'de> for GameVersion {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		use serde::de::Error;

		let s = String::deserialize(deserializer)?;
		let (date_str, part_str, revision_str) = s
			.rsplit_once('.')
			.map(|(l, revision_str)| {
				let (date_str, part_str) =
					l.rsplit_once('.').ok_or(Error::custom("Missing part"))?;
				Ok((date_str, part_str, revision_str))
			})
			.ok_or(Error::custom("Missing revision"))??;
		Ok(GameVersion {
			date: NaiveDate::parse_from_str(date_str, VERSION_DATE_FORMAT)
				.map_err(Error::custom)?,
			part: part_str.parse().map_err(Error::custom)?,
			revision: revision_str.parse().map_err(Error::custom)?,
		})
	}
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Version {
	pub game_version: GameVersion,
	pub version_name: String,
	#[serde(with = "release_date_format")]
	pub release_date: NaiveDate,
	pub patch_note_url: Option<Url>,
	pub update_notice_url: Option<Url>,
}
