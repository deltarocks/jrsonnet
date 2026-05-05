use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use jrsonnet_evaluator::manifest::{
	JsonFormat, ManifestFormat, StringFormat, ToStringFormat, YamlStreamFormat,
};
use jrsonnet_stdlib::{IniFormat, TomlFormat, XmlJsonmlFormat, YamlFormat};

#[derive(Clone, Copy, ValueEnum)]
pub enum ManifestFormatName {
	/// Expect string as output, and write them directly
	String,
	Json,
	Yaml,
	Toml,
	XmlJsonml,
	Ini,
}

#[derive(Parser)]
#[clap(next_help_heading = "MANIFESTIFICATION OUTPUT")]
pub struct ManifestOpts {
	/// Output format, wraps resulting value to corresponding std.manifest call
	///
	/// [default: json, yaml when -y is used]
	#[clap(long, short = 'f')]
	format: Option<ManifestFormatName>,
	/// Expect plain string as output.
	/// Mutually exclusive with `--format`
	#[clap(long, short = 'S', conflicts_with = "format")]
	string: bool,
	/// Write output as YAML stream, can be used with --format json/yaml
	#[clap(long, short = 'y', conflicts_with = "string")]
	yaml_stream: bool,
	/// Number of spaces to pad output manifest with.
	/// `0` for hard tabs, `-1` for single line output
	///
	/// [default: 3 for json, 2 for yaml/toml]
	#[clap(long)]
	line_padding: Option<usize>,
	/// No not add a trailing newline to the output
	#[clap(long)]
	pub no_trailing_newline: bool,
	/// Preserve order in object manifestification
	#[cfg(feature = "exp-preserve-order")]
	#[clap(long)]
	pub preserve_order: bool,
}
impl ManifestOpts {
	pub fn manifest_format(&self) -> Box<dyn ManifestFormat> {
		let format: Box<dyn ManifestFormat> = if self.string {
			Box::new(StringFormat)
		} else {
			#[cfg(feature = "exp-preserve-order")]
			let preserve_order = self.preserve_order;
			let format = match self.format {
				Some(v) => v,
				None if self.yaml_stream => ManifestFormatName::Yaml,
				None => ManifestFormatName::Json,
			};
			match format {
				ManifestFormatName::String => Box::new(ToStringFormat),
				ManifestFormatName::Json => Box::new(JsonFormat::cli(
					self.line_padding.unwrap_or(3),
					#[cfg(feature = "exp-preserve-order")]
					preserve_order,
				)),
				ManifestFormatName::Yaml => Box::new(YamlFormat::cli(
					self.line_padding.unwrap_or(2),
					#[cfg(feature = "exp-preserve-order")]
					preserve_order,
				)),
				ManifestFormatName::Toml => Box::new(TomlFormat::cli(
					self.line_padding.unwrap_or(2),
					#[cfg(feature = "exp-preserve-order")]
					preserve_order,
				)),
				ManifestFormatName::XmlJsonml => Box::new(XmlJsonmlFormat::cli()),
				ManifestFormatName::Ini => Box::new(IniFormat::cli(
					#[cfg(feature = "exp-preserve-order")]
					preserve_order,
				)),
			}
		};
		if self.yaml_stream {
			Box::new(YamlStreamFormat::cli(format))
		} else {
			format
		}
	}
}

#[derive(Parser)]
pub struct OutputOpts {
	/// Write to the output file rather than stdout
	#[clap(long, short = 'o')]
	pub output_file: Option<PathBuf>,
	/// Automatically creates all parent directories for files
	#[clap(long, short = 'c')]
	pub create_output_dirs: bool,
	/// Write multiple files to the directory, list files on stdout
	#[clap(long, short = 'm')]
	pub multi: Option<PathBuf>,
}
