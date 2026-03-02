use anyhow::{Context, Result};
use chrono::NaiveDate;
use ffxiv_versions_types::{GameVersion, Version};
use serde::Serialize;
use url::Url;

const EXPECTED_JSON: &str = r#"{
	"game_version": "2026.01.30.0000.0000",
	"version_name": "7.41x1",
	"release_date": "2026-02-05",
	"patch_note_url": null,
	"update_notice_url": "https://na.finalfantasyxiv.com/lodestone/news/detail/e1cabf2fe5698223626bd53e6b6057a7612cf8fe"
}"#;

const EXPECTED_CSV: &str = r"game_version,version_name,release_date,patch_note_url,update_notice_url
2026.01.30.0000.0000,7.41x1,2026-02-05,,https://na.finalfantasyxiv.com/lodestone/news/detail/e1cabf2fe5698223626bd53e6b6057a7612cf8fe";

fn test_version() -> Result<Version> {
	// 2026.01.30.0000.0000,7.41x1,2026-02-05,,https://na.finalfantasyxiv.com/lodestone/news/detail/e1cabf2fe5698223626bd53e6b6057a7612cf8fe
	Ok(Version {
		game_version: GameVersion {
			date: NaiveDate::from_ymd_opt(2026, 1, 30).context("Invalid date")?,
			part: 0,
			revision: 0,
		},
		version_name: "7.41x1".to_owned(),
		release_date: NaiveDate::from_ymd_opt(2026, 2, 5).context("Invalid date")?,
		patch_note_url: None,
		update_notice_url: Some(Url::parse(
			"https://na.finalfantasyxiv.com/lodestone/news/detail/e1cabf2fe5698223626bd53e6b6057a7612cf8fe",
		)?),
	})
}

#[test]
fn json_test() -> Result<()> {
	let version = test_version()?;

	let mut buf = Vec::with_capacity(256);
	let mut serializer = serde_json::Serializer::with_formatter(
		&mut buf,
		serde_json::ser::PrettyFormatter::with_indent(b"\t"),
	);
	version.serialize(&mut serializer)?;
	let json = String::from_utf8(buf)?;
	assert_eq!(json, EXPECTED_JSON);

	let deserialized_version = serde_json::from_str::<Version>(&json)?;
	assert_eq!(deserialized_version, version);

	Ok(())
}

#[test]
fn csv_test() -> Result<()> {
	let version = test_version()?;
	let mut buf = Vec::with_capacity(256);
	let mut writer = csv::Writer::from_writer(&mut buf);
	writer.serialize(&version)?;
	drop(writer);
	let mut csv = String::from_utf8(buf)?;
	csv.truncate(csv.trim_end().len()); // trailing '\n'
	assert_eq!(csv, EXPECTED_CSV);

	let mut reader = csv::Reader::from_reader(csv.as_bytes());
	let mut records_iter = reader.deserialize();
	let deserialized_version: Version = records_iter.next().unwrap()?;
	assert_eq!(deserialized_version, version);
	assert!(records_iter.next().is_none());

	Ok(())
}
