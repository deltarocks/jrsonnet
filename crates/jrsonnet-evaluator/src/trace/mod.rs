#[cfg(feature = "explaining-traces")]
use std::cell::RefCell;
use std::{
	any::Any,
	path::{Path, PathBuf},
};

use jrsonnet_gcmodule::Trace;
use jrsonnet_ir::CodeLocation;
#[cfg(feature = "explaining-traces")]
use jrsonnet_ir::Span;

use crate::{error::ErrorKind, Error};

/// The way paths should be displayed
#[derive(Clone, Trace)]
pub enum PathResolver {
	/// Only filename
	FileName,
	/// Absolute path
	Absolute,
	/// Path relative to base directory
	Relative(PathBuf),
}

impl PathResolver {
	/// Will return `Self::Relative(cwd)`, or `Self::Absolute` on cwd failure
	pub fn new_cwd_fallback() -> Self {
		std::env::current_dir().map_or(Self::Absolute, Self::Relative)
	}
	pub fn resolve(&self, from: &Path) -> String {
		match self {
			Self::FileName => from
				.file_name()
				.expect("file name exists")
				.to_string_lossy()
				.into_owned(),
			Self::Absolute => from.to_string_lossy().into_owned(),
			Self::Relative(base) => {
				if from.is_relative() {
					return from.to_string_lossy().into_owned();
				}
				pathdiff::diff_paths(from, base)
					.expect("base is absolute")
					.to_string_lossy()
					.into_owned()
			}
		}
	}
}

/// Implements pretty-printing of traces
#[allow(clippy::module_name_repetitions)]
pub trait TraceFormat: Trace {
	fn write_trace(
		&self,
		out: &mut dyn std::fmt::Write,
		error: &Error,
	) -> Result<(), std::fmt::Error>;
	fn format(&self, error: &Error) -> Result<String, std::fmt::Error> {
		let mut out = String::new();
		self.write_trace(&mut out, error)?;
		Ok(out)
	}
	fn as_any(&self) -> &dyn Any;
	fn as_any_mut(&mut self) -> &mut dyn Any;
}

fn print_code_location(
	out: &mut impl std::fmt::Write,
	start: &CodeLocation,
	end: &CodeLocation,
) -> Result<(), std::fmt::Error> {
	if start.line == end.line {
		if start.column == end.column {
			write!(out, "{}:{}", start.line, end.column.saturating_sub(1))?;
		} else {
			write!(out, "{}:{}-{}", start.line, start.column - 1, end.column)?;
		}
	} else {
		write!(
			out,
			"{}:{}-{}:{}",
			start.line,
			end.column.saturating_sub(1),
			start.line,
			end.column
		)?;
	}
	Ok(())
}

/// vanilla-like jsonnet formatting
#[derive(Trace)]
pub struct CompactFormat {
	pub resolver: PathResolver,
	pub max_trace: usize,
	pub padding: usize,
}
impl Default for CompactFormat {
	fn default() -> Self {
		Self {
			resolver: PathResolver::Absolute,
			max_trace: 20,
			padding: 4,
		}
	}
}

impl TraceFormat for CompactFormat {
	fn write_trace(
		&self,
		out: &mut dyn std::fmt::Write,
		error: &Error,
	) -> Result<(), std::fmt::Error> {
		write!(out, "{}", error.error())?;
		if let ErrorKind::ImportSyntaxError { path, error } = error.error() {
			use std::fmt::Write;

			writeln!(out)?;
			let mut n = path.source_path().path().map_or_else(
				|| path.source_path().to_string(),
				|r| self.resolver.resolve(r),
			);
			let mut offset = error.location.1 as usize;
			let is_eof = if offset >= path.code().len() {
				offset = path.code().len().saturating_sub(1);
				true
			} else {
				false
			};
			#[expect(clippy::cast_possible_truncation, reason = "code is limited by 4gb")]
			let mut location = path
				.map_source_locations(&[offset as u32])
				.into_iter()
				.next()
				.unwrap();
			if is_eof {
				location.column += 1;
			}

			write!(n, ":").unwrap();
			print_code_location(&mut n, &location, &location).unwrap();
			write!(out, "{:<p$}{n}", "", p = self.padding)?;
		}
		let file_names = error
			.trace()
			.0
			.iter()
			.map(|el| &el.location)
			.map(|location| {
				use std::fmt::Write;
				#[allow(clippy::option_if_let_else)]
				if let Some(location) = location {
					let mut resolved_path = match location.0.source_path().path() {
						Some(r) => self.resolver.resolve(r),
						None => location.0.source_path().to_string(),
					};
					// TODO: Process all trace elements first
					let location = location.0.map_source_locations(&[location.1, location.2]);
					write!(resolved_path, ":").unwrap();
					print_code_location(&mut resolved_path, &location[0], &location[1]).unwrap();
					write!(resolved_path, ":").unwrap();
					Some(resolved_path)
				} else {
					None
				}
			})
			.collect::<Vec<_>>();
		let align = file_names
			.iter()
			.flatten()
			.map(String::len)
			.max()
			.unwrap_or(0);
		for (el, file) in error.trace().0.iter().zip(file_names) {
			writeln!(out)?;
			if let Some(file) = file {
				write!(
					out,
					"{:<p$}{:<w$} {}",
					"",
					file,
					el.desc,
					p = self.padding,
					w = align
				)?;
			} else {
				write!(out, "{:<p$}{}", "", el.desc, p = self.padding,)?;
			}
		}
		Ok(())
	}

	fn as_any(&self) -> &dyn Any {
		self
	}

	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}
}

