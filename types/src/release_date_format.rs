use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serializer};

use crate::RELEASE_DATE_FORMAT;

#[expect(clippy::trivially_copy_pass_by_ref)] // Required for correct function signature
pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	let s = format!("{}", date.format(RELEASE_DATE_FORMAT));
	serializer.serialize_str(&s)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
	D: Deserializer<'de>,
{
	let s = String::deserialize(deserializer)?;
	NaiveDate::parse_from_str(&s, RELEASE_DATE_FORMAT).map_err(serde::de::Error::custom)
}
