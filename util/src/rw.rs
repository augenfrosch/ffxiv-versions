use anyhow::Result;
use ffxiv_versions_types::Version;
use serde::ser::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const BYTE_BUFFER_SIZE: usize = 1 << 14;

pub async fn read_csv_file(mut file: tokio::fs::File) -> Result<Vec<Version>> {
	let mut buf = Vec::with_capacity(BYTE_BUFFER_SIZE);
	file.read_to_end(&mut buf).await?;
	let mut data = Vec::new();
	let mut reader = csv::Reader::from_reader(buf.as_slice());
	let records_iter = reader.deserialize::<Version>();
	for record in records_iter {
		data.push(record?);
	}
	Ok(data)
}

pub async fn write_csv_file(mut file: tokio::fs::File, data: &[Version]) -> Result<()> {
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

pub async fn read_json_file(mut file: tokio::fs::File) -> Result<Vec<Version>> {
	let mut buf = Vec::with_capacity(BYTE_BUFFER_SIZE);
	file.read_to_end(&mut buf).await?;
	Ok(serde_json::from_reader::<&[u8], Vec<Version>>(&buf)?)
}

pub async fn write_json_file(mut file: tokio::fs::File, data: &[Version]) -> Result<()> {
	let mut buf = Vec::with_capacity(BYTE_BUFFER_SIZE);
	let mut serializer = serde_json::Serializer::with_formatter(
		&mut buf,
		serde_json::ser::PrettyFormatter::with_indent(b"\t"),
	);
	data.serialize(&mut serializer)?;
	file.write_all(&buf).await?;
	Ok(())
}
