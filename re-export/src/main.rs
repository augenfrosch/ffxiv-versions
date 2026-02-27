use std::{fmt::Display, path::Path, sync::Arc};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ffxiv_version_types::Version;
use tokio::io::AsyncWriteExt;

const FILES: [&str; 4] = ["global", "cn", "kr", "tw"];
const FILE_FORMATS: [FileFormat; 2] = [FileFormat::Csv, FileFormat::Json];

const BYTE_BUFFER_SIZE: usize = 1 << 14;

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

			let file = std::fs::File::open(file_path)?;

			let data = Arc::new(tokio::sync::RwLock::new(match args.source {
				FileFormat::Csv => read_csv_file(file),
				FileFormat::Json => read_json_file(file),
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

fn read_csv_file(file: std::fs::File) -> Result<Vec<Version>> {
	let mut data = Vec::new();
	let mut reader = csv::Reader::from_reader(file);
	let records_iter = reader.deserialize::<Version>();
	for record in records_iter {
		data.push(record?);
	}
	Ok(data)
}

async fn write_csv_file(mut file: tokio::fs::File, data: &[Version]) -> Result<()> {
	let mut buf = Vec::with_capacity(BYTE_BUFFER_SIZE);
	let mut writer = csv::Writer::from_writer(&mut buf);
	for version in data {
		writer.serialize(version)?;
	}
	drop(writer);
	buf.truncate(buf.len().saturating_sub(1)); // trailing '\n'
	file.write_all(&buf).await?;
	Ok(())
}

fn read_json_file(file: std::fs::File) -> Result<Vec<Version>> {
	Ok(serde_json::from_reader::<std::fs::File, Vec<Version>>(
		file,
	)?)
}

async fn write_json_file(mut file: tokio::fs::File, data: &[Version]) -> Result<()> {
	use serde::ser::Serialize;

	let mut buf = Vec::with_capacity(BYTE_BUFFER_SIZE);
	let mut serializer = serde_json::Serializer::with_formatter(
		&mut buf,
		serde_json::ser::PrettyFormatter::with_indent(b"\t"),
	);
	data.serialize(&mut serializer)?;
	file.write_all(&buf).await?;
	Ok(())
}
