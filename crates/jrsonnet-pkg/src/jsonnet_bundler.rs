use std::{fmt, path::Path, str::FromStr};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize, de};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonnetFile {
	pub version: u32,
	#[serde(default)]
	pub dependencies: Vec<Dependency>,
	#[serde(default = "legacy_imports_default", rename = "legacyImports")]
	pub legacy_imports: bool,
}

fn legacy_imports_default() -> bool {
	true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
	pub source: Source,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub sum: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	#[serde(default, skip_serializing_if = "is_false")]
	pub single: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref, reason = "serde")]
fn is_false(v: &bool) -> bool {
	!v
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
	Git(GitSource),
	Local(LocalSource),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitScheme {
	Https,
	Ssh,
}

/// Wrapper over `Utf8PathBuf`, ensuring it can't escape to either an absolute
/// path or a parent directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubDir(Utf8PathBuf);

#[derive(Debug, thiserror::Error)]
#[error("subdir attempted to escape")]
pub struct SubDirEscapeError;

impl FromStr for SubDir {
	type Err = SubDirEscapeError;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Self::try_from(Utf8PathBuf::from(s))
	}
}
impl TryFrom<Utf8PathBuf> for SubDir {
	type Error = SubDirEscapeError;

	fn try_from(buf: Utf8PathBuf) -> Result<Self, Self::Error> {
		for ele in buf.components() {
			match ele {
				Utf8Component::Prefix(_) | Utf8Component::RootDir | Utf8Component::ParentDir => {
					return Err(SubDirEscapeError);
				}
				Utf8Component::CurDir | Utf8Component::Normal(_) => {}
			}
		}
		Ok(Self(buf))
	}
}

impl SubDir {
	pub fn empty() -> Self {
		Self(Utf8PathBuf::new())
	}
	pub fn as_str(&self) -> &str {
		self.0.as_str()
	}
	pub fn as_path(&self) -> &Utf8Path {
		&self.0
	}
	pub fn into_inner(self) -> Utf8PathBuf {
		self.0
	}
	pub fn join(&self, other: impl AsRef<Utf8Path>) -> Result<SubDir, SubDirEscapeError> {
		SubDir::try_from(self.0.join(other))
	}
	pub fn strip_prefix(&self, prefix: &SubDir) -> Option<SubDir> {
		Some(
			SubDir::try_from(self.0.strip_prefix(&prefix.0).ok()?.to_owned())
				.expect("stripping would not result in escape"),
		)
	}
	pub fn is_empty(&self) -> bool {
		self.0.as_str().is_empty()
	}
	pub fn file_name(&self) -> Option<&str> {
		self.0.file_name()
	}
	/// Strip a trailing `.git` extension, if any.
	#[must_use]
	pub fn without_git_suffix(&self) -> SubDir {
		let mut p = self.0.clone();
		if p.extension() == Some("git") {
			p.set_extension("");
		}
		SubDir(p)
	}
}
impl AsRef<Utf8Path> for SubDir {
	fn as_ref(&self) -> &Utf8Path {
		&self.0
	}
}
impl AsRef<Path> for SubDir {
	fn as_ref(&self) -> &Path {
		self.0.as_ref()
	}
}
impl AsRef<str> for SubDir {
	fn as_ref(&self) -> &str {
		self.0.as_str()
	}
}
impl fmt::Display for SubDir {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
impl PartialEq<str> for SubDir {
	fn eq(&self, other: &str) -> bool {
		self.0.as_str() == other
	}
}
impl PartialEq<&str> for SubDir {
	fn eq(&self, other: &&str) -> bool {
		self.0.as_str() == *other
	}
}

/// Wrapper over `String`, guaranteeing the value is a valid host: only ASCII
/// alphanumerics, dashes and dots, with at least one segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hostname(String);

#[derive(Debug, thiserror::Error)]
#[error("invalid hostname")]
pub struct InvalidHostnameError;

impl FromStr for Hostname {
	type Err = InvalidHostnameError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.is_empty() || s == "." || s == ".." {
			return Err(InvalidHostnameError);
		}
		for seg in s.split('.') {
			if seg.is_empty() {
				return Err(InvalidHostnameError);
			}
			if !seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
				return Err(InvalidHostnameError);
			}
		}
		Ok(Self(s.to_owned()))
	}
}

