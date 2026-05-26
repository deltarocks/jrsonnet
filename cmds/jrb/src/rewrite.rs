use std::{
	cmp::Reverse,
	fs,
	path::{Path, PathBuf},
};

use jrsonnet_pack::{decode_text, quote_jsonnet};
use jrsonnet_rowan_parser::{AstNode, AstToken, nodes::ExprImport};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct Rule {
	pub legacy: String,
	pub canonical: String,
}

pub fn rules_from_vendor(vendor_dir: &Path) -> Vec<Rule> {
	let mut rules = Vec::new();
	let entries = match fs::read_dir(vendor_dir) {
		Ok(e) => e,
		Err(e) => {
			warn!("read_dir {}: {e}", vendor_dir.display());
			return rules;
		}
	};
	for entry in entries.flatten() {
		let path = entry.path();
		let Ok(meta) = entry.file_type() else {
			continue;
		};
		if !meta.is_symlink() {
			continue;
		}
		let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
			continue;
		};
		let target = match fs::read_link(&path) {
			Ok(t) => t,
			Err(e) => {
				warn!("readlink {}: {e}", path.display());
				continue;
			}
		};
		let target_str = target.to_string_lossy().replace('\\', "/");
		let canonical = target_str.trim_end_matches('/').to_owned();
		if canonical.is_empty() || canonical.starts_with('/') {
			continue;
		}
		rules.push(Rule {
			legacy: name.to_owned(),
			canonical,
		});
	}
	rules.sort_by_key(|b| Reverse(b.legacy.len()));
	rules
}

pub struct Stats {
	pub files_scanned: usize,
	pub files_modified: usize,
	pub imports_rewritten: usize,
}

fn apply_rules(name: &str, rules: &[Rule]) -> Option<String> {
	for r in rules {
		if name == r.legacy {
			return Some(r.canonical.clone());
		}
		if let Some(rest) = name.strip_prefix(&format!("{}/", r.legacy)) {
			return Some(format!("{}/{}", r.canonical, rest));
		}
	}
	None
}

fn rewrite_source(source: &str, rules: &[Rule]) -> Option<(String, usize)> {
	let (file, errs) = jrsonnet_rowan_parser::parse(source);
	if !errs.is_empty() {
		return None;
	}
	let mut edits: Vec<(usize, usize, String)> = Vec::new();
	for node in file.syntax().descendants() {
		let Some(imp) = ExprImport::cast(node) else {
			continue;
		};
		let Some(text) = imp.text() else { continue };
		let Some(decoded) = decode_text(&text) else {
			continue;
		};
		let Some(new_value) = apply_rules(&decoded, rules) else {
			continue;
		};
		let range = text.syntax().text_range();
		let start: usize = range.start().into();
		let end: usize = range.end().into();
		edits.push((start, end, quote_jsonnet(&new_value)));
	}
	if edits.is_empty() {
		return None;
	}
	edits.sort_by_key(|(s, _, _)| *s);
	let count = edits.len();
	let mut out = String::with_capacity(source.len());
	let mut cursor = 0usize;
	for (start, end, replacement) in edits {
		out.push_str(&source[cursor..start]);
		out.push_str(&replacement);
		cursor = end;
	}
	out.push_str(&source[cursor..]);
	Some((out, count))
}

fn is_jsonnet_file(path: &Path) -> bool {
	matches!(
		path.extension().and_then(|e| e.to_str()),
		Some("jsonnet" | "libsonnet" | "TEMPLATE")
	)
}

fn collect_files(root: &Path, skip: &Path, out: &mut Vec<PathBuf>) {
	if root == skip {
		return;
	}
	let meta = match fs::symlink_metadata(root) {
		Ok(m) => m,
		Err(e) => {
			warn!("stat {}: {e}", root.display());
			return;
		}
	};
	let file_type = meta.file_type();
	if file_type.is_symlink() {
		return;
	}
	if file_type.is_dir() {
		let entries = match fs::read_dir(root) {
			Ok(e) => e,
			Err(e) => {
				warn!("read_dir {}: {e}", root.display());
				return;
			}
		};
		for entry in entries.flatten() {
			collect_files(&entry.path(), skip, out);
		}
		return;
	}
	if file_type.is_file() && is_jsonnet_file(root) {
		out.push(root.to_owned());
	}
}

pub fn rewrite(paths: &[PathBuf], vendor_dir: &Path, rules: &[Rule], dry_run: bool) -> Stats {
	let skip = vendor_dir
		.canonicalize()
		.unwrap_or_else(|_| vendor_dir.to_path_buf());
	let mut files = Vec::new();
	for p in paths {
		let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
		collect_files(&canon, &skip, &mut files);
	}
	files.sort();
	files.dedup();

	let mut stats = Stats {
		files_scanned: files.len(),
		files_modified: 0,
		imports_rewritten: 0,
	};
	for file in &files {
		let source = match fs::read_to_string(file) {
			Ok(s) => s,
			Err(e) => {
				error!("read {}: {e}", file.display());
				continue;
			}
		};
		let Some((new_source, count)) = rewrite_source(&source, rules) else {
			debug!("unchanged: {}", file.display());
			continue;
		};
		stats.files_modified += 1;
		stats.imports_rewritten += count;
		if dry_run {
			info!("would rewrite {} import(s) in {}", count, file.display());
		} else {
			match fs::write(file, &new_source) {
				Ok(()) => info!("rewrote {} import(s) in {}", count, file.display()),
				Err(e) => error!("write {}: {e}", file.display()),
			}
		}
	}
	stats
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rules() -> Vec<Rule> {
		vec![Rule {
			legacy: "kube-prometheus".into(),
			canonical: "github.com/prometheus-operator/kube-prometheus/jsonnet/kube-prometheus"
				.into(),
		}]
	}

	#[test]
	fn rewrites_prefix() {
		let src = "import 'kube-prometheus/main.libsonnet'\n";
		let (out, n) = rewrite_source(src, &rules()).expect("rewrite");
		assert_eq!(n, 1);
		assert_eq!(
			out,
			"import 'github.com/prometheus-operator/kube-prometheus/jsonnet/kube-prometheus/main.libsonnet'\n"
		);
	}

	#[test]
	fn rewrites_exact_match() {
		let src = "import \"kube-prometheus\"\n";
		let (out, _n) = rewrite_source(src, &rules()).expect("rewrite");
		assert!(
			out.contains(
				"'github.com/prometheus-operator/kube-prometheus/jsonnet/kube-prometheus'"
			)
		);
	}

	#[test]
	fn leaves_unrelated_imports_alone() {
		let src = "import 'something/else.libsonnet'\n";
		assert!(rewrite_source(src, &rules()).is_none());
	}

	#[test]
	fn does_not_match_substring() {
		let src = "import 'kube-prometheus-extra/foo.libsonnet'\n";
		assert!(rewrite_source(src, &rules()).is_none());
	}
}
