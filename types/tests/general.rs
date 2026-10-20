use anyhow::Result;
use chrono::NaiveDate;
use ffxiv_versions_types::{GameVersion, PatchType};

#[test]
fn parse_and_stringify_versions() -> Result<()> {
	let versions = [
		(
			"2024.07.03.0000.0001",
			GameVersion {
				date: NaiveDate::from_ymd_opt(2024, 7, 3).unwrap(),
				part: 0,
				revision: 1,
				patch_type: PatchType::Delta,
			},
		),
		(
			"H2024.05.31.0000.0000aa",
			GameVersion {
				date: NaiveDate::from_ymd_opt(2024, 5, 31).unwrap(),
				part: 0,
				revision: 0,
				patch_type: PatchType::History {
					section: "aa".to_string(),
				},
			},
		),
	];

	for (version_str, version) in versions {
		assert_eq!(version_str.parse().as_ref(), Ok(&version));
		assert_eq!(version_str, version.to_string());
	}

	Ok(())
}

#[test]
fn compare_versions() -> Result<()> {
	let hist_dt = "H2024.05.31.0000.0000b".parse::<GameVersion>()?;
	let hist_dt_later = "H2024.05.31.0000.0000aa".parse::<GameVersion>()?;
	let diff_dt = "2024.05.31.0000.0000".parse::<GameVersion>()?;
	let diff_dt_later = "2024.07.03.0000.0001".parse::<GameVersion>()?;

	assert!(hist_dt < hist_dt_later);
	assert!(hist_dt_later < diff_dt);
	assert!(diff_dt < diff_dt_later);

	Ok(())
}
