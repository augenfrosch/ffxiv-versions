use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use ffxiv_versions_types::{GameVersion, Version};
use ffxiv_versions_util::rw::read_csv_file;
use tokio::sync::RwLock;

mod thaliak;
use thaliak::get_thaliak_versions;

mod update_notice;
use update_notice::check_versions_update_notices;

use crate::thaliak::BaseGameRepositoriesResponse;

const FILES: [&str; 4] = ["global", "cn", "kr", "tw"];

type Versions = Arc<RwLock<Vec<Version>>>;
type ThaliakVersions = Arc<RwLock<BaseGameRepositoriesResponse>>;

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

			join_set.spawn(check_versions_basic(versions.clone()));
			join_set.spawn(check_version_thaliak(
				versions.clone(),
				thaliak_versions.clone(),
				file_name,
			));
			join_set.spawn(check_versions_update_notices(versions.clone(), file_name));

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

async fn check_versions_basic(versions: Versions) -> Result<()> {
	let versions: &[Version] = &versions.read().await;
	ensure!(versions.len() > 0);

	for version in versions {
		let release_date = version.release_date;
		let game_version_date = version.game_version.date;
		ensure!(game_version_date <= release_date);
	}

	for window in versions.windows(2) {
		let ver = &window[0];
		let next = &window[1];
		ensure!(ver.game_version.date < next.game_version.date);
	}

	Ok(())
}

async fn check_version_thaliak(
	versions: Versions,
	thaliak_versions: ThaliakVersions,
	file_name: &str,
) -> Result<()> {
	let versions: &[Version] = &versions.read().await;
	let thaliak_versions = match file_name {
		"global" => &thaliak_versions.read().await.global,
		"cn" => &thaliak_versions.read().await.cn,
		"kr" => &thaliak_versions.read().await.kr,
		_ => return Ok(()), // TW is not yet supported by Thaliak (officially / for v1)
	};
	for version in versions {
		let mut seen_version = false;
		for thaliak_version in thaliak_versions
			.iter()
			.filter(|th_ver| version.release_date == th_ver.first_seen.date_naive())
		{
			if thaliak_version.first_seen.date_naive() != thaliak_version.first_offered.date_naive()
			{
				// Global only; Thaliak was only offered patch with a delay. Release date is correct / the same as `first_seen`'s date
				ensure!(
					file_name == "global"
						&& thaliak_version.version_string == "2025.06.10.0000.0000"
				);
			}
			// This is a workaround for HIST Patches not being parsed. TODO: fix this to make sure it isn't skipping more
			if let Ok(thaliak_game_version) = thaliak_version.version_string.parse::<GameVersion>()
			{
				let same_version = thaliak_game_version == version.game_version;

				if same_version {
					seen_version = true;
				} else {
					ensure!(
						thaliak_game_version < version.game_version,
						"Game version seen by Thaliak on {} is greater: {} > {}",
						version.release_date,
						thaliak_game_version,
						version.game_version
					);
				}
			}
		}
		ensure!(
			seen_version,
			"Game version {} not seen by Thaliak on {}",
			version.game_version,
			version.release_date
		);
	}

	Ok(())
}