impl Hostname {
	pub fn as_str(&self) -> &str {
		&self.0
	}
}
impl AsRef<str> for Hostname {
	fn as_ref(&self) -> &str {
		&self.0
	}
}
impl AsRef<Path> for Hostname {
	fn as_ref(&self) -> &Path {
		self.0.as_ref()
	}
}
impl AsRef<Utf8Path> for Hostname {
	fn as_ref(&self) -> &Utf8Path {
		self.0.as_str().into()
	}
}
impl fmt::Display for Hostname {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}
impl PartialEq<str> for Hostname {
	fn eq(&self, other: &str) -> bool {
		self.0 == other
	}
}
impl PartialEq<&str> for Hostname {
	fn eq(&self, other: &&str) -> bool {
		self.0 == *other
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSource {
	pub scheme: GitScheme,
	pub host: Hostname,
	/// Repo path relative to host: `user/repo[.git]` (or with subgroups).
	pub repo: SubDir,
	/// Subdirectory within the repo. Empty means the repo root.
	pub subdir: SubDir,
}

/// A relative path that may climb out of its package via `..` parts, but only
/// at the head - once you go down (`SubDir` portion) you can't go back up.
///
/// The total upward count is bounded only at resolution time, against the
/// containing package's depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSource {
	pub ups: usize,
	pub dir: SubDir,
}

impl FromStr for LocalSource {
	// Technically incorrect, as it only rejects mid-path ../'s...
	type Err = SubDirEscapeError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut ups = 0usize;
		let mut rest = s;
		loop {
			if let Some(r) = rest.strip_prefix("./") {
				rest = r;
			} else if rest == "." {
				rest = "";
				break;
			} else if let Some(r) = rest.strip_prefix("../") {
				ups = ups.checked_add(1).expect("can't be longer than s length");
				rest = r;
			} else if rest == ".." {
				ups = ups.checked_add(1).expect("can't be longer than s length");
				rest = "";
				break;
			} else {
				break;
			}
		}
		Ok(Self {
			ups,
			dir: SubDir::from_str(rest)?,
		})
	}
}

impl fmt::Display for LocalSource {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let mut out = String::with_capacity(self.ups * 3 + self.dir.as_str().len());
		for _ in 0..self.ups {
			out.push_str("../");
		}
		out.push_str(self.dir.as_str());
		if out.is_empty() {
			out.push('.');
		} else if out.ends_with('/') {
			out.pop();
		}
		// TODO: I didn't finish
		f.write_str(&out)
	}
}

impl LocalSource {
	pub fn resolve_under(&self, parent: &SubDir) -> Result<SubDir, SubDirEscapeError> {
		let mut comps: Vec<&str> = parent.as_path().components().map(|c| c.as_str()).collect();
		if self.ups > comps.len() {
			return Err(SubDirEscapeError);
		}
		comps.truncate(comps.len() - self.ups);
		let mut buf = Utf8PathBuf::from_iter(comps);
		buf.push(self.dir.as_path());
		SubDir::try_from(buf)
	}
}

impl Serialize for LocalSource {
	fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
		#[derive(Serialize)]
		struct JsonLocal<'a> {
			directory: &'a str,
		}
		let rendered = self.to_string();
		JsonLocal {
			directory: &rendered,
		}
		.serialize(ser)
	}
}

impl<'de> Deserialize<'de> for LocalSource {
	fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
		#[derive(Deserialize)]
		struct JsonLocal {
			directory: String,
		}
		let j = JsonLocal::deserialize(de)?;
		LocalSource::from_str(&j.directory)
			.map_err(|e| de::Error::custom(format!("invalid local path {:?}: {e}", j.directory)))
	}
}

impl GitSource {
	/// Repo path with the trailing `.git` (if any) stripped.
	pub fn plain_repo_name(&self) -> SubDir {
		self.repo.without_git_suffix()
	}

	/// Canonical install path: `host/user/repo[/subdir]`.
	pub fn name(&self) -> SubDir {
		let mut p = Utf8PathBuf::from(self.host.as_str());
		p.push(self.plain_repo_name());
		if !self.subdir.is_empty() {
			p.push(self.subdir.as_path());
		}
		SubDir::try_from(p).expect("host + subdirs is a valid SubDir")
	}

