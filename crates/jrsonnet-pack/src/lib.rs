//! Pack a jsonnet entry point and its transitive imports into a self-contained bundle
#![deny(missing_docs)]

use std::{
	collections::{HashMap, hash_map::Entry},
	path::{Component, Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use jrsonnet_evaluator::ImportResolver;
use jrsonnet_ir::SourcePath;
use jrsonnet_lexer::unescape;
use jrsonnet_rowan_parser::{
	AstNode, AstToken, LocatedSyntaxError,
	nodes::{ExprImport, ImportKindKind, Text, TextKind},
};
use serde::{Deserialize, Serialize};

/// Error that might occur during the bundle preparation
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Other error
	#[error("{0}")]
	Other(String),
	/// Source parse error
	// TODO: Include syntax error?
	#[error("parse error in {path}")]
	Parse {
		/// Path with parse error
		path: SourcePath,
		/// Parse errors
		errors: Vec<LocatedSyntaxError>,
	},
	/// Dynamic imports are not supported.
	#[error("unsupported dynamic import in {path}")]
	UnsupportedImportString {
		/// Stringified [`SourcePath`]
		path: SourcePath,
	},
	/// Only file-backed sources can be packed for now.
	#[error("{path}: source is not file-backed, can't bundle")]
	NotFileBacked {
		/// Stringified [`SourcePath`]
		path: String,
	},
	/// File is expected to be utf-8 encoded, but it is not.
	#[error("{0} was imported with {1:?}, but it is not utf-8 encoded")]
	NotUtf8(SourcePath, ImportKindKind),
}

impl From<String> for Error {
	fn from(s: String) -> Self {
		Error::Other(s)
	}
}

/// One file inside a [`PlaygroundBundle`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaygroundBundleFile {
	/// File name.
	pub name: String,
	/// File text content.
	pub content: String,
}

/// Bundle as accepted by jrsonnet playground
/// <https://delta.rocks/jrsonnet/playground>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaygroundBundle {
	/// Bundle format version; currently always `1`.
	pub version: u32,
	/// Name of the bundle's entry file (should be contained in `files`).
	pub entry: String,
	/// Optional output format hint for the playground.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub format: Option<String>,
	/// Files inside of the bundle.
	pub files: Vec<PlaygroundBundleFile>,
}

/// Bundle `entry` and all of its transitive imports into a [`PlaygroundBundle`].
///
/// `format` becomes the bundle's optional `format` field — the playground uses
/// it to pre-select the manifest format. Pass `None` to omit it.
pub fn bundle_playground(
	resolver: &dyn ImportResolver,
	entry: &SourcePath,
	format: Option<String>,
) -> Result<PlaygroundBundle, Error> {
	let walked = walk(resolver, entry)?;
	build_playground(&walked, format)
}

/// Bundle `entry` and all of its transitive imports into a single
/// self-contained jsonnet expression.
pub fn bundle_onefile(resolver: &dyn ImportResolver, entry: &SourcePath) -> Result<String, Error> {
	let walked = walk(resolver, entry)?;
	build_onefile(&walked)
}

#[derive(Debug)]
struct RawImport {
	start: usize,
	end: usize,
	decoded: String,
	kind: ImportKindKind,
}

#[derive(Debug)]
struct PendingRewrite {
	start: usize,
	end: usize,
	resolved: SourcePath,
	kind: ImportKindKind,
}

#[derive(Debug, Default)]
struct AccessFlags {
	normal: bool,
	str: bool,
	bin: bool,
}

#[derive(Debug)]
struct FileData {
	source: SourcePath,
	raw_text: Option<String>,
	raw_bytes: Vec<u8>,

	rewrites: Vec<PendingRewrite>,
	access: AccessFlags,
}

#[derive(Debug)]
struct Walked {
	files: HashMap<SourcePath, FileData>,
	entry: SourcePath,
}

fn walk(resolver: &dyn ImportResolver, entry: &SourcePath) -> Result<Walked, Error> {
	let mut files: HashMap<SourcePath, FileData> = HashMap::new();
	process(resolver, entry, ImportKindKind::ImportKw, &mut files)?;
	Ok(Walked {
		entry: entry.clone(),
		files,
	})
}

