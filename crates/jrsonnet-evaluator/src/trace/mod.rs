#[cfg(feature = "explaining-traces")]
use std::cell::RefCell;
use std::{
	any::Any,
	fmt,
	path::{Component, Path, PathBuf},
};

use jrsonnet_gcmodule::Trace;
use jrsonnet_ir::{CodeLocation, Span};

use crate::{Error, ResolvePathOwned, analyze::DiagLevel, error::ErrorKind};

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
				let diff = pathdiff::diff_paths(from, base).expect("base is absolute");
				let parents = diff
					.components()
					.take_while(|c| matches!(c, Component::ParentDir))
					.count();
				let base_depth = base
					.components()
					.filter(|c| matches!(c, Component::Normal(_)))
					.count();
				if parents > 0 && parents >= base_depth {
					return from.to_string_lossy().into_owned();
				}
				diff.to_string_lossy().into_owned()
			}
		}
	}
}

/// Implements pretty-printing of traces
#[allow(clippy::module_name_repetitions)]
pub trait TraceFormat: Trace {
	fn write_trace(&self, out: &mut dyn fmt::Write, error: &Error) -> Result<(), fmt::Error>;
	fn format(&self, error: &Error) -> Result<String, fmt::Error> {
		let mut out = String::new();
		self.write_trace(&mut out, error)?;
		Ok(out)
	}
	fn as_any(&self) -> &dyn Any;
	fn as_any_mut(&mut self) -> &mut dyn Any;
}

fn span_label(resolver: &PathResolver, span: &Span) -> String {
	use std::fmt::Write;
	let mut path = span
		.0
		.source_path()
		.path()
		.map_or_else(|| span.0.source_path().to_string(), |p| resolver.resolve(p));
	#[expect(clippy::cast_possible_truncation, reason = "code is limited by 4gb")]
	let len = span.0.code().len() as u32;
	let start = span.1.min(len);
	let end = span.2.min(len);
	let (start_loc, end_loc) = if start == end {
		let [loc] = span.0.map_source_locations(&[start]);
		(loc, loc)
	} else {
		span.0.map_source_locations(&[start, end]).into()
	};
	write!(path, ":").unwrap();
	print_code_location(&mut path, &start_loc, &end_loc).unwrap();
	path
}

#[cfg(feature = "explaining-traces")]
fn span_render_range(span: &Span) -> Option<std::ops::RangeInclusive<usize>> {
	let len = span.0.code().len();
	if len == 0 {
		return None;
	}
	let max = len - 1;
	let r = span.range();
	Some((*r.start()).min(max)..=(*r.end()).min(max))
}

fn diag_level_label(level: DiagLevel) -> &'static str {
	match level {
		DiagLevel::Error => "error",
		DiagLevel::Warning => "warning",
	}
}

