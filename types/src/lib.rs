use std::{error::Error, fmt::Display, str::FromStr};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use url::Url;

mod release_date_format;

const VERSION_DATE_FORMAT: &str = "%Y.%m.%d";
const RELEASE_DATE_FORMAT: &str = "%Y-%m-%d";

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct GameVersion {
	pub date: NaiveDate,
	pub part: u32,
	pub revision: u32,
	// HIST patches are currently not accounted for
}

impl Display for GameVersion {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let GameVersion {
			date,
			part,
			revision,
		} = self;
		write!(
			f,
			"{date}.{part:04}.{revision:04}",
			date = date.format(VERSION_DATE_FORMAT)
		)
	}
}

impl Serialize for GameVersion {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let s = self.to_string();
		serializer.serialize_str(&s)
	}
}

#[non_exhaustive]
#[derive(Debug)]
pub enum ParseGameVersionError {
	MissingParts,
	PartParsing,
}

impl Display for ParseGameVersionError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"Failed to parse game version, {reason}",
			reason = match self {
				Self::MissingParts => "some parts are missing from the name",
				Self::PartParsing => "some parts could not be parsed",
			}
		)
	}
}

impl Error for ParseGameVersionError {}

impl FromStr for GameVersion {
	type Err = ParseGameVersionError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (date_str, part_str, revision_str) = s
			.rsplit_once('.')
			.map(|(l, revision_str)| {
				let (date_str, part_str) = l
					.rsplit_once('.')
					.ok_or(ParseGameVersionError::MissingParts)?;
				Ok((date_str, part_str, revision_str))
			})
			.ok_or(ParseGameVersionError::MissingParts)??;
		Ok(GameVersion {
			date: NaiveDate::parse_from_str(date_str, VERSION_DATE_FORMAT)
				.map_err(|_| ParseGameVersionError::PartParsing)?,
			part: part_str
				.parse()
				.map_err(|_| ParseGameVersionError::PartParsing)?,
			revision: revision_str
				.parse()
				.map_err(|_| ParseGameVersionError::PartParsing)?,
		})
	}
}

impl<'de> Deserialize<'de> for GameVersion {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		s.parse()
			.map_err(|_| serde::de::Error::custom("Failed to parse game version"))
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
	pub game_version: GameVersion,
	pub version_name: String,
	#[serde(with = "release_date_format")]
	pub release_date: NaiveDate,
	pub patch_note_url: Option<Url>,
	pub update_notice_url: Option<Url>,
}