fn process(
	resolver: &dyn ImportResolver,
	source: &SourcePath,
	import_kind: ImportKindKind,
	files: &mut HashMap<SourcePath, FileData>,
) -> Result<(), Error> {
	let data = files.entry(source.clone());
	let data = match data {
		Entry::Vacant(e) => {
			let bytes = resolver
				.load_file_contents(source)
				.map_err(|e| Error::Other(format!("{e}")))?;

			e.insert(FileData {
				source: source.clone(),
				raw_text: None,
				raw_bytes: bytes,
				rewrites: Vec::new(),
				access: AccessFlags::default(),
			})
		}
		Entry::Occupied(v) => v.into_mut(),
	};

	if data.raw_text.is_none() && import_kind != ImportKindKind::ImportbinKw {
		let raw_text = String::from_utf8(data.raw_bytes.clone())
			.map_err(|_| Error::NotUtf8(source.clone(), import_kind))?;
		data.raw_text = Some(raw_text);
	}

	let imports = if import_kind == ImportKindKind::ImportKw && !data.access.normal {
		data.access.normal = true;
		let code = data.raw_text.as_ref().expect("set in the branch above");
		collect_imports(code).map_err(|e| match e {
			ImportScanError::Parse(errors) => Error::Parse {
				path: source.clone(),
				errors,
			},
			ImportScanError::Unsupported => Error::UnsupportedImportString {
				path: source.clone(),
			},
		})?
	} else {
		vec![]
	};

	if import_kind == ImportKindKind::ImportbinKw {
		data.access.bin = true;
	}
	if import_kind == ImportKindKind::ImportstrKw {
		data.access.str = true;
	}

	let mut rewrites = Vec::with_capacity(imports.len());
	let mut children: Vec<(SourcePath, ImportKindKind)> = Vec::with_capacity(imports.len());
	for imp in imports {
		let resolved = resolver
			.resolve_from(source, &imp.decoded.as_str())
			.map_err(|e| Error::Other(format!("{e}")))?;
		rewrites.push(PendingRewrite {
			start: imp.start,
			end: imp.end,
			resolved: resolved.clone(),
			kind: imp.kind,
		});
		children.push((resolved, imp.kind));
	}
	rewrites.sort_by_key(|r| r.start);
	data.rewrites = rewrites;

	for (child, kind) in children {
		process(resolver, &child, kind, files)?;
	}

	Ok(())
}

enum ImportScanError {
	Parse(Vec<LocatedSyntaxError>),
	Unsupported,
}

fn collect_imports(source: &str) -> Result<Vec<RawImport>, ImportScanError> {
	let (file, errs) = jrsonnet_rowan_parser::parse(source);
	if !errs.is_empty() {
		return Err(ImportScanError::Parse(errs));
	}
	let mut out = Vec::new();
	for node in file.syntax().descendants() {
		let Some(imp) = ExprImport::cast(node) else {
			continue;
		};
		let kind = imp
			.import_kind()
			.map_or(ImportKindKind::ImportKw, |k| k.kind());
		let Some(text) = imp.text() else {
			return Err(ImportScanError::Unsupported);
		};
		let Some(decoded) = decode_text(&text) else {
			return Err(ImportScanError::Unsupported);
		};
		let range = text.syntax().text_range();
		out.push(RawImport {
			start: range.start().into(),
			end: range.end().into(),
			decoded,
			kind,
		});
	}
	Ok(out)
}

/// Unescape/decode rowan [`Text`] node to the original text.
/// TODO: It does not work with string blocks as for now.
// TODO: Move to lexer or idk?
// TODO: String blocks (It is only used for imports for now, and it feels insane to use block imports)
pub fn decode_text(t: &Text) -> Option<String> {
	let raw = t.syntax().text();
	match t.kind() {
		TextKind::StringDouble | TextKind::StringSingle => {
			let inner = raw.get(1..raw.len() - 1)?;
			unescape(inner)
		}
		TextKind::StringDoubleVerbatim => {
			let inner = raw.get(2..raw.len() - 1)?;
			Some(inner.replace("\"\"", "\""))
		}
		TextKind::StringSingleVerbatim => {
			let inner = raw.get(2..raw.len() - 1)?;
			Some(inner.replace("''", "'"))
		}
		_ => None,
	}
}

