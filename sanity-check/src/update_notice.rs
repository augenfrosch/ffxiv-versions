use std::sync::Arc;

use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use ffxiv_versions_types::GameVersion;
use ffxiv_versions_util::DataFile;
use url::Url;

use crate::{UpdateNotices, Versions};

pub mod cn;
pub mod global;
pub mod kr;
pub mod tw;

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

pub async fn check_versions_update_notices(
	versions: Arc<Versions>,
	update_notices: Arc<UpdateNotices>,
	data_file: DataFile,
) -> Result<()> {
	for (version, update_notice_info) in versions
		.get(data_file)
		.await
		.iter()
		.zip(update_notices.get(data_file).await)
	{
		let Some(update_notice_info) = update_notice_info else {
			continue;
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
