use std::str::FromStr;

use clap::Parser;
use jrsonnet_evaluator::{Result, tla::TlaArg, trace::PathResolver};
use jrsonnet_stdlib::ContextInitializer;

#[derive(Clone)]
pub struct ExtStr {
	pub name: String,
	pub value: String,
}

/// Parses a string like `name=<value>`, or `name` and reads value from env variable.
/// With no value it will be read from env variable.
/// If env variable is not found then it will be an error.
/// Value can contain `=` symbol.
///
/// ```
/// use std::str::FromStr;
/// use jrsonnet_cli::ExtStr;
///
/// let ext = ExtStr::from_str("name=value").unwrap();
/// assert_eq!(ext.name, "name");
/// assert_eq!(ext.value, "value");
///
/// let ext = ExtStr::from_str("name=value=with=equals").unwrap();
/// assert_eq!(ext.name, "name");
/// assert_eq!(ext.value, "value=with=equals");
/// ```
///
impl FromStr for ExtStr {
	type Err = &'static str;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		match s.find('=') {
			Some(idx) => Ok(Self {
				name: s[..idx].to_owned(),
				value: s[idx + 1..].to_owned(),
			}),
			None => Ok(Self {
				name: s.to_owned(),
				value: std::env::var(s).or(Err("missing env var"))?,
			}),
		}
	}
}

#[derive(Clone)]
pub struct ExtFile {
	pub name: String,
	pub path: String,
}

impl FromStr for ExtFile {
	type Err = String;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		let Some((name, path)) = s.split_once('=') else {
			return Err("bad ext-file syntax".to_owned());
		};
		Ok(Self {
			name: name.into(),
			path: path.into(),
		})
	}
}

#[derive(Parser)]
#[clap(next_help_heading = "STANDARD LIBRARY")]
pub struct StdOpts {
	/// Disable standard library.
	/// By default standard library will be available via global `std` variable.
	#[clap(long)]
	no_stdlib: bool,
	/// Add string external variable.
	/// External variables are globally available so it is preferred
	/// to use top level arguments whenever it's possible.
	/// If [=data] is not set then it will be read from `name` env variable.
	/// Can be accessed from code via `std.extVar("name")`.
	#[clap(long, short = 'V', name = "name[=var data]", number_of_values = 1)]
	ext_str: Vec<ExtStr>,
	/// Read string external variable from file.
	/// See also `--ext-str`
	#[clap(long, name = "name=var path", number_of_values = 1)]
	ext_str_file: Vec<ExtFile>,
	/// Add external variable from code.
	/// See also `--ext-str`
	#[clap(long, name = "name[=var source]", number_of_values = 1)]
	ext_code: Vec<ExtStr>,
	/// Read string external variable from file.
	/// See also `--ext-str`
	#[clap(long, name = "name=var code path", number_of_values = 1)]
	ext_code_file: Vec<ExtFile>,
}
impl StdOpts {
	pub fn context_initializer(&self) -> Result<Option<ContextInitializer>> {
		if self.no_stdlib {
			return Ok(None);
		}
		let ctx = ContextInitializer::new(PathResolver::new_cwd_fallback());
		for ext in &self.ext_str {
			ctx.settings_mut().ext_vars.insert(
				ext.name.as_str().into(),
				TlaArg::String(ext.value.as_str().into()),
			);
		}
		for ext in &self.ext_str_file {
			ctx.settings_mut().ext_vars.insert(
				ext.name.as_str().into(),
				TlaArg::ImportStr(ext.path.clone()),
			);
		}
		for ext in &self.ext_code {
			ctx.settings_mut().ext_vars.insert(
				ext.name.as_str().into(),
				TlaArg::InlineCode(ext.value.clone()),
			);
		}
		for ext in &self.ext_code_file {
			ctx.settings_mut()
				.ext_vars
				.insert(ext.name.as_str().into(), TlaArg::Import(ext.path.clone()));
		}
		Ok(Some(ctx))
	}
}
