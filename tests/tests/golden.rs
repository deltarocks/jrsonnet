use std::path::Path;

use insta::{assert_snapshot, glob};
use jrsonnet_evaluator::{
	FileImportResolver, State,
	manifest::JsonFormat,
	trace::{CompactFormat, PathResolver, TraceFormat},
};
use jrsonnet_stdlib::ContextInitializer;
mod common;
use common::ContextInitializer as TestContextInitializer;

fn run(file: &Path) -> String {
	let mut s = State::builder();
	s.context_initializer((
		ContextInitializer::new(PathResolver::new_cwd_fallback()),
		TestContextInitializer,
	))
	.import_resolver(FileImportResolver::default());
	let s = s.build();

	let _entered = s.enter();

	let trace_format = CompactFormat {
		resolver: PathResolver::FileName,
		max_trace: 20,
		padding: 4,
	};

	let v = match s.import(file) {
		Ok(v) => v,
		Err(e) => return trace_format.format(&e).unwrap(),
	};
	match v.manifest(JsonFormat::default()) {
		Ok(v) => v,
		Err(e) => trace_format.format(&e).unwrap(),
	}
}

#[test]
fn golden() {
	glob!("../", "golden/*.jsonnet", |path| {
		let result = run(path);

		assert_snapshot!(result);
	});
}

#[test]
#[cfg(feature = "exp-null-coaelse")]
fn golden_null_coalesce() {
	glob!("../", "golden_null_coalesce/*.jsonnet", |path| {
		let result = run(path);

		assert_snapshot!(result);
	});
}

#[test]
#[cfg(feature = "exp-destruct")]
fn golden_destruct() {
	glob!("../", "golden_destruct/*.jsonnet", |path| {
		let result = run(path);

		assert_snapshot!(result);
	});
}