	/// Last path component of `repo[/subdir]`, used as the legacy symlink name.
	pub fn legacy_name(&self) -> String {
		self.name()
			.file_name()
			.expect("name has at least one component")
			.to_owned()
	}

	/// Git remote URL for cloning.
	pub fn remote(&self) -> String {
		let host = self.host.as_str();
		let repo = self.repo.as_str();
		match self.scheme {
			GitScheme::Ssh => format!("ssh://git@{host}/{repo}"),
			GitScheme::Https => format!("https://{host}/{repo}"),
		}
	}

	/// Parse a URI like `github.com/user/repo/subdir@version` into a
	/// `Dependency`.
	pub fn parse(uri: &str) -> Option<Dependency> {
		git_uri::parse(uri).ok()
	}
}

peg::parser! {
	grammar git_uri() for str {
		rule host_segment() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '-']+;
		rule host() -> Hostname
			= s:$(host_segment()++".")
			{ Hostname::from_str(s).expect("grammar restricted to valid host chars") }

		// User/repo path segments. `~` is allowed for Bitbucket personal repos.
		rule path_segment() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '~']+;
		// Subdir segments allow dots (e.g. `ksonnet.beta.3`).
		rule subdir_segment() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.']+;

		// `user[/group...]/repo.git`
		rule repo_dotgit() -> SubDir
			= s:$(path_segment()++"/" ".git")
			{ SubDir::from_str(s).expect("grammar restricted to subpath chars") }
		// `user/repo` (exactly two segments, no `.git`)
		rule repo_simple() -> SubDir
			= s:$(path_segment() "/" path_segment())
			{ SubDir::from_str(s).expect("grammar restricted to subpath chars") }

		// Subdir starts with `/`. May be empty.
		rule subdir() -> SubDir
			= "/" s:$(subdir_segment() ** "/") "/"?
			{ SubDir::from_str(s).expect("grammar restricted to subdir chars") }
			/ { SubDir::empty() }

		rule version() -> &'input str
			= "@" v:$([_]+) { v }


		// git@host:path.git[/subdir][@version]  (SCP style)
		rule scp_uri() -> Dependency
			= "git@" h:host() ":" repo:repo_dotgit() subdir:subdir()
			  v:version()?
		{
			make_dep(GitScheme::Ssh, h, repo, subdir, v)
		}

		// ssh://git@host/path.git[/subdir][@version]
		rule ssh_uri() -> Dependency
			= "ssh://git@" h:host() "/" repo:repo_dotgit() subdir:subdir()
			  v:version()?
		{
			make_dep(GitScheme::Ssh, h, repo, subdir, v)
		}

		// [https://]host/path.git[/subdir][@version]
		rule https_dotgit() -> Dependency
			= "https://"? h:host() "/" repo:repo_dotgit() subdir:subdir()
			  v:version()?
		{
			make_dep(GitScheme::Https, h, repo, subdir, v)
		}

		// [https://]host/user/repo[/subdir[/...]][@version]
		rule https_simple() -> Dependency
			= "https://"? h:host() "/" repo:repo_simple() subdir:subdir()
			  v:version()?
		{
			make_dep(GitScheme::Https, h, repo, subdir, v)
		}

		pub rule parse() -> Dependency
			= ssh_uri() / scp_uri() / https_dotgit() / https_simple()
	}
}

fn make_dep(
	scheme: GitScheme,
	host: Hostname,
	repo: SubDir,
	subdir: SubDir,
	version: Option<&str>,
) -> Dependency {
	Dependency {
		source: Source::Git(GitSource {
			scheme,
			host,
			repo,
			subdir,
		}),
		version: version.map(str::to_owned),
		sum: None,
		name: None,
		single: false,
	}
}

impl Dependency {
	/// Canonical install path for deduplication and vendor extraction.
	pub fn canonical_name(&self) -> Utf8PathBuf {
		match &self.source {
			Source::Git(git) => git.name().into_inner(),
			Source::Local(local) => Utf8PathBuf::from(local.to_string()),
		}
	}

	/// Legacy symlink name: `dep.name` override, or last path component.
	pub fn legacy_link_name(&self) -> String {
		if let Some(name) = &self.name {
			return name.clone();
		}
		match &self.source {
			Source::Git(git) => git.legacy_name(),
			Source::Local(local) => local
				.dir
				.file_name()
				.map_or_else(|| local.to_string(), str::to_owned),
		}
	}
}

