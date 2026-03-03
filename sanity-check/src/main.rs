use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use ffxiv_versions_types::Version;
use ffxiv_versions_util::rw::read_csv_file;
use tokio::sync::RwLock;

mod thaliak;
use thaliak::get_thaliak_versions;

use crate::thaliak::BaseGameRepositoriesResponseVersion;

const FILES: [&str; 4] = ["global", "cn", "kr", "tw"];

#[tokio::main]
async fn main() -> Result<()> {
	env_logger::init();

	let source_folder = Path::new(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.context("Manifest directory has no parent")?
		.join("data")
		.join("csv");

	let thaliak_versions = tokio::spawn(async move { get_thaliak_versions().await });
	let thaliak_versions = Arc::new(RwLock::new(thaliak_versions.await??));

	let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();

	for file_name in FILES {
		let source_folder = source_folder.clone();
		let thaliak_versions = thaliak_versions.clone();
		join_set.spawn(async move {
			let file_path = source_folder.join(format!("{file_name}.csv"));
			let file = tokio::fs::File::open(file_path).await?;

			let versions = Arc::new(RwLock::new(read_csv_file(file).await?));

			let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();
			{
				let versions = versions.clone();
				join_set.spawn(async move { check_versions_basic(&versions.read().await).await });
			}
			{
				let versions = versions.clone();
				let thaliak_versions = thaliak_versions.clone();
				match file_name {
					"global" => {
						join_set.spawn(async move {
							check_version_thaliak(
								&versions.read().await,
								&thaliak_versions.read().await.global,
							)
							.await
						});
					},
					"cn" => {
						join_set.spawn(async move {
							check_version_thaliak(
								&versions.read().await,
								&thaliak_versions.read().await.cn,
							)
							.await
						});
					},
					"kr" => {
						join_set.spawn(async move {
							check_version_thaliak(
								&versions.read().await,
								&thaliak_versions.read().await.kr,
							)
							.await
						});
					},
					_ => {},
				};
			}

			while let Some(res) = join_set.join_next().await {
				res??;
			}
			Ok(())
		});
	}

	while let Some(res) = join_set.join_next().await {
		res??;
	}
	Ok(())
}

async fn check_versions_basic(versions: &[Version]) -> Result<()> {
	ensure!(versions.len() > 0);

	for version in versions {
		let release_date = version.release_date;
		let game_version_date = version.game_version.date;
		ensure!(game_version_date <= release_date);
		// ensure!(
		// 	game_version_date
		// 		>= release_date
		// 			.checked_sub_days(chrono::Days::new(32)) // This is kind of meaningless if it is over 1 month (KR version: 2025-09-26 >= 2025-10-28 - X Days)
		// 			.context("Resulting date would be out of range")?
		// );
	}

	for window in versions.windows(2) {
		let ver = &window[0];
		let next = &window[1];
		ensure!(ver.game_version.date < next.game_version.date);
	}

	Ok(())
}

async fn check_version_thaliak(
	versions: &[Version],
	thaliak_versions: &[BaseGameRepositoriesResponseVersion],
) -> Result<()> {
	for version in versions {
		let game_version_date = version
					.game_version
					.date
					.and_time(chrono::NaiveTime::from_hms_opt(8, 0, 0).context("Invalid time")?)
					.and_utc();
		let release_date = version
			.release_date
			.and_time(chrono::NaiveTime::from_hms_opt(10, 0, 0).context("Invalid time")?)
			.and_utc();
		for thaliak_version in thaliak_versions {
			

			ensure!(game_version_date <= thaliak_version.first_seen)
		}
	}

	Ok(())
}