/// Encode jsonnet string to the source form
pub fn quote_jsonnet(s: &str) -> String {
	let mut out = String::with_capacity(s.len() + 2);
	out.push('\'');
	for c in s.chars() {
		match c {
			'\\' => out.push_str("\\\\"),
			'\'' => out.push_str("\\'"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			c => out.push(c),
		}
	}
	out.push('\'');
	out
}

fn path_of(s: &SourcePath) -> Result<&Path, Error> {
	s.path().ok_or_else(|| Error::NotFileBacked {
		path: format!("{s}"),
	})
}

fn common_ancestor(parents: &[&Path]) -> PathBuf {
	let Some((first, rest)) = parents.split_first() else {
		return PathBuf::new();
	};
	let mut common: Vec<Component> = first.components().collect();
	for p in rest {
		let comps: Vec<Component> = p.components().collect();
		let n = common
			.iter()
			.zip(comps.iter())
			.take_while(|(a, b)| a == b)
			.count();
		common.truncate(n);
	}
	let mut out = PathBuf::new();
	for c in common {
		out.push(c.as_os_str());
	}
	out
}

fn bundle_name_from(root: &Path, path: &Path) -> String {
	let rel = path.strip_prefix(root).unwrap_or(path);
	rel.to_string_lossy().replace('\\', "/")
}

fn relative_bundle_path(from: &str, to: &str) -> String {
	let from_parts: Vec<&str> = from.split('/').collect();
	let from_dir = &from_parts[..from_parts.len().saturating_sub(1)];
	let to_parts: Vec<&str> = to.split('/').collect();
	let to_dir = &to_parts[..to_parts.len().saturating_sub(1)];

	let mut i = 0;
	while i < from_dir.len() && i < to_dir.len() && from_dir[i] == to_dir[i] {
		i += 1;
	}

	let mut out = String::new();
	for _ in i..from_dir.len() {
		out.push_str("../");
	}
	for seg in &to_parts[i..] {
		out.push_str(seg);
		out.push('/');
	}
	if out.ends_with('/') {
		out.pop();
	}
	if out.is_empty() {
		out.push_str(to_parts.last().copied().unwrap_or(""));
	}
	out
}

fn build_playground(walked: &Walked, format: Option<String>) -> Result<PlaygroundBundle, Error> {
	let canonical_paths: Vec<&Path> = walked
		.files
		.values()
		.map(|fd| path_of(&fd.source))
		.collect::<Result<_, _>>()?;
	let parents: Vec<&Path> = canonical_paths
		.iter()
		.map(|p| p.parent().unwrap_or_else(|| Path::new("")))
		.collect();
	let root = common_ancestor(&parents);

	let mut names: HashMap<SourcePath, String> = HashMap::new();
	for (key, fd) in &walked.files {
		names.insert(key.clone(), bundle_name_from(&root, path_of(&fd.source)?));
	}

	let entry_name = names
		.get(&walked.entry)
		.cloned()
		.ok_or_else(|| Error::Other("entry missing from bundle".into()))?;

	let mut files = Vec::with_capacity(walked.files.len());
	for (key, fd) in &walked.files {
		let self_name = names.get(key).expect("name");
		let Some(raw_text) = &fd.raw_text else {
			return Err(Error::Other(
				"playground does not support importbin".to_owned(),
			));
		};
		let content = rewrite_imports_for_playground(raw_text, &fd.rewrites, self_name, &names)?;
		files.push(PlaygroundBundleFile {
			name: self_name.clone(),
			content,
		});
	}
	files.sort_by(|a, b| a.name.cmp(&b.name));

	Ok(PlaygroundBundle {
		version: 1,
		entry: entry_name,
		format,
		files,
	})
}

fn rewrite_imports_for_playground(
	code: &str,
	rewrites: &[PendingRewrite],
	self_name: &str,
	names: &HashMap<SourcePath, String>,
) -> Result<String, Error> {
	let mut out = String::with_capacity(code.len());
	let mut cursor = 0usize;
	for r in rewrites {
		out.push_str(&code[cursor..r.start]);
		let target_name = names
			.get(&r.resolved)
			.ok_or_else(|| Error::Other(format!("missing bundle name for {}", r.resolved)))?;
		let rel = relative_bundle_path(self_name, target_name);
		out.push_str(&quote_jsonnet(&rel));
		cursor = r.end;
	}
	out.push_str(&code[cursor..]);
	Ok(out)
}

fn local_name(prefix: &str, idx: usize) -> String {
	format!("__bundle_{prefix}{idx}")
}

fn build_onefile(walked: &Walked) -> Result<String, Error> {
	let mut normal_idx: HashMap<SourcePath, usize> = HashMap::new();
	let mut str_idx: HashMap<SourcePath, usize> = HashMap::new();
	let mut bin_idx: HashMap<SourcePath, usize> = HashMap::new();
	for (key, fd) in &walked.files {
		if fd.access.normal {
			let n = normal_idx.len();
			normal_idx.insert(key.clone(), n);
		}
		if fd.access.str {
			let n = str_idx.len();
			str_idx.insert(key.clone(), n);
		}
		if fd.access.bin {
			let n = bin_idx.len();
			bin_idx.insert(key.clone(), n);
		}
	}

	let entry_idx = *normal_idx.get(&walked.entry).ok_or_else(|| {
		Error::Other("entry was not bound as a normal local; nothing to emit".into())
	})?;

	let mut bindings: Vec<String> =
		Vec::with_capacity(normal_idx.len() + str_idx.len() + bin_idx.len());

	for (key, &idx) in &normal_idx {
		let fd = &walked.files[key];
		let Some(raw_text) = fd.raw_text.as_ref() else {
			continue;
		};
		let body =
			rewrite_imports_for_onefile(raw_text, &fd.rewrites, &normal_idx, &str_idx, &bin_idx)?;
		let name = local_name("f", idx);
		bindings.push(format!("{name} = (\n{}\n  )", indent(&body, "    ")));
	}
	for (key, &idx) in &str_idx {
		let fd = &walked.files[key];
		let Some(raw_text) = fd.raw_text.as_ref() else {
			continue;
		};
		bindings.push(format!(
			"{} = {}",
			local_name("s", idx),
			quote_jsonnet(raw_text)
		));
	}
	for (key, &idx) in &bin_idx {
		let fd = &walked.files[key];
		bindings.push(format!(
			"{} = std.base64DecodeBytes({})",
			local_name("b", idx),
			quote_jsonnet(&STANDARD.encode(&fd.raw_bytes))
		));
	}

	let entry_local = local_name("f", entry_idx);
	let mut out = String::new();
	out.push_str("local\n");
	for (i, b) in bindings.iter().enumerate() {
		out.push_str("  ");
		out.push_str(b);
		if i + 1 == bindings.len() {
			out.push(';');
		} else {
			out.push(',');
		}
		out.push('\n');
	}
	out.push_str(&entry_local);
	out.push('\n');
	Ok(out)
}

fn indent(s: &str, prefix: &str) -> String {
	let mut out = String::with_capacity(s.len() + prefix.len() * 4);
	for (i, line) in s.lines().enumerate() {
		if i > 0 {
			out.push('\n');
		}
		if !line.is_empty() {
			out.push_str(prefix);
			out.push_str(line);
		}
	}
	out
}

fn rewrite_imports_for_onefile(
	code: &str,
	rewrites: &[PendingRewrite],
	normal_idx: &HashMap<SourcePath, usize>,
	str_idx: &HashMap<SourcePath, usize>,
	bin_idx: &HashMap<SourcePath, usize>,
) -> Result<String, Error> {
	let mut out = String::with_capacity(code.len());
	let mut cursor = 0usize;
	for r in rewrites {
		let (kw_start, _kw_end) = keyword_span_before(code, r.start)
			.ok_or_else(|| Error::Other("could not locate import keyword".into()))?;
		out.push_str(&code[cursor..kw_start]);
		match r.kind {
			ImportKindKind::ImportKw => {
				let idx = *normal_idx.get(&r.resolved).ok_or_else(|| {
					Error::Other(format!("missing normal local for {}", r.resolved))
				})?;
				out.push_str(&local_name("f", idx));
			}
			ImportKindKind::ImportstrKw => {
				let idx = *str_idx
					.get(&r.resolved)
					.ok_or_else(|| Error::Other(format!("missing str local for {}", r.resolved)))?;
				out.push_str(&local_name("s", idx));
			}
			ImportKindKind::ImportbinKw => {
				let idx = *bin_idx
					.get(&r.resolved)
					.ok_or_else(|| Error::Other(format!("missing bin local for {}", r.resolved)))?;
				out.push_str(&local_name("b", idx));
			}
		}
		cursor = r.end;
	}
	out.push_str(&code[cursor..]);
	Ok(out)
}

fn keyword_span_before(code: &str, pos: usize) -> Option<(usize, usize)> {
	let prefix = &code[..pos];
	let end = prefix.trim_end().len();
	if end == 0 {
		return None;
	}
	for kw in ["importbin", "importstr", "import"] {
		if end >= kw.len() && prefix[..end].ends_with(kw) {
			let start = end - kw.len();
			if start == 0 || !is_ident_continue(prefix.as_bytes()[start - 1]) {
				return Some((start, start + kw.len()));
			}
		}
	}
	None
}

fn is_ident_continue(b: u8) -> bool {
	b.is_ascii_alphanumeric() || b == b'_'
}
