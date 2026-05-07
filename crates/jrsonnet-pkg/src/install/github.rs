#![allow(clippy::result_large_err)]

use std::{
	collections::HashSet,
	fs::{self, File},
	io::Write as _,
	path::{Path, PathBuf},
};

use camino::Utf8PathBuf;
use reqwest::{blocking::Response, header};
use tracing::{debug, info, warn};

use super::{
	Error, LocalExtraction, ResolveResult, Result, VendorSource,
	accessor::{AccessorEntry, ZipFileAccessor},
	make_symlink,
};
use crate::{
	install::{PKG_USER_AGENT, cache_dir},
	jsonnet_bundler::{Dependency, GitSource, JsonnetFile, Source, SubDir},
};

fn is_sha(s: &str) -> bool {
	s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn commit_cache_path(source: &GitSource, sha: &str) -> Result<PathBuf> {
	Ok(cache_dir("github")?
		.join(source.plain_repo_name())
		.join(format!("{sha}.zip")))
}

fn resolve_sha(source: &GitSource, version: &str) -> Result<String> {
	let url = format!(
		"https://api.github.com/repos/{}/commits/{}",
		source.plain_repo_name(),
		version
	);
	let response = reqwest::blocking::Client::new()
		.get(&url)
		.header(header::ACCEPT, "application/vnd.github.sha")
		.header(header::USER_AGENT, PKG_USER_AGENT)
		.send()
		.and_then(Response::error_for_status)?;
	let sha = response.text()?;
	Ok(sha.trim().to_owned())
}

fn fetch_zip(source: &GitSource, sha: &str) -> Result<ZipFileAccessor> {
	let cached = commit_cache_path(source, sha)?;
	if cached.exists() {
		debug!("using cached archive {}", cached.display());
		return Ok(ZipFileAccessor::new_prefixed(
			File::open(&cached).map_err(|e| Error::Io(cached.clone(), e))?,
		)?);
	}

	let url = format!(
		"https://github.com/{}/archive/{}.zip",
		source.plain_repo_name(),
		sha
	);
	info!("downloading {url}");

	let bytes = reqwest::blocking::Client::new()
		.get(&url)
		.header(header::USER_AGENT, PKG_USER_AGENT)
		.send()
		.and_then(Response::error_for_status)?
		.bytes()?;

	if let Some(parent) = cached.parent() {
		fs::create_dir_all(parent).map_err(|e| Error::Io(parent.to_owned(), e))?;
	}
	let mut downloaded = File::create_new(&cached).map_err(|e| Error::Io(cached.clone(), e))?;
	downloaded
		.write_all(&bytes)
		.map_err(|e| Error::Io(cached.clone(), e))?;

	Ok(ZipFileAccessor::new_prefixed(downloaded)?)
}

fn open_cached_zip(zip_path: &Path) -> Result<ZipFileAccessor> {
	Ok(ZipFileAccessor::new_prefixed(
		File::open(zip_path).map_err(|e| Error::Io(zip_path.to_owned(), e))?,
	)?)
}

fn extract_subdir(archive: &ZipFileAccessor, subdir: &SubDir, dest: &Path) -> Result<()> {
	archive.iter(subdir, &mut |name, entry| {
		let target = dest.join(&name);
		match entry {
			AccessorEntry::Dir => {
				fs::create_dir_all(&target).map_err(|e| Error::Io(target, e))?;
			}
			AccessorEntry::File(data) => {
				if let Some(parent) = target.parent() {
					fs::create_dir_all(parent).map_err(|e| Error::Io(parent.to_owned(), e))?;
				}
				fs::write(&target, &data).map_err(|e| Error::Io(target, e))?;
			}
			AccessorEntry::Symlink(link_target) => {
				let symlink_parent = name
					.as_path()
					.parent()
					.map(|p| SubDir::try_from(Utf8PathBuf::from(p)))
					.transpose()
					.expect("parent of a SubDir is a SubDir")
					.unwrap_or_else(SubDir::empty);
				if link_target.resolve_under(&symlink_parent).is_err() {
					warn!("symlink {name} -> {link_target} escapes extraction; skipping");
					return Ok(());
				}
				if let Some(parent) = target.parent() {
					fs::create_dir_all(parent).map_err(|e| Error::Io(parent.to_owned(), e))?;
				}
				make_symlink(&link_target.to_string(), &target)
					.map_err(|e| Error::Io(target, e))?;
			}
		}
		Ok(())
	})
}

fn collect_archive_deps(
	archive: &ZipFileAccessor,
	dir: &SubDir,
	git_deps: &mut Vec<Dependency>,
	local_extractions: &mut Vec<LocalExtraction>,
	visited: &mut HashSet<SubDir>,
) -> Result<()> {
	if !visited.insert(dir.clone()) {
		return Ok(());
	}

	let manifest_path = dir
		.join("jsonnetfile.json")
		.expect("appending a literal filename keeps it within parent");

	let Some(data) = archive.read(&manifest_path)? else {
		return Ok(());
	};
	let Ok(manifest) = serde_json::from_slice::<JsonnetFile>(&data) else {
		return Ok(());
	};

	for dep in manifest.dependencies {
		match &dep.source {
			Source::Git(_) => git_deps.push(dep),
			Source::Local(local) => {
				let Ok(child_dir) = local.resolve_under(dir) else {
					tracing::info!("local source {local} escapes its package; skipping");
					continue;
				};
				let name = child_dir
					.file_name()
					.map_or_else(|| local.to_string(), str::to_owned);
				local_extractions.push(LocalExtraction {
					tree_path: child_dir.clone(),
					name,
				});
				collect_archive_deps(archive, &child_dir, git_deps, local_extractions, visited)?;
			}
		}
	}
	Ok(())
}

pub(super) fn resolve(source: &GitSource, version: Option<&str>) -> Result<ResolveResult> {
	let version_str = version.unwrap_or("HEAD");
	let sha = if is_sha(version_str) {
		version_str.to_owned()
	} else {
		let resolved = resolve_sha(source, version_str)?;
		info!("resolved {version_str} to {resolved}");
		resolved
	};

	let archive = fetch_zip(source, &sha)?;

	let mut transitive_git_deps = Vec::new();
	let mut local_extractions = Vec::new();
	let mut visited = HashSet::new();
	collect_archive_deps(
		&archive,
		&source.subdir,
		&mut transitive_git_deps,
		&mut local_extractions,
		&mut visited,
	)?;

	let zip_path = commit_cache_path(source, &sha)?;

	Ok(ResolveResult {
		version: sha.clone(),
		transitive_git_deps,
		local_extractions,
		source: VendorSource::GithubZip {
			zip_path,
			commit_sha: sha,
			subdir: source.subdir.clone(),
		},
	})
}

pub(super) fn extract(zip_path: &Path, subdir: &SubDir, dest: &Path) -> Result<()> {
	let archive = open_cached_zip(zip_path)?;
	extract_subdir(&archive, subdir, dest)
}
