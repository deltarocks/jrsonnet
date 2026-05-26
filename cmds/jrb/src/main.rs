use std::{
	path::{Path, PathBuf},
	process::exit,
};

use clap::{Parser, Subcommand};
use jrsonnet_pkg::{
	install,
	jsonnet_bundler::{GitSource, JsonnetFile},
};
use tracing::{debug, error, info, warn};

mod rewrite;

#[derive(Parser)]
#[clap(about = "A jsonnet package manager")]
struct Opts {
	/// The directory used to cache packages in.
	#[clap(long, default_value = "vendor")]
	jsonnetpkg_home: PathBuf,
	#[clap(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	/// Initialize a new empty jsonnetfile
	Init,
	/// Install new dependencies. Existing ones are silently skipped
	Install {
		/// Package URIs to install
		uris: Vec<String>,
		/// Show what would be done without making changes
		#[clap(long)]
		dry_run: bool,
	},
	/// Update all or specific dependencies
	Update {
		/// Package URIs to update (all if empty)
		uris: Vec<String>,
		/// Show what would be done without making changes
		#[clap(long)]
		dry_run: bool,
	},
	/// Remove dependencies by name
	Remove {
		/// Dependency names (matched against both canonical and legacy names)
		names: Vec<String>,
		/// Show what would be removed without making changes
		#[clap(long)]
		dry_run: bool,
	},
	/// Rewrite `legacyImports`-style import paths to their canonical equivalents
	Rewrite {
		/// Paths to scan (defaults to current directory)
		paths: Vec<PathBuf>,
		/// Show what would be rewritten without making changes
		#[clap(long)]
		dry_run: bool,
	},
}

const MANIFEST: &str = "jsonnetfile.json";
const LOCKFILE: &str = "jsonnetfile.lock.json";

fn load_manifest() -> JsonnetFile {
	let path = Path::new(MANIFEST);
	if path.exists() {
		JsonnetFile::load(path).unwrap_or_else(|e| {
			error!("failed to load {MANIFEST}: {e}");
			exit(1);
		})
	} else {
		JsonnetFile {
			version: 1,
			dependencies: Vec::new(),
			legacy_imports: true,
		}
	}
}

fn save_json(path: &Path, value: &impl serde::Serialize) {
	let json = serde_json::to_string_pretty(value).expect("serialization failed");
	std::fs::write(path, format!("{json}\n")).unwrap_or_else(|e| {
		error!("failed to write {}: {e}", path.display());
		exit(1);
	});
}

fn load_lockfile() -> Option<JsonnetFile> {
	let path = Path::new(LOCKFILE);
	if path.exists() {
		Some(JsonnetFile::load(path).unwrap_or_else(|e| {
			error!("failed to load {LOCKFILE}: {e}");
			exit(1);
		}))
	} else {
		None
	}
}

fn do_install(
	manifest: &JsonnetFile,
	lock: Option<&JsonnetFile>,
	vendor_dir: &Path,
	dry_run: bool,
) {
	let new_lock = install::install(manifest, lock, vendor_dir, dry_run).unwrap_or_else(|e| {
		error!("install failed: {e}");
		exit(1);
	});
	if !dry_run {
		save_json(Path::new(LOCKFILE), &new_lock);
	}
}

#[allow(clippy::too_many_lines)]
fn main() {
	tracing_subscriber::fmt().init();

	rustls::crypto::ring::default_provider()
		.install_default()
		.expect("install rustls crypto provider");

	let opts = Opts::parse();

	match opts.command {
		Command::Init => {
			let path = Path::new(MANIFEST);
			if path.exists() {
				warn!("{MANIFEST} already exists");
				exit(1);
			}
			let jf = JsonnetFile {
				version: 1,
				dependencies: Vec::new(),
				legacy_imports: true,
			};
			save_json(path, &jf);
		}
		Command::Install { uris, dry_run } => {
			let mut manifest = load_manifest();

			for uri in &uris {
				let dep = GitSource::parse(uri).unwrap_or_else(|| {
					eprintln!("failed to parse URI: {uri}");
					exit(1);
				});
				let is_new = !manifest.dependencies.iter().any(|d| {
					std::mem::discriminant(&d.source) == std::mem::discriminant(&dep.source)
						&& d.canonical_name() == dep.canonical_name()
				});
				if is_new {
					manifest.dependencies.push(dep);
				}
			}

			if !uris.is_empty() {
				save_json(Path::new(MANIFEST), &manifest);
			}

			let lock = load_lockfile();
			do_install(&manifest, lock.as_ref(), &opts.jsonnetpkg_home, dry_run);
		}
		Command::Update { uris, dry_run } => {
			let mut manifest = load_manifest();

			if !uris.is_empty() {
				for uri in &uris {
					let dep = GitSource::parse(uri).unwrap_or_else(|| {
						eprintln!("failed to parse URI: {uri}");
						exit(1);
					});
					if let Some(existing) = manifest
						.dependencies
						.iter_mut()
						.find(|d| d.canonical_name() == dep.canonical_name())
					{
						*existing = dep;
					} else {
						manifest.dependencies.push(dep);
					}
				}
				save_json(Path::new(MANIFEST), &manifest);
			}

			do_install(&manifest, None, &opts.jsonnetpkg_home, dry_run);
		}
		Command::Remove { names, dry_run } => {
			let mut manifest = load_manifest();

			let matched: Vec<_> = manifest
				.dependencies
				.iter()
				.filter(|dep| {
					names.iter().any(|name| {
						dep.canonical_name() == *name || dep.legacy_link_name() == *name
					})
				})
				.cloned()
				.collect::<Vec<_>>();

			if matched.is_empty() {
				eprintln!("no matching dependencies found");
				exit(1);
			}

			for dep in &matched {
				let canonical = dep.canonical_name();
				let dir = opts.jsonnetpkg_home.join(&canonical);
				let legacy = dep.legacy_link_name();
				let link = opts.jsonnetpkg_home.join(&legacy);
				if dry_run {
					info!("would remove: {canonical} ({})", dir.display());
				} else {
					info!("removing: {canonical}");
					if dir.exists() {
						let _ = std::fs::remove_dir_all(&dir);
					}
					if link.symlink_metadata().is_ok() {
						let _ = std::fs::remove_file(&link);
					}
				}
			}

			if !dry_run {
				manifest.dependencies.retain(|dep| {
					!names.iter().any(|name| {
						dep.canonical_name() == *name || dep.legacy_link_name() == *name
					})
				});
				save_json(Path::new(MANIFEST), &manifest);
				save_json(Path::new(LOCKFILE), &manifest);
			}
		}
		Command::Rewrite { paths, dry_run } => {
			let rules = rewrite::rules_from_vendor(&opts.jsonnetpkg_home);
			if rules.is_empty() {
				warn!(
					"no legacy symlinks found under {} — nothing to rewrite",
					opts.jsonnetpkg_home.display(),
				);
				return;
			}
			for r in &rules {
				debug!("rule: {} -> {}", r.legacy, r.canonical);
			}
			let scan_paths = if paths.is_empty() {
				vec![PathBuf::from(".")]
			} else {
				paths
			};
			let stats = rewrite::rewrite(&scan_paths, &opts.jsonnetpkg_home, &rules, dry_run);
			let verb = if dry_run { "would rewrite" } else { "rewrote" };
			info!(
				"scanned {} file(s); {verb} {} import(s) across {} file(s)",
				stats.files_scanned, stats.imports_rewritten, stats.files_modified,
			);
		}
	}
}
