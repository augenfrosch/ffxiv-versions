use anyhow::{Context, Result};
use graphql_client::{GraphQLQuery, Response};
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://thaliak.xiv.dev/graphql/2022-08-14";

#[derive(Debug)]
pub struct DateTime(chrono::DateTime<chrono::Utc>);

impl std::ops::Deref for DateTime {
	type Target = chrono::DateTime<chrono::Utc>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

// impl AsRef<chrono::DateTime<chrono::Utc>> for DateTime {
// 	fn as_ref(&self) -> &chrono::DateTime<chrono::Utc> {
// 		&self.0
// 	}
// }

impl Serialize for DateTime {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		let date_time = self.0;
		serializer.serialize_str(&format!("{date_time:?}"))
	}
}

impl<'de> Deserialize<'de> for DateTime {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		let dt = chrono::DateTime::<chrono::FixedOffset>::parse_from_rfc3339(&s)
			.map_err(|_| serde::de::Error::custom("DateTime could not be deserialized"))?;
		Ok(Self(dt.to_utc()))
	}
}

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "src/thaliak/2022-08-14.json",
	query_path = "src/thaliak/query.graphql",
	response_derives = "Debug"
)]
struct AllBaseGameRepositoriesQuery;

#[derive(Debug)]
pub struct BaseGameRepositoriesResponseVersion {
	pub version_string: String,
	pub first_seen: DateTime,
	pub first_offered: DateTime,
}

impl TryFrom<all_base_game_repositories_query::RepositoryVersionsVersions>
	for BaseGameRepositoriesResponseVersion
{
	type Error = anyhow::Error;

	fn try_from(
		value: all_base_game_repositories_query::RepositoryVersionsVersions,
	) -> std::result::Result<Self, Self::Error> {
		let all_base_game_repositories_query::RepositoryVersionsVersions {
			version_string,
			first_seen,
			first_offered,
		} = value;
		// This is a workaround for "2023.12.12.0000.0000" missing a `fistOffered`` timestamp and possible similar future issues
		let (first_seen, first_offered) = match (first_seen, first_offered) {
			(None, None) => anyhow::bail!("Missing both `firstSeen` & `first_offered` timestamps"),
			(None, Some(first_offered)) => (DateTime(first_offered.0.clone()), first_offered),
			(Some(first_seen), None) => (DateTime(first_seen.0.clone()), first_seen),
			(Some(first_seen), Some(first_offered)) => (first_seen, first_offered),
		};
		Ok(Self {
			version_string,
			first_seen: first_seen,
			first_offered: first_offered,
		})
	}
}

#[derive(Debug)]
pub struct BaseGameRepositoriesResponse {
	pub global: Vec<BaseGameRepositoriesResponseVersion>,
	pub cn: Vec<BaseGameRepositoriesResponseVersion>,
	pub kr: Vec<BaseGameRepositoriesResponseVersion>,
}

impl TryFrom<all_base_game_repositories_query::ResponseData> for BaseGameRepositoriesResponse {
	type Error = anyhow::Error;

	fn try_from(
		value: all_base_game_repositories_query::ResponseData,
	) -> std::result::Result<Self, Self::Error> {
		fn repository_versions_into(
			repository_versions: all_base_game_repositories_query::repositoryVersions,
		) -> Result<Vec<BaseGameRepositoriesResponseVersion>> {
			let mut versions = Vec::with_capacity(repository_versions.versions.len());
			for repositroy_version in repository_versions.versions.into_iter() {
				versions.push(repositroy_version.try_into()?);
			}
			Ok(versions)
		}
		let all_base_game_repositories_query::ResponseData { global, kr, cn } = value;
		Ok(Self {
			global: repository_versions_into(
				global.context("Missing global version's base game repository versions")?,
			)?,
			cn: repository_versions_into(
				cn.context("Missing CN version's base game repository versions")?,
			)?,
			kr: repository_versions_into(
				kr.context("Missing KR version's base game repository versions")?,
			)?,
		})
	}
}

pub async fn get_thaliak_versions() -> Result<BaseGameRepositoriesResponse> {
	let request_body =
		AllBaseGameRepositoriesQuery::build_query(all_base_game_repositories_query::Variables {});
	let client = reqwest::Client::new();
	let response: Response<all_base_game_repositories_query::ResponseData> = client
		.post(BASE_URL)
		.json(&request_body)
		.send()
		.await?
		.json()
		.await?;
	response
		.data
		.context("Responses of GraphQL API is missing data")?
		.try_into()
}
