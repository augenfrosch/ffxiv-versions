use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use ffxiv_versions_types::{GameVersion, Version};
use ffxiv_versions_util::{DataFile, rw::read_csv_file};
use tokio::sync::OnceCell;
use url::Url;

mod thaliak;
use thaliak::{
	BaseGameRepositoriesResponse, BaseGameRepositoriesResponseVersion, get_thaliak_versions,
};

mod update_notice;
use update_notice::{UpdateNoticeInfo, check_versions_update_notices};

#[derive(Debug)]
struct Versions {
	global: OnceCell<Vec<Version>>,
	cn: OnceCell<Vec<Version>>,
	kr: OnceCell<Vec<Version>>,
	tw: OnceCell<Vec<Version>>,
}

impl Versions {
	pub async fn read_source_data_file(data_file: DataFile) -> Result<Vec<Version>> {
		let file_path = Path::new(env!("CARGO_MANIFEST_DIR"))
			.parent()
			.context("Manifest directory has no parent")?
			.join("data")
			.join("csv")
			.join(format!(
				"{file_name}.csv",
				file_name = data_file.file_prefix()
			));
		let file = tokio::fs::File::open(file_path).await?;

		read_csv_file(file).await
	}

	pub async fn get(&self, data_file: DataFile) -> &[Version] {
		let versions = match data_file {
			DataFile::Global => &self.global,
			DataFile::Cn => &self.cn,
			DataFile::Kr => &self.kr,
			DataFile::Tw => &self.tw,
		};
		versions
			.get_or_init(|| async {
				Self::read_source_data_file(data_file)
					.await
					.expect("Failed to read source data file")
			})
			.await
	}

	pub const fn new() -> Self {
		Self {
			global: OnceCell::const_new(),
			cn: OnceCell::const_new(),
			kr: OnceCell::const_new(),
			tw: OnceCell::const_new(),
		}
	}
}

type ThaliakVersion = BaseGameRepositoriesResponseVersion;

#[derive(Debug)]
struct ThaliakVersions {
	response: OnceCell<BaseGameRepositoriesResponse>,
}

impl ThaliakVersions {
	pub async fn get(&self, data_file: DataFile) -> Option<&[ThaliakVersion]> {
		let response = self
			.response
			.get_or_init(|| async {
				get_thaliak_versions()
					.await
					.expect("Failed to get versions from Thaliak")
			})
			.await;
		match data_file {
			DataFile::Global => Some(&response.global),
			DataFile::Cn => Some(&response.cn),
			DataFile::Kr => Some(&response.kr),
			DataFile::Tw => None,
		}
	}

	pub const fn new() -> Self {
		Self {
			response: OnceCell::const_new(),
		}
	}
}

#[derive(Debug)]
struct UpdateNoticeRegexes {
	pub global: OnceCell<update_notice::global::Regexes>,
	pub cn: OnceCell<update_notice::cn::Regexes>,
	pub kr: OnceCell<update_notice::kr::Regexes>,
	pub tw: OnceCell<update_notice::tw::Regexes>,
}

impl UpdateNoticeRegexes {
	pub fn compile_all() -> Result<Self> {
		Ok(Self {
			global: OnceCell::new_with(Some(update_notice::global::Regexes::compile_all()?)),
			cn: OnceCell::new_with(Some(update_notice::cn::Regexes::compile_all()?)),
			kr: OnceCell::new_with(Some(update_notice::kr::Regexes::compile_all()?)),
			tw: OnceCell::new_with(Some(update_notice::tw::Regexes::compile_all()?)),
		})
	}
}

#[derive(Debug)]
struct UpdateNotices {
	versions: Arc<Versions>,
	client: reqwest::Client,
	regexes: Arc<UpdateNoticeRegexes>,
	global: OnceCell<Vec<Option<UpdateNoticeInfo>>>, // This would be much simpler if TW had a patch notice for 7.0
	cn: OnceCell<Vec<Option<UpdateNoticeInfo>>>,
	kr: OnceCell<Vec<Option<UpdateNoticeInfo>>>,
	tw: OnceCell<Vec<Option<UpdateNoticeInfo>>>,
}

impl UpdateNotices {
	pub async fn get_update_notice_info(
		update_notice_url: Url,
		data_file: DataFile,
		client: reqwest::Client,
		regexes: Arc<UpdateNoticeRegexes>,
	) -> Result<UpdateNoticeInfo> {
		let response_text = client.get(update_notice_url).send().await?.text().await?;
		Ok(match data_file {
			DataFile::Global => {
				update_notice::global::parse_update_notice(
					&response_text,
					regexes.global.get().context("Regexes are not compiled")?,
					client.clone(),
				)
				.await?
			},
			DataFile::Cn => update_notice::cn::parse_update_notice(
				&response_text,
				regexes.cn.get().context("Regexes are not compiled")?,
			)?,
			DataFile::Kr => update_notice::kr::parse_update_notice(
				&response_text,
				regexes.kr.get().context("Regexes are not compiled")?,
			)?,
			DataFile::Tw => update_notice::tw::parse_update_notice(
				&response_text,
				regexes.tw.get().context("Regexes are not compiled")?,
			)?,
		})
	}

