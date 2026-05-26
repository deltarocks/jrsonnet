mod manifest;
mod stdlib;
mod tla;
mod trace;

use std::{env, marker::PhantomData, path::PathBuf};

use clap::Parser;
use jrsonnet_evaluator::{
	FileImportResolver,
	stack::{StackDepthLimitOverrideGuard, limit_stack_depth},
};
use jrsonnet_gcmodule::{ObjectSpace, with_thread_object_space};
pub use manifest::*;
pub use stdlib::*;
pub use tla::*;
pub use trace::*;

#[derive(Parser)]
#[clap(next_help_heading = "INPUT")]
pub struct InputOpts {
	/// Treat input as code, evaluate them instead of reading file
	#[clap(long, short = 'e')]
	pub exec: bool,

	/// Path to the file to be compiled if `--evaluate` is unset, otherwise code itself
	pub input: String,
}

#[derive(Parser)]
#[clap(next_help_heading = "OPTIONS")]
pub struct MiscOpts {
	/// Maximal allowed number of stack frames,
	/// stack overflow error will be raised if this number gets exceeded.
	#[clap(long, short = 's', default_value = "512")]
	max_stack: usize,

	/// Library search dirs. (right-most wins)
	/// Any not found `imported` file will be searched in these.
	/// This can also be specified via `JSONNET_PATH` variable,
	#[cfg_attr(
		windows,
		doc = "which should contain a semicolon-separated list of directories."
	)]
	#[cfg_attr(
		not(windows),
		doc = "which should contain a colon-separated list of directories."
	)]
	#[clap(long, short = 'J')]
	jpath: Vec<PathBuf>,
}
impl MiscOpts {
	pub fn import_resolver(&self) -> FileImportResolver {
		let mut library_paths = self.jpath.clone();
		library_paths.reverse();
		if let Some(path) = env::var_os("JSONNET_PATH") {
			library_paths.extend(env::split_paths(path.as_os_str()));
		}

		FileImportResolver::new(library_paths)
	}
	pub fn stack_size_override(&self) -> StackDepthLimitOverrideGuard {
		limit_stack_depth(self.max_stack)
	}
}

#[derive(Parser)]
#[clap(next_help_heading = "GARBAGE COLLECTION")]
pub struct GcOpts {
	/// Do not skip gc on exit
	#[clap(long)]
	gc_collect_on_exit: bool,
	/// Print gc stats before exit
	#[clap(long)]
	gc_print_stats: bool,
	/// Force garbage collection before printing stats
	/// Useful for checking for memory leaks
	/// Does nothing useless --gc-print-stats is specified
	#[clap(long)]
	gc_collect_before_printing_stats: bool,
}
impl GcOpts {
	pub fn stats_printer(&self) -> Option<GcStatsPrinter> {
		#[allow(clippy::unnecessary_lazy_evaluations/*, reason = "GcStatsPrinter has side-effect on Drop"*/)]
		self.gc_print_stats.then(|| GcStatsPrinter {
			collect_before_printing_stats: self.gc_collect_before_printing_stats,
		})
	}
	pub fn leak_on_exit(&self) -> Option<LeakSpace> {
		(!self.gc_collect_on_exit).then(|| LeakSpace(PhantomData))
	}
}

pub struct LeakSpace(PhantomData<()>);

impl Drop for LeakSpace {
	fn drop(&mut self) {
		with_thread_object_space(ObjectSpace::leak);
	}
}

pub struct GcStatsPrinter {
	collect_before_printing_stats: bool,
}
impl Drop for GcStatsPrinter {
	fn drop(&mut self) {
		eprintln!("=== GC STATS ===");
		if self.collect_before_printing_stats {
			let collected = jrsonnet_gcmodule::collect_thread_cycles();
			eprintln!("Collected: {collected}");
		}
		eprintln!("Tracked: {}", jrsonnet_gcmodule::count_thread_tracked());
	}
}