impl Serialize for GitSource {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		#[derive(Serialize)]
		struct JsonGit<'a> {
			remote: String,
			#[serde(skip_serializing_if = "str::is_empty")]
			subdir: &'a str,
		}
		JsonGit {
			remote: self.remote(),
			subdir: self.subdir.as_str(),
		}
		.serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for GitSource {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		#[derive(Deserialize)]
		struct JsonGit {
			remote: String,
			#[serde(default)]
			subdir: String,
		}
		let j = JsonGit::deserialize(deserializer)?;

		let parsed = GitSource::parse(&j.remote)
			.ok_or_else(|| de::Error::custom(format!("unable to parse git url {:?}", j.remote)))?;
		let Source::Git(mut gs) = parsed.source else {
			unreachable!()
		};

		if !j.subdir.is_empty() {
			gs.subdir = SubDir::from_str(j.subdir.trim_start_matches('/'))
				.map_err(|e| de::Error::custom(format!("invalid subdir {:?}: {e}", j.subdir)))?;
		}

		Ok(gs)
	}
}

impl JsonnetFile {
	pub fn load(path: &Path) -> Result<Self, Error> {
		let data = std::fs::read(path).map_err(|e| Error::Io(path.to_owned(), e))?;
		serde_json::from_slice(&data).map_err(Error::Json)
	}
}

