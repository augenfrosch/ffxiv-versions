use std::{fmt::Display, path::Path, sync::Arc};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ffxiv_versions_util::rw::*;

const FILES: [&str; 4] = ["global", "cn", "kr", "tw"];
const FILE_FORMATS: [FileFormat; 2] = [FileFormat::Csv, FileFormat::Json];

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
	/// Source file format which is to be re-exportet to the other supported formats
	#[arg(short, long)]
	source: FileFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
enum FileFormat {
	Csv,
	Json,
}

impl Display for FileFormat {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			FileFormat::Csv => write!(f, "csv"),
			FileFormat::Json => write!(f, "json"),
		}
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();
	let source_file_format = args.source;
	let data_folder = Path::new(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.context("Manifest directory has no parent")?
		.join("data");

	let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();
	for file_name in FILES {
		let data_folder = data_folder.clone();
		join_set.spawn(async move {
			let file_path = data_folder
				.join(format!("{source_file_format}"))
				.join(format!("{file_name}.{source_file_format}"));

			let file = tokio::fs::File::open(file_path).await?;

			let data = Arc::new(tokio::sync::RwLock::new(match args.source {
				FileFormat::Csv => read_csv_file(file).await,
				FileFormat::Json => read_json_file(file).await,
			}?));

			let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();
			for target_file_format in FILE_FORMATS
				.iter()
				.filter(|format| **format != source_file_format)
			{
				let data_folder = data_folder.clone();
				let data = data.clone();
				join_set.spawn(async move {
					let file_path = data_folder
						.join(format!("{target_file_format}"))
						.join(format!("{file_name}.{target_file_format}"));
					let file = tokio::fs::File::create(file_path).await?;

					match target_file_format {
						FileFormat::Csv => write_csv_file(file, &data.read().await).await?,
						FileFormat::Json => write_json_file(file, &data.read().await).await?,
					}
					Ok(())
				});
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