	async fn get_update_notices_info(
		&self,
		data_file: DataFile,
	) -> Result<Vec<Option<UpdateNoticeInfo>>> {
		let versions = self.versions.get(data_file).await;

		let mut update_notices = Vec::with_capacity(versions.len());
		for version in versions {
			let Some(update_notice_url) = &version.update_notice_url else {
				update_notices.push(None);
				continue;
			};
			let update_notice_url = match data_file {
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
			let client = self.client.clone();
			let regexes = self.regexes.clone();

			// MAYBE add back some parallelization? Doing it sequentially ensures that we don't spam the servers with requests.
			// Global returned a 429 without any throttling but is currently only ~2-3x slower than with a 250ms sleep between
			update_notices.push(Some(
				Self::get_update_notice_info(update_notice_url, data_file, client, regexes).await?,
			));
		}

		Ok(update_notices)
	}

	pub async fn get(&self, data_file: DataFile) -> &[Option<UpdateNoticeInfo>] {
		let update_notices = match data_file {
			DataFile::Global => &self.global,
			DataFile::Cn => &self.cn,
			DataFile::Kr => &self.kr,
			DataFile::Tw => &self.tw,
		};
		update_notices
			.get_or_init(|| async {
				self.get_update_notices_info(data_file)
					.await
					.expect("Failed to fetch update notice info")
			})
			.await
	}

	pub fn new(versions: Arc<Versions>) -> Result<Self> {
		const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

		let client = reqwest::Client::builder()
			.user_agent(USER_AGENT)
			.redirect(reqwest::redirect::Policy::none())
			.build()?;

		Ok(Self {
			versions,
			client,
			regexes: Arc::new(UpdateNoticeRegexes::compile_all()?),
			global: OnceCell::const_new(),
			cn: OnceCell::const_new(),
			kr: OnceCell::const_new(),
			tw: OnceCell::const_new(),
		})
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	env_logger::init();

	let versions = Arc::new(Versions::new());
	let thaliak_versions = Arc::new(ThaliakVersions::new());
	let update_notices = Arc::new(UpdateNotices::new(versions.clone())?);

	let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();

	for data_file in DataFile::all_files() {
		let versions = versions.clone();
		let thaliak_versions = thaliak_versions.clone();
		let update_notices = update_notices.clone();
		join_set.spawn(async move {
			let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();

			join_set.spawn(check_versions_basic(versions.clone(), data_file));
			join_set.spawn(check_versions_thaliak(
				versions.clone(),
				thaliak_versions.clone(),
				data_file,
			));
			join_set.spawn(check_versions_update_notices(
				versions.clone(),
				update_notices.clone(),
				data_file,
			));
			join_set.spawn(check_versions_timing(
				versions.clone(),
				thaliak_versions.clone(),
				update_notices.clone(),
				data_file,
			));

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

async fn check_versions_basic(versions: Arc<Versions>, data_file: DataFile) -> Result<()> {
	let versions = versions.get(data_file).await;
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

async fn check_versions_thaliak(
	versions: Arc<Versions>,
	thaliak_versions: Arc<ThaliakVersions>,
	data_file: DataFile,
) -> Result<()> {
	let versions = versions.get(data_file).await;
	let thaliak_versions = thaliak_versions.get(data_file).await;
	let Some(thaliak_versions) = thaliak_versions else {
		return Ok(()); // TW is not yet supported by Thaliak
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
					data_file == DataFile::Global
						&& thaliak_version.version_string == "2025.06.10.0000.0000"
				);
			}
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
			} else {
				// This is a workaround for HIST patch names not being parsed. MAYBE implement this for future stuff
				ensure!(thaliak_version.version_string.starts_with('H'));
			}
		}
		ensure!(
			seen_version,
			"Game version {} not seen by Thaliak on {}",
			version.game_version,
			version.release_date
		);
	}

	let latest_version = versions.last().context("Versions slice is empty")?;
	ensure!(
		!thaliak_versions.iter().any(|thaliak_version| {
			thaliak_version
				.version_string
				.parse::<GameVersion>()
				.is_ok_and(|game_version| game_version > latest_version.game_version)
		}),
		"Version {} is not the latest seen by Thaliak",
		latest_version.game_version
	);

	Ok(())
}

async fn check_versions_timing(
	versions: Arc<Versions>,
	thaliak_versions: Arc<ThaliakVersions>,
	update_notices: Arc<UpdateNotices>,
	data_file: DataFile,
) -> Result<()> {
	let versions = versions.get(data_file).await;
	let thaliak_versions = thaliak_versions.get(data_file).await;
	let Some(thaliak_versions) = thaliak_versions else {
		return Ok(()); // See above
	};
	for (version, update_notice_info) in versions.iter().zip(update_notices.get(data_file).await) {
		let thaliak_version = thaliak_versions
			.iter()
			.find(|thaliak_version| {
				thaliak_version.version_string == version.game_version.to_string()
			})
			.context("Thaliak's tracked versions is missing the game version")?;

		let Some(update_notice_info) = update_notice_info else {
			continue;
		};
		let time_delta = update_notice_info.datetime - *thaliak_version.first_seen;

		// Allowed time delta in hours relative to update_notice_info.datetime; TODO: test if this is working as expected
		let (allowed_margin_before, allowed_margin_after) =
			match (data_file, version.game_version.to_string().as_str()) {
				(DataFile::Cn, "2024.09.17.0000.0000") => (24, 0), // CN 7.0 (see other comments for more info)
				(DataFile::Cn, "2025.10.23.0000.0000") => (0, 25), // CN 7.35 ^
				(DataFile::Kr, "2024.11.19.0000.0000") => (27, 0), // KR 7.0
				(DataFile::Kr, _) => (10, 0), // KR patch timing seemingly (at least at the start) increases by 1h every patch
				_ => (4, 0),
			};
		ensure!(
			time_delta.num_hours() <= allowed_margin_before
				&& -time_delta.num_hours() <= allowed_margin_after,
			"Thaliak's first seen DateTime for version {} is too far away from the update notice's DateTime ({:?} vs {:?}; delta = {})",
			version.game_version,
			update_notice_info.datetime,
			*thaliak_version.first_seen,
			time_delta.num_hours(),
		);
	}

	Ok(())
}

#[test]
fn test_sanity_checks() -> Result<()> {
	main()
}
