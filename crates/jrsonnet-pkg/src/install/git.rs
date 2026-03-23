#![allow(clippy::result_large_err)]

use std::{collections::HashSet, fs, path::Path};

use gix::{
	bstr::{self, ByteSlice},
	interrupt, progress,
	remote::{self, ref_map},
};
use tracing::info;

use super::{Error, LocalExtraction, ResolveResult, Result, VendorSource, cache_dir};
use crate::jsonnet_bundler::{Dependency, GitSource, JsonnetFile, Source, SubDir};

fn repo_cache_path(remote: &GitSource) -> Result<std::path::PathBuf> {
	Ok(cache_dir("git")?.join(&remote.host).join(&remote.repo))
}

fn ensure_repo(remote: &GitSource) -> Result<gix::Repository> {
	let cache_path = repo_cache_path(remote)?;

	if cache_path.exists() {
		if let Ok(repo) = gix::open(&cache_path) {
			fetch_remote(&repo, &remote.remote())?;
			return Ok(repo);
		}
		fs::remove_dir_all(&cache_path).map_err(|e| Error::Io(cache_path.clone(), e))?;
	}

	fs::create_dir_all(cache_path.parent().expect("has parent"))
		.map_err(|e| Error::Io(cache_path.clone(), e))?;

	let mut clone = gix::prepare_clone_bare(remote.remote(), &cache_path)?;
	let (repo, _) = clone.fetch_only(progress::Discard, &interrupt::IS_INTERRUPTED)?;
	fetch_remote(&repo, &remote.remote())?;

	Ok(repo)
}

fn fetch_remote(repo: &gix::Repository, remote: &str) -> Result<(), Error> {
	repo.remote_at(remote)?
		.with_refspecs(["+refs/*:refs/*"], remote::Direction::Fetch)?
		.connect(remote::Direction::Fetch)?
		.prepare_fetch(progress::Discard, ref_map::Options::default())?
		.receive(progress::Discard, &interrupt::IS_INTERRUPTED)?;
	Ok(())
}

fn extract_tree(
	repo: &gix::Repository,
	tree: &gix::Tree<'_>,
	subdir: &SubDir,
	dest: &Path,
) -> Result<(), Error> {
	let target_tree;
	let tree = if subdir.is_empty() {
		tree
	} else {
		let mut t = tree.clone();
		let entry = t
			.peel_to_entry_by_path(subdir.as_path().as_std_path())?
			.ok_or_else(|| Error::SubdirNotFound(subdir.to_string()))?;
		target_tree = entry.object()?.into_tree();
		&target_tree
	};

	let files = tree.traverse().breadthfirst.files()?;

	for entry in &files {
		if !entry.mode.is_blob() {
			continue;
		}
		let rel_path = entry
			.filepath
			.to_str()
			.map_err(|_| Error::InvalidPath(entry.filepath.to_string()))?;
		let file_path = dest.join(rel_path);

		if let Some(parent) = file_path.parent() {
			fs::create_dir_all(parent).map_err(|e| Error::Io(parent.to_owned(), e))?;
		}

		let blob = repo.find_object(entry.oid)?;
		fs::write(&file_path, &blob.data).map_err(|e| Error::Io(file_path, e))?;
	}

	Ok(())
}

fn resolve_version<'r>(repo: &'r gix::Repository, version: &str) -> Result<gix::Id<'r>> {
	let spec: &bstr::BStr = version.into();
	if let Ok(id) = repo.rev_parse_single(spec) {
		return Ok(id);
	}
	for prefix in ["refs/heads/", "refs/tags/"] {
		let refname = format!("{prefix}{version}");
		if let Ok(r) = repo.find_reference(&refname) {
			return Ok(r.into_fully_peeled_id()?);
		}
	}
	Ok(repo.rev_parse_single(spec)?)
}

fn read_blob_at_path(
	repo: &gix::Repository,
	tree: &gix::Tree<'_>,
	path: &SubDir,
) -> Option<Vec<u8>> {
	let mut t = tree.clone();
	let entry = t
		.peel_to_entry_by_path(path.as_path().as_std_path())
		.ok()??;
	let blob = repo.find_object(entry.oid()).ok()?;
	Some(blob.data.clone())
}

fn collect_tree_deps(
	repo: &gix::Repository,
	tree: &gix::Tree<'_>,
	dir: &SubDir,
	git_deps: &mut Vec<Dependency>,
	local_extractions: &mut Vec<LocalExtraction>,
	visited: &mut HashSet<SubDir>,
) {
	if !visited.insert(dir.clone()) {
		return;
	}

	let manifest_path = dir
		.join("jsonnetfile.json")
		.expect("appending a literal filename keeps it within parent");
	let Some(data) = read_blob_at_path(repo, tree, &manifest_path) else {
		return;
	};
	let Ok(manifest) = serde_json::from_slice::<JsonnetFile>(&data) else {
		return;
	};

	for dep in manifest.dependencies {
		match &dep.source {
			Source::Git(_) => git_deps.push(dep),
			Source::Local(local) => {
				let Ok(child_dir) = local.resolve_under(dir) else {
					info!("local source {local} escapes its package; skipping");
					continue;
				};
				let name = child_dir
					.file_name()
					.map_or_else(|| local.to_string(), str::to_owned);
				local_extractions.push(LocalExtraction {
					tree_path: child_dir.clone(),
					name,
				});
				collect_tree_deps(repo, tree, &child_dir, git_deps, local_extractions, visited);
			}
		}
	}
}

pub(super) fn resolve(
	git_source: &GitSource,
	version: Option<&str>,
) -> Result<ResolveResult, Error> {
	info!("fetching via git: {}", git_source.remote());
	let repo = ensure_repo(git_source)?;
	let id = match version {
		Some(v) => resolve_version(&repo, v)?,
		None => repo.head_id()?,
	};
	let commit = repo.find_object(id)?.into_commit();
	let tree = commit.tree()?;

	let mut transitive_git_deps = Vec::new();
	let mut local_extractions = Vec::new();
	let mut visited = HashSet::new();
	collect_tree_deps(
		&repo,
		&tree,
		&git_source.subdir,
		&mut transitive_git_deps,
		&mut local_extractions,
		&mut visited,
	);

	let repo_path = repo_cache_path(git_source)?;
	let sha = id.to_string();

	Ok(ResolveResult {
		version: sha.clone(),
		transitive_git_deps,
		local_extractions,
		source: VendorSource::GitTree {
			repo_path,
			commit_sha: sha,
			subdir: git_source.subdir.clone(),
		},
	})
}

pub(super) fn extract(
	repo_path: &Path,
	commit_sha: &str,
	subdir: &SubDir,
	dest: &Path,
) -> Result<(), Error> {
	let repo = gix::open(repo_path)?;
	let spec: &bstr::BStr = commit_sha.into();
	let id = repo.rev_parse_single(spec)?;
	let commit = repo.find_object(id)?.into_commit();
	let tree = commit.tree()?;
	extract_tree(&repo, &tree, subdir, dest)
}
