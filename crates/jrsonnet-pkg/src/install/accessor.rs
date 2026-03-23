use std::{
	fs::File,
	io::{self, Read},
	result,
	str::FromStr as _,
	sync::Mutex,
};

use tracing::warn;
use zip::{ZipArchive, result::ZipError};

use crate::jsonnet_bundler::{SubDir, SubDirEscapeError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error(transparent)]
	Zip(#[from] ZipError),
	#[error("invalid prefixed archive")]
	ZipInvalidPrefix,
	#[error("zip io: {0}")]
	ZipIo(io::Error),
	#[error("subdir not found: {0}")]
	SubDirNotFound(SubDir),
	#[error(transparent)]
	SubdirEscape(#[from] SubDirEscapeError),
}
type Result<T, E = Error> = result::Result<T, E>;

pub trait SourceAccessor {}

pub struct ZipFileAccessor {
	archive: Mutex<ZipArchive<File>>,
	// Github archives have top-level directory with repo name
	prefix: SubDir,
}

impl ZipFileAccessor {
	pub fn new_prefixed(file: File) -> Result<Self> {
		let archive = ZipArchive::new(file)?;
		let prefix = archive.name_for_index(0).ok_or(Error::ZipInvalidPrefix)?;

		Ok(Self {
			prefix: SubDir::from_str(prefix)?,
			archive: Mutex::new(archive),
		})
	}
	/// Read a file from inside the archive's logical root (after stripping the
	/// github-style `<repo>-<sha>/` prefix).
	#[allow(clippy::significant_drop_tightening, reason = "false-positive")]
	pub fn read(&self, name: &SubDir) -> Result<Option<Vec<u8>>> {
		let prefixed = self
			.prefix
			.join(name)
			.expect("prefix and name are both subdirs");
		let mut archive = self.archive.lock().expect("not poisoned");
		let mut v = match archive.by_name(prefixed.as_str()) {
			Ok(v) => v,
			Err(ZipError::FileNotFound) => return Ok(None),
			Err(e) => return Err(e.into()),
		};
		if !v.is_file() {
			return Ok(None);
		}
		let mut out = Vec::new();
		v.read_to_end(&mut out).map_err(Error::ZipIo)?;
		Ok(Some(out))
	}
	#[allow(clippy::significant_drop_tightening, reason = "false-positive")]
	pub fn iter<E>(
		&self,
		subdir: &SubDir,
		cb: &mut dyn FnMut(SubDir, AccessorEntry) -> Result<(), E>,
	) -> Result<(), E>
	where
		E: From<Error>,
	{
		let mut archive = self.archive.lock().expect("not poisoned");
		let len = archive.len();

		let mut found = false;
		for i in 0..len {
			let mut entry = archive.by_index(i).map_err(Error::from)?;
			let raw = entry.name();
			let Ok(full_name) = SubDir::from_str(raw) else {
				warn!("invalid zip entry name: {raw}");
				continue;
			};
			// Peel off the github-archive top-level `<repo>-<sha>/` prefix.
			let Some(in_repo) = full_name.strip_prefix(&self.prefix) else {
				continue;
			};
			let Some(name) = in_repo.strip_prefix(subdir) else {
				continue;
			};
			found = true;
			if name.is_empty() && entry.is_dir() {
				continue;
			}

			cb(
				name.clone(),
				if entry.is_dir() {
					AccessorEntry::Dir
				} else if entry.is_file() {
					let mut data = Vec::new();
					entry.read_to_end(&mut data).map_err(Error::ZipIo)?;
					AccessorEntry::File(data)
				} else {
					// TODO: Symlinks?
					panic!("unknown accessor entry type: {name:?}")
				},
			)?;
		}

		if !found {
			return Err(Error::SubDirNotFound(subdir.clone()).into());
		}

		Ok(())
	}
	pub fn len(&self) -> usize {
		self.archive.lock().expect("not poisoned").len()
	}
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}
}

pub enum AccessorEntry {
	Dir,
	File(Vec<u8>),
}