fn print_code_location(
	out: &mut impl fmt::Write,
	start: &CodeLocation,
	end: &CodeLocation,
) -> Result<(), fmt::Error> {
	let end_col = end.column.saturating_sub(1).max(start.column);
	if start.line == end.line {
		if start.column == end_col {
			write!(out, "{}:{}", start.line, start.column)?;
		} else {
			write!(out, "{}:{}-{}", start.line, start.column, end_col)?;
		}
	} else {
		write!(
			out,
			"{}:{}-{}:{}",
			start.line, start.column, end.line, end_col
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
	fn write_trace(&self, out: &mut dyn fmt::Write, error: &Error) -> Result<(), fmt::Error> {
		match error.error() {
			ErrorKind::ImportFileNotFound(from, import) => {
				let from = from
					.path()
					.map_or_else(|| from.to_string(), |path| self.resolver.resolve(path));
				let import = match import {
					ResolvePathOwned::Str(s) => s.clone(),
					ResolvePathOwned::Path(path_buf) => self.resolver.resolve(path_buf),
				};
				write!(out, "import file not found {import} from {from}")?;
			}
			ErrorKind::StaticAnalysisError(_) => {
				write!(out, "static analysis errors")?;
			}
			_ => {
				write!(out, "{}", error.error())?;
			}
		}

		if let ErrorKind::StaticAnalysisError(diagnostics) = error.error() {
			let labels: Vec<Option<String>> = diagnostics
				.iter()
				.map(|d| d.span.as_ref().map(|s| span_label(&self.resolver, s)))
				.collect();
			let align = labels.iter().flatten().map(String::len).max().unwrap_or(0);
			let cont_indent = " ".repeat(self.padding + align + 1);
			for (diag, label) in diagnostics.iter().zip(labels.iter()) {
				writeln!(out)?;
				let level = diag_level_label(diag.level);
				let message = diag.message.replace('\n', &format!("\n{cont_indent}"));
				let label = label.as_deref().unwrap_or("");
				write!(
					out,
					"{:<p$}{label:<w$} {level}: {message}",
					"",
					p = self.padding,
					w = align,
				)?;
			}
		}

		if let ErrorKind::ImportSyntaxError { error, .. } = error.error() {
			writeln!(out)?;
			let label = span_label(&self.resolver, &error.location);
			write!(out, "{:<p$}{label}", "", p = self.padding)?;
		}
		let file_names = error
			.trace()
			.0
			.iter()
			.map(|el| {
				el.location.as_ref().map(|loc| {
					use std::fmt::Write;
					let mut s = span_label(&self.resolver, loc);
					write!(s, ":").unwrap();
					s
				})
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
				write!(out, "{:<p$}{}", "", el.desc, p = self.padding)?;
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
	fn write_trace(&self, out: &mut dyn fmt::Write, error: &Error) -> Result<(), fmt::Error> {
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
	#[allow(clippy::too_many_lines)]
	fn write_trace(&self, out: &mut dyn fmt::Write, error: &Error) -> Result<(), fmt::Error> {
		struct ResetData {
			loc: Span,
		}
		use hi_doc::{Formatting, SnippetBuilder, Text, source_to_ansi};

		match error.error() {
			ErrorKind::StaticAnalysisError(_) => write!(out, "static analysis errors")?,
			_ => write!(out, "{}", error.error())?,
		}
		if let ErrorKind::ImportSyntaxError { path, error } = error.error() {
			writeln!(out, "\n...at {}", path.source_path())?;
			if let Some(range) = span_render_range(&error.location) {
				let mut builder = SnippetBuilder::new(path.code());
				builder
					.error(Text::fragment("syntax error", Formatting::default()))
					.range(range)
					.build();
				let ansi = source_to_ansi(&builder.build());
				write!(out, "{}", ansi.trim_end())?;
			}
		}
		if let ErrorKind::StaticAnalysisError(diagnostics) = error.error() {
			let mut builder: Option<(SnippetBuilder, Span)> = None;
			let flush = |slot: Option<(SnippetBuilder, Span)>,
			             out: &mut dyn fmt::Write|
			 -> Result<(), fmt::Error> {
				if let Some((b, anchor)) = slot {
					writeln!(out, "\n...at {}", anchor.0.source_path())?;
					let ansi = source_to_ansi(&b.build());
					write!(out, "{}", ansi.trim_end())?;
				}
				Ok(())
			};
			for diag in diagnostics {
				if let Some(span) = &diag.span {
					let Some(range) = span_render_range(span) else {
						continue;
					};
					let same_src = builder.as_ref().is_some_and(|(_, a)| a.0 == span.0);
					if !same_src {
						flush(builder.take(), out)?;
						builder = Some((SnippetBuilder::new(span.0.code()), span.clone()));
					}
					let b = &mut builder.as_mut().unwrap().0;
					let ab = match diag.level {
						DiagLevel::Error => {
							b.error(Text::fragment(diag.message.clone(), Formatting::default()))
						}
						DiagLevel::Warning => {
							b.warning(Text::fragment(diag.message.clone(), Formatting::default()))
						}
					};
					ab.range(range).build();
				} else {
					flush(builder.take(), out)?;
					let prefix = diag_level_label(diag.level);
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