#[derive(Debug)]
pub enum Error {
	Io(std::path::PathBuf, std::io::Error),
	Json(serde_json::Error),
}
impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Error::Io(path, e) => write!(f, "{}: {e}", path.display()),
			Error::Json(e) => write!(f, "{e}"),
		}
	}
}
impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
	use super::*;

	fn host(s: &str) -> Hostname {
		Hostname::from_str(s).expect("test host")
	}
	fn sd(s: &str) -> SubDir {
		SubDir::from_str(s).expect("test subdir")
	}

	#[test]
	fn parse_basic() {
		let input = r#"{
			"version": 1,
			"dependencies": [
				{
					"source": {
						"git": {
							"remote": "https://github.com/grafana/jsonnet-libs.git",
							"subdir": "grafana-builder"
						}
					},
					"version": "54865853ebc1f901964e25a2e7a0e4d2cb6b9648",
					"sum": "ELsYwK+kGdzX1mee2Yy+/b2mdO4Y503BOCDkFzwmGbE="
				}
			],
			"legacyImports": false
		}"#;

		let jf: JsonnetFile = serde_json::from_str(input).unwrap();
		assert_eq!(jf.version, 1);
		assert!(!jf.legacy_imports);
		assert_eq!(jf.dependencies.len(), 1);

		let dep = &jf.dependencies[0];
		let Source::Git(git) = &dep.source else {
			panic!("expected git source");
		};
		assert_eq!(git.host, "github.com");
		assert_eq!(git.repo, "grafana/jsonnet-libs.git");
		assert_eq!(git.subdir, "grafana-builder");
		assert_eq!(
			git.name(),
			"github.com/grafana/jsonnet-libs/grafana-builder"
		);
		assert_eq!(git.legacy_name(), "grafana-builder");
		assert_eq!(git.remote(), "https://github.com/grafana/jsonnet-libs.git");
		assert_eq!(
			dep.version.as_deref(),
			Some("54865853ebc1f901964e25a2e7a0e4d2cb6b9648")
		);
	}

	#[test]
	fn parse_local_source() {
		let input = r#"{
			"version": 1,
			"dependencies": [
				{
					"source": {
						"local": { "directory": "../shared-lib" }
					},
					"version": ""
				}
			]
		}"#;

		let jf: JsonnetFile = serde_json::from_str(input).unwrap();
		let dep = &jf.dependencies[0];
		let Source::Local(local) = &dep.source else {
			panic!("expected local source");
		};
		assert_eq!(local.ups, 1);
		assert_eq!(local.dir, "shared-lib");
		assert_eq!(local.to_string(), "../shared-lib");
		assert!(jf.legacy_imports);
	}

	#[test]
	fn parse_uri_github_slug() {
		let dep = GitSource::parse("github.com/ksonnet/ksonnet-lib/ksonnet.beta.3").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.scheme, GitScheme::Https);
		assert_eq!(gs.host, "github.com");
		assert_eq!(gs.repo, "ksonnet/ksonnet-lib");
		assert_eq!(gs.subdir, "ksonnet.beta.3");
		assert_eq!(dep.version, None);
		assert_eq!(gs.remote(), "https://github.com/ksonnet/ksonnet-lib");
	}

	#[test]
	fn parse_uri_ssh() {
		let dep = GitSource::parse("ssh://git@example.com/user/repo.git/foobar@v1").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.scheme, GitScheme::Ssh);
		assert_eq!(gs.host, "example.com");
		assert_eq!(gs.repo, "user/repo.git");
		assert_eq!(gs.subdir, "foobar");
		assert_eq!(dep.version.as_deref(), Some("v1"));
		assert_eq!(gs.remote(), "ssh://git@example.com/user/repo.git");
	}

	#[test]
	fn parse_uri_scp() {
		let dep = GitSource::parse("git@my.host:user/repo.git/foobar@v1").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.scheme, GitScheme::Ssh);
		assert_eq!(gs.host, "my.host");
		assert_eq!(gs.subdir, "foobar");
		assert_eq!(dep.version.as_deref(), Some("v1"));
		assert_eq!(gs.remote(), "ssh://git@my.host/user/repo.git");
	}

	#[test]
	fn parse_uri_https_explicit() {
		let dep = GitSource::parse("https://example.com/foo/bar").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.scheme, GitScheme::Https);
		assert_eq!(gs.host, "example.com");
		assert_eq!(gs.repo, "foo/bar");
		assert_eq!(gs.subdir, "");
		assert_eq!(gs.remote(), "https://example.com/foo/bar");
	}

	#[test]
	fn parse_uri_no_scheme() {
		let dep = GitSource::parse("example.com/foo/bar").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.scheme, GitScheme::Https);
		assert_eq!(gs.host, "example.com");
		assert_eq!(gs.remote(), "https://example.com/foo/bar");
	}

	#[test]
	fn parse_uri_path_and_version() {
		let dep = GitSource::parse("example.com/foo/bar/baz@bat").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.repo, "foo/bar");
		assert_eq!(gs.subdir, "baz");
		assert_eq!(dep.version.as_deref(), Some("bat"));
	}

	#[test]
	fn parse_uri_version_only() {
		let dep = GitSource::parse("example.com/foo/bar@baz").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.repo, "foo/bar");
		assert_eq!(gs.subdir, "");
		assert_eq!(dep.version.as_deref(), Some("baz"));
	}

	#[test]
	fn parse_uri_deep_path() {
		let dep = GitSource::parse("example.com/foo/bar/baz/bat").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.repo, "foo/bar");
		assert_eq!(gs.subdir, "baz/bat");
	}

	#[test]
	fn parse_uri_subgroups() {
		let dep = GitSource::parse("example.com/group/subgroup/repository.git").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.repo, "group/subgroup/repository.git");
		assert_eq!(gs.plain_repo_name(), "group/subgroup/repository");
		assert_eq!(gs.subdir, "");
		assert_eq!(
			gs.remote(),
			"https://example.com/group/subgroup/repository.git"
		);
	}

	#[test]
	fn parse_uri_subgroup_subdir() {
		let dep = GitSource::parse("example.com/group/subgroup/repository.git/subdir").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.plain_repo_name(), "group/subgroup/repository");
		assert_eq!(gs.subdir, "subdir");
	}

	#[test]
	fn parse_uri_bitbucket_personal() {
		let dep = GitSource::parse("bitbucket.org/~user/repository.git").unwrap();
		let Source::Git(gs) = &dep.source else {
			panic!()
		};
		assert_eq!(gs.host, "bitbucket.org");
		assert_eq!(gs.repo, "~user/repository.git");
		assert_eq!(gs.remote(), "https://bitbucket.org/~user/repository.git");
	}

	#[test]
	fn name_with_subdir() {
		let gs = GitSource {
			scheme: GitScheme::Https,
			host: host("github.com"),
			repo: sd("ksonnet/ksonnet-lib"),
			subdir: sd("ksonnet.beta.3"),
		};
		assert_eq!(gs.name(), "github.com/ksonnet/ksonnet-lib/ksonnet.beta.3");
		assert_eq!(gs.legacy_name(), "ksonnet.beta.3");
	}

	#[test]
	fn name_without_subdir() {
		let gs = GitSource {
			scheme: GitScheme::Https,
			host: host("github.com"),
			repo: sd("user/repo"),
			subdir: SubDir::empty(),
		};
		assert_eq!(gs.name(), "github.com/user/repo");
		assert_eq!(gs.legacy_name(), "repo");
	}

	#[test]
	fn defaults() {
		let input = r#"{ "version": 1 }"#;
		let jf: JsonnetFile = serde_json::from_str(input).unwrap();
		assert!(jf.dependencies.is_empty());
		assert!(jf.legacy_imports);
	}

	#[test]
	fn roundtrip() {
		let jf = JsonnetFile {
			version: 1,
			dependencies: vec![Dependency {
				source: Source::Git(GitSource {
					scheme: GitScheme::Https,
					host: host("github.com"),
					repo: sd("user/repo"),
					subdir: sd("lib"),
				}),
				version: Some("main".into()),
				sum: None,
				name: None,
				single: false,
			}],
			legacy_imports: false,
		};
		let json = serde_json::to_string_pretty(&jf).unwrap();
		let parsed: JsonnetFile = serde_json::from_str(&json).unwrap();
		assert_eq!(parsed.dependencies.len(), 1);
		let Source::Git(gs) = &parsed.dependencies[0].source else {
			panic!()
		};
		assert_eq!(gs.host, "github.com");
		assert_eq!(gs.repo, "user/repo");
		assert_eq!(gs.subdir, "lib");
	}

	#[test]
	fn hostname_rejects_slash() {
		assert!(Hostname::from_str("foo/bar").is_err());
		assert!(Hostname::from_str("").is_err());
		assert!(Hostname::from_str(".").is_err());
		assert!(Hostname::from_str("..").is_err());
		assert!(Hostname::from_str(".foo").is_err());
		assert!(Hostname::from_str("foo.").is_err());
		assert!(Hostname::from_str("foo..bar").is_err());
		assert!(Hostname::from_str("foo bar").is_err());
		assert!(Hostname::from_str("foo.bar").is_ok());
	}

	#[test]
	fn subdir_rejects_escape() {
		assert!(SubDir::from_str("../foo").is_err());
		assert!(SubDir::from_str("/foo").is_err());
		assert!(SubDir::from_str("foo/../bar").is_err());
		assert!(SubDir::from_str("foo/bar").is_ok());
		assert!(SubDir::from_str("").is_ok());
	}

	#[test]
	fn local_source_parse() {
		let l = LocalSource::from_str("../shared-lib").unwrap();
		assert_eq!(l.ups, 1);
		assert_eq!(l.dir, "shared-lib");

		let l = LocalSource::from_str("../../foo/bar").unwrap();
		assert_eq!(l.ups, 2);
		assert_eq!(l.dir, "foo/bar");

		let l = LocalSource::from_str("./foo").unwrap();
		assert_eq!(l.ups, 0);
		assert_eq!(l.dir, "foo");

		let l = LocalSource::from_str(".").unwrap();
		assert_eq!(l.ups, 0);
		assert!(l.dir.is_empty());

		let l = LocalSource::from_str("..").unwrap();
		assert_eq!(l.ups, 1);
		assert!(l.dir.is_empty());

		// Mid-path `..` is rejected.
		assert!(LocalSource::from_str("foo/../bar").is_err());
		// Absolute path is rejected.
		assert!(LocalSource::from_str("/foo").is_err());
	}

	#[test]
	fn local_source_render_roundtrip() {
		for s in ["../shared-lib", "../../foo/bar", "foo", "."] {
			assert_eq!(LocalSource::from_str(s).unwrap().to_string(), s);
		}
	}

	#[test]
	fn local_source_resolve_under() {
		// `../foo` from `pkg/sub` lands at `pkg/foo`.
		let l = LocalSource::from_str("../foo").unwrap();
		assert_eq!(l.resolve_under(&sd("pkg/sub")).unwrap(), "pkg/foo");

		// Plain `foo` from `pkg/sub` lands at `pkg/sub/foo`.
		let l = LocalSource::from_str("foo").unwrap();
		assert_eq!(l.resolve_under(&sd("pkg/sub")).unwrap(), "pkg/sub/foo");

		// Too many `..` escapes the parent.
		let l = LocalSource::from_str("../../../foo").unwrap();
		assert!(l.resolve_under(&sd("pkg")).is_err());
	}
}
