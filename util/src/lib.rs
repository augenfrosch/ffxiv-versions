#![allow(clippy::missing_errors_doc)] // TODO: documentation (if anyone else actually uses this; probably also shouldn't use anyhow then)

pub mod rw;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataFile {
	Global,
	Cn,
	Kr,
	Tw,
}

impl DataFile {
	#[must_use]
	pub fn file_prefix(self) -> &'static str {
		match self {
			DataFile::Global => "global",
			DataFile::Cn => "cn",
			DataFile::Kr => "kr",
			DataFile::Tw => "tw",
		}
	}

	#[must_use]
	pub fn all_files() -> impl IntoIterator<Item = DataFile> {
		[Self::Global, Self::Cn, Self::Kr, Self::Tw]
	}
}
