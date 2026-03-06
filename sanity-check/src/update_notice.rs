use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use ffxiv_versions_types::{GameVersion, Version};
use ffxiv_versions_util::DataFile;
use url::Url;

use crate::Versions;

mod cn;
mod global;
mod kr;
mod tw;

#[derive(Debug, PartialEq)]
pub enum UpdateNoticeType {
	Hotfix,
	NamedPatchGlobal {
		patch_note_url: Url,
	},
	NamedPatchCn {
		patch_name: String,
		game_version: GameVersion,
	},
	NamedPatchKrTw {
		patch_note_url: Url,
		patch_name: String,
	},
}

impl UpdateNoticeType {
	pub fn patch_note_url(&self) -> Option<&Url> {
		match self {
			Self::NamedPatchGlobal { patch_note_url }
			| Self::NamedPatchKrTw { patch_note_url, .. } => Some(patch_note_url),
			Self::Hotfix | Self::NamedPatchCn { .. } => None,
		}
	}
	pub fn patch_name(&self) -> Option<&str> {
		match self {
			Self::NamedPatchCn { patch_name, .. } | Self::NamedPatchKrTw { patch_name, .. } => {
				Some(patch_name)
			},
			Self::Hotfix | Self::NamedPatchGlobal { .. } => None,
		}
	}
	pub fn game_version(&self) -> Option<&GameVersion> {
		match self {
			Self::NamedPatchCn { game_version, .. } => Some(game_version),
			Self::Hotfix | Self::NamedPatchGlobal { .. } | Self::NamedPatchKrTw { .. } => None,
		}
	}
}

#[derive(Debug)]
pub struct UpdateNoticeInfo {
	pub datetime: DateTime<Utc>,
	pub update_notice_type: UpdateNoticeType,
}

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

pub async fn check_versions_update_notices(versions: Versions, data_file: DataFile) -> Result<()> {
	let versions: &[Version] = &versions.read().await;
	// KR closes the connection early without a user agent
	let client = reqwest::Client::builder()
		.user_agent(USER_AGENT)
		.redirect(reqwest::redirect::Policy::none())
		.build()?;
	for (version, update_notice_url) in versions
		.iter()
		.filter_map(|version| version.update_notice_url.as_ref().map(|url| (version, url)))
	{
		let url = match data_file {
			DataFile::Cn => {
				let mut url = Url::parse("https://cqnews.web.sdo.com/api/news/newsDetail")?;
				let id: &str = update_notice_url
					.fragment()
					.context("Url has no fragment")?
					.split('/')
					.next_back()
					.context("Update notice url is missing ID")?;
				url.set_query(Some(&format!("gameCode=ff&id={id}")));
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
		let update_notice_info = match data_file {
			DataFile::Global => global::parse_update_notice(&response_text, client.clone()).await?,
			DataFile::Cn => cn::parse_update_notice(&response_text)?,
			DataFile::Kr => kr::parse_update_notice(&response_text)?,
			DataFile::Tw => tw::parse_update_notice(&response_text)?,
		};

		if (data_file == DataFile::Cn || data_file == DataFile::Kr)
			&& version.version_name.ends_with(".0")
		{
			// CN & KR update notices are combined with maintenance-end notices, resulting in a larger delta
			// depending on the patch release timing and/or length of the maintenace, such as for an expansions release.
			// The downloads, here for 7.0, are available one day early for these versions
			let time_delta = update_notice_info.datetime.date_naive() - version.release_date;
			ensure!(
				time_delta.num_days() <= 1,
				"Version {} release date is not (reasonably close) before update notices release: {} vs {} ({:?})",
				version.game_version,
				version.release_date,
				update_notice_info.datetime.date_naive(),
				update_notice_info.datetime
			);
		} else if data_file == DataFile::Cn
			&& version.release_date != update_notice_info.datetime.date_naive()
		{
			// CN's 7.35 (+7.38) update notice is dated Nov. 3rd but talks about the maintenance on the 4th being completed
			// (if the translation I'm working with is correct, which it might very not be). Maybe it was written early and the date was not updated when it was published
			// All other datetime references I could find indicate htat Nov. 4th being correct
			// TODO: Find a better way of defining expceptions (this is getting silly at just 3 instances and I haven't even implemented it for KR and TW)
			ensure!(version.game_version.to_string() == "2025.10.23.0000.0000");
		} else {
			ensure!(
				version.release_date == update_notice_info.datetime.date_naive(),
				"Version {}  release date does not match update notices release: {} vs {} ({:?})",
				version.game_version,
				version.release_date,
				update_notice_info.datetime.date_naive(),
				update_notice_info.datetime
			);
		}

		if update_notice_info.update_notice_type == UpdateNoticeType::Hotfix {
			ensure!(
				version.patch_note_url.is_none(),
				"Hotfixes should not have patch notes: {}",
				version.game_version
			);
		}
		if let Some(patch_note_url) = update_notice_info.update_notice_type.patch_note_url() {
			ensure!(
				version
					.patch_note_url
					.as_ref()
					.is_some_and(|url| url == patch_note_url),
				"Version {} patch note url doesn't match upated notice: {} vs. {}",
				version.game_version,
				version.patch_note_url.clone().unwrap(),
				patch_note_url
			);
		}
		if let Some(patch_name) = update_notice_info.update_notice_type.patch_name() {
			ensure!(version.version_name == patch_name);
		}
		if let Some(game_version) = update_notice_info.update_notice_type.game_version() {
			ensure!(&version.game_version == game_version);
		}
	}

	Ok(())
}
