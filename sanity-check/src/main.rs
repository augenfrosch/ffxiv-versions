use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use ffxiv_versions_types::{GameVersion, Version};
use ffxiv_versions_util::rw::read_csv_file;
use regex::Regex;
use tokio::sync::RwLock;
use url::Url;

mod thaliak;
use thaliak::get_thaliak_versions;

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
				ensure!(thaliak_version.version_string == "2025.06.10.0000.0000");
			}
			// This is a workaround for HIST Patches not being parsed. TODO: fix this to make sure it isn't skipping more
			if let Ok(thaliak_game_version) = thaliak_version.version_string.parse::<GameVersion>()
			{
				let same_version = thaliak_game_version == version.game_version;

				if !same_version {
					ensure!(
						thaliak_game_version < version.game_version,
						"Game version seen by Thaliak on {} is greater: {} > {}",
						version.release_date,
						thaliak_game_version,
						version.game_version
					);
				} else {
					seen_version = true;
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

#[derive(Debug)]
enum UpdateNoticeType {
	Hotfix,
	NamedPatch {
		patch_note_url: Url,
		patch_name: Option<String>,
		game_version: Option<GameVersion>,
	}, // Global has no patch name; only CN has game version
}

#[derive(Debug)]
struct UpdateNoticeInfo {
	pub datetime: DateTime<Utc>,
	pub update_notice_type: UpdateNoticeType,
}

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

async fn try_parse_update_notice_global(
	response_text: String,
	client: reqwest::Client,
) -> Result<UpdateNoticeInfo> {
	use scraper::{Html, Selector};
	let (datetime, link_href, hotfix) = {
		let html = Html::parse_document(&response_text);

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
			.map(|s| s.to_owned());
		// ensure!(selection.next() == None); // Doesn't hold true for global's post early access hotfix (2024.07.06.0000.0000)

		let selector = Selector::parse("article > div:first-of-type")
			.map_err(|err| anyhow::anyhow!("Failed to parse_selector ({err})"))?;
		let mut selection = html.select(&selector);
		let re = Regex::new("FINAL FANTASY XIV Hot[fF]ix(es)?")?;
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
				patch_note_url,
				patch_name: None,
				game_version: None,
			},
		})
	} else {
		bail!("Failed to parse update notice")
	}
}

async fn check_versions_update_notices(versions: Versions, file_name: &str) -> Result<()> {
	let versions: &[Version] = &versions.read().await;
	// KR closes the connection early without a user agent
	let client = reqwest::Client::builder()
		.user_agent(USER_AGENT)
		.redirect(reqwest::redirect::Policy::none())
		.build()?;
	for version in versions {
		if let Some(update_notice_url) = &version.update_notice_url {
			let url = match file_name {
				"cn" => {
					let mut url = Url::parse("https://cqnews.web.sdo.com/api/news/newsDetail")?;
					let id: &str = update_notice_url
						.path_segments()
						.context("Url cannot be base")?
						.next_back()
						.context("Update notice url is missing ID")?;
					url.set_query(Some(&format!("?gameCode=ff&id={id}")));
					url
				},
				_ => update_notice_url.clone(),
			};
			let response_text = client
				.get(url)
				// .header("Connection", "keep-alive")
				.send()
				.await?
				.text()
				.await?;
			let update_notice_info = match file_name {
				"global" => {
					Some(try_parse_update_notice_global(response_text, client.clone()).await?)
				},
				// temp; TODO remove this / make this non optional
				_ => None,
			};

			if let Some(update_notice_info) = update_notice_info {
				ensure!(version.release_date == update_notice_info.datetime.date_naive());
				match update_notice_info.update_notice_type {
					UpdateNoticeType::Hotfix => ensure!(version.patch_note_url == None),
					UpdateNoticeType::NamedPatch {
						patch_note_url,
						patch_name,
						game_version,
					} => {
						ensure!(
							version
								.patch_note_url
								.as_ref()
								.is_some_and(|url| url == &patch_note_url),
							"Version {} patch note url doesn't match upated notice: {} vs. {}",
							version.game_version,
							version.patch_note_url.clone().unwrap(),
							patch_note_url
						);

						if let Some(patch_name) = patch_name {
							ensure!(version.version_name == patch_name);
						}
						if let Some(game_version) = game_version {
							ensure!(version.game_version == game_version);
						}
					},
				}
			}
		}
	}

	Ok(())
}