#[derive(Trace)]
pub struct JsFormat {
	pub max_trace: usize,
}
impl TraceFormat for JsFormat {
	fn write_trace(
		&self,
		out: &mut dyn std::fmt::Write,
		error: &Error,
	) -> Result<(), std::fmt::Error> {
		write!(out, "{}", error.error())?;
		for item in &error.trace().0 {
			writeln!(out)?;
			let desc = &item.desc;
			if let Some(source) = &item.location {
				let start_end = source.0.map_source_locations(&[source.1, source.2]);
				let resolved_path = source.0.source_path().path().map_or_else(
					|| source.0.source_path().to_string(),
					|r| r.display().to_string(),
				);

				write!(
					out,
					"    at {} ({}:{}:{})",
					desc, resolved_path, start_end[0].line, start_end[0].column,
				)?;
			} else {
				write!(out, "    during {desc}")?;
			}
		}
		Ok(())
	}

	fn as_any(&self) -> &dyn Any {
		self
	}

	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}
}

#[cfg(feature = "explaining-traces")]
#[derive(Trace)]
pub struct HiDocFormat {
	pub resolver: PathResolver,
	pub max_trace: usize,
}
#[cfg(feature = "explaining-traces")]
impl TraceFormat for HiDocFormat {
	fn write_trace(
		&self,
		out: &mut dyn std::fmt::Write,
		error: &Error,
	) -> Result<(), std::fmt::Error> {
		struct ResetData {
			loc: Span,
		}
		use hi_doc::{source_to_ansi, Formatting, SnippetBuilder, Text};

		write!(out, "{}", error.error())?;
		if let ErrorKind::ImportSyntaxError { path, error } = error.error() {
			writeln!(out)?;
			let mut builder = SnippetBuilder::new(path.code());
			builder
				.error(Text::fragment("syntax error", Formatting::default()))
				.range(error.location.range())
				.build();
			let source = builder.build();
			let ansi = source_to_ansi(&source);
			write!(out, "{ansi}")?;
		}
		if let ErrorKind::StaticAnalysisError(diagnostics) = error.error() {
			use crate::analyze::DiagLevel;
			let mut builder: Option<SnippetBuilder> = None;
			let mut current_src: Option<&str> = None;
			let flush =
				|builder: Option<SnippetBuilder>, out: &mut dyn std::fmt::Write| -> Result<(), std::fmt::Error> {
					if let Some(b) = builder {
						let ansi = source_to_ansi(&b.build());
						write!(out, "\n{}", ansi.trim_end())?;
					}
					Ok(())
				};
			for diag in diagnostics {
				if let Some(span) = &diag.span {
					let src = span.0.code();
					if current_src != Some(src) {
						flush(builder.take(), out)?;
						builder = Some(SnippetBuilder::new(src));
						current_src = Some(src);
					}
					let b = builder.as_mut().unwrap();
					let ab = match diag.level {
						DiagLevel::Error => b.error(Text::fragment(
							diag.message.clone(),
							Formatting::default(),
						)),
						DiagLevel::Warning => b.warning(Text::fragment(
							diag.message.clone(),
							Formatting::default(),
						)),
					};
					ab.range(span.range()).build();
				} else {
					flush(builder.take(), out)?;
					current_src = None;
					let prefix = match diag.level {
						DiagLevel::Error => "error",
						DiagLevel::Warning => "warning",
					};
					write!(out, "\n{prefix}: {}", diag.message)?;
				}
			}
			flush(builder, out)?;
		}
		let trace = &error.trace();
		let snippet_builder: RefCell<Option<SnippetBuilder>> = RefCell::new(None);
		let mut last_location: Option<Span> = None;
		let mut flush_builder = |data: Option<ResetData>| {
			use std::fmt::Write;
			let mut out = String::new();
			let location_changed = if let Some(ResetData { loc }) = &data {
				if last_location.as_ref().map(|l| l.0.code()) != Some(loc.0.code()) {
					true
				} else if let (Some(last), new) = (&last_location, loc) {
					// Reverse condition if traceback
					last.1 > new.1 || last.2 > new.2
				} else {
					false
				}
			} else {
				true
			};
			if location_changed {
				if let Some(builder) = snippet_builder.borrow_mut().take() {
					let rendered = builder.build();
					let ansi = source_to_ansi(&rendered);
					if let Some(loc) = &last_location {
						let _ = writeln!(out, "...at {}", loc.0.source_path());
					}
					let _ = write!(out, "{}", ansi.trim_end());
				}
				last_location = None;

				if let Some(ResetData { loc }) = data {
					*snippet_builder.borrow_mut() = Some(SnippetBuilder::new(loc.0.code()));
					last_location = Some(loc);
				}
			}
			if out.is_empty() {
				return None;
			}
			Some(out)
		};
		for item in &trace.0 {
			let desc = &item.desc;
			if let Some(source) = &item.location {
				if let Some(flushed) = flush_builder(Some(ResetData {
					loc: source.clone(),
				})) {
					writeln!(out)?;
					write!(out, "{flushed}")?;
				}
				let mut builder = snippet_builder.borrow_mut();
				let builder = builder.as_mut().unwrap();
				builder
					.note(Text::fragment(desc, Formatting::default()))
					.range(source.1 as usize..=(source.2 as usize - 1).max(source.1 as usize))
					.build();
			} else {
				if let Some(flushed) = flush_builder(None) {
					writeln!(out)?;
					write!(out, "{flushed}")?;
				}
				writeln!(out)?;
				write!(out, "   {desc}")?;
			}
		}

		if let Some(flushed) = flush_builder(None) {
			writeln!(out)?;
			write!(out, "{flushed}")?;
		}
		Ok(())
	}

	fn as_any(&self) -> &dyn Any {
		self
	}

	fn as_any_mut(&mut self) -> &mut dyn Any {
		self
	}
}
