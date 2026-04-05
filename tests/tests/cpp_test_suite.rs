use std::{
	env, fs,
	io::{self, ErrorKind},
	path::{Path, PathBuf},
};

use jrsonnet_evaluator::{
	FileImportResolver, IStr, ObjValueBuilder, State, Val, apply_tla,
	gc::WithCapacityExt as _,
	manifest::JsonFormat,
	rustc_hash::FxHashMap,
	tla::TlaArg,
	trace::{CompactFormat, PathResolver, TraceFormat},
};
use jrsonnet_stdlib::ContextInitializer;
mod common;
use common::ContextInitializer as TestContextInitializer;

fn run(file: &Path, root: &Path) -> String {
	let mut s = State::builder();

	let std_context = ContextInitializer::new(PathResolver::Relative(root.to_owned()));
	// C++ test suite
	std_context.add_ext_str("var1".into(), "test".into());
	std_context
		.add_ext_code("var2", "{x:1,y:2}")
		.expect("code is valid");

	// Golang test suite
	std_context
		.add_ext_code("codeVar", "3+3")
		.expect("code is valid");
	std_context.add_ext_str("stringVar".into(), "2 + 2".into());
	std_context
		.add_ext_code(
			"selfRecursiveVar",
			r#"[42, std.extVar("selfRecursiveVar")[0] + 1]"#,
		)
		.expect("code is valid");
	std_context
		.add_ext_code(
			"mutuallyRecursiveVar1",
			r#"[42, std.extVar("mutuallyRecursiveVar2")[0] + 1]"#,
		)
		.expect("code is valid");
	std_context
		.add_ext_code(
			"mutuallyRecursiveVar2",
			r#"[42, std.extVar("mutuallyRecursiveVar1")[0] + 1]"#,
		)
		.expect("code is valid");

	s.context_initializer((std_context, TestContextInitializer))
		.import_resolver(FileImportResolver::default());
	let s = s.build();

	let _entered = s.enter();

	let trace_format = CompactFormat {
		resolver: PathResolver::FileName,
		max_trace: 20,
		padding: 4,
	};

	let mut v = match s.import(file) {
		Ok(v) => v,
		Err(e) => return trace_format.format(&e).unwrap(),
	};

	if file
		.file_name()
		.expect("file has basename")
		.to_str()
		.expect("jsonnet testsuite has ascii names")
		.starts_with("tla.")
	{
		let mut args = FxHashMap::new();
		args.insert(IStr::from("var1"), TlaArg::String("test".into()));
		args.insert(
			IStr::from("var2"),
			TlaArg::Val({
				let mut o = ObjValueBuilder::new();

				o.field("x").value(Val::num(1));
				o.field("y").value(Val::num(2));

				Val::Obj(o.build())
			}),
		);
		v = apply_tla(&args, v).expect("failed to apply tla");
	} else {
		v = match apply_tla(&FxHashMap::new(), v) {
			Ok(v) => v,
			Err(e) => return trace_format.format(&e).unwrap(),
		};
	}

	match v.manifest(JsonFormat::default()) {
		Ok(v) => v,
		Err(e) => trace_format.format(&e).unwrap(),
	}
}

fn read_file(path: &Path) -> io::Result<Option<String>> {
	match fs::read_to_string(path) {
		Ok(v) => Ok(Some(v)),
		Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
		Err(e) => Err(e),
	}
}

const SKIPPED: &[&str] = &[
	// C++ tests:

	// Parser fails with stack overflow. While is a bug, this is a too unusual
	// thing to run untrusted jsonnet code? Will be fixed with nom/rowan.
	"error.parse.deep_array_nesting.jsonnet",
	// Runtime, not static error in jrsonnet
	"error.parse.object_local_clash.jsonnet",
	"error.function_duplicate_param.jsonnet",
	// Too slow to throw due to how lazyness is implemented in jrsonnet
	"error.recursive_object_non_term.jsonnet",
	// In jrsonnet returns the one passed argument, works as Rust's dbg!()
	"error.trace_one_param.jsonnet",
	// In jrsonnet can display any value
	"error.trace_two_param.jsonnet",
	// Depends on unsafe handling of strings as arrays in jsonnet stdlib
	"invariant_manifest.jsonnet",
	// Little bit hard to capture trace logs in this test suite at this moment
	"trace.jsonnet",
	// Go tests:

	// Something is wrong, go-jsonnet skips safe integer range check here
	"bitwise_or9.jsonnet",
	// Jrsonnet does not use byte strings, all utf8 is converted to bytes first
	"builtinBase64_string_high_codepoint.jsonnet",
	// Split by empty string is string characters, same as everywhere else
	"builtinSplitLimitR6.jsonnet",
	// escapeStringJson only accepts string in jrsonnet
	"builtin_escapeStringJson.jsonnet",
	// golang float formatting is inefficient and not portable
	"builtin_manifestTomlEx.jsonnet",
	"div3.jsonnet",
	"pow6.jsonnet",
	// golang escapes "e" yaml key, does it think it is float?
	"builtin_manifestYamlDoc.jsonnet",
	// multi output is a CLI part, not an interpreter.
	"multi.jsonnet",
	"multi_no_newline.jsonnet",
	"multi_no_newline_string_output.jsonnet",
	"multi_string_output.jsonnet",
	// Tested otherwise
	"native1.jsonnet",
	"native2.jsonnet",
	"native3.jsonnet",
	"native6.jsonnet",
	// Since when parser should throw an error for that?..
	"number_leading_zero.jsonnet",
	// Golang fails with max stack frames exceeded error
	"std.makeArray_recursive_evalutation_order_matters.jsonnet",
	// Tailstrict semantics is partially unspecified
	"tailstrict3.jsonnet",
	// Jrsonnet has this overload
	"number_times_string.jsonnet",
	// Jrsonnet has this overload
	"string_times_number.jsonnet",
];

#[test]
fn cpp_test_suite() -> io::Result<()> {
	for root_dir in ["cpp_test_suite", "go_testdata"] {
		let root_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
		let root = root_tests.join(root_dir);
		let root_override = root_tests.join(format!("{root_dir}_golden_override"));

		for entry in fs::read_dir(&root).map_err(|e| io::Error::other(format!("failed to enumerate cpp_test_suite dir (Note: it needs to be cloned from C++ jsonnet repo for this test): {e}")))? {
		let entry = entry?;
		if entry.path().extension().is_none_or(|e| e != "jsonnet") {
			continue;
		}

		if entry
			.path()
			.file_name()
			.and_then(|v| v.to_str())
			.is_some_and(|v| SKIPPED.contains(&v))
		{
			continue;
		}

		println!("test: {}", entry.path().display());

		let result = run(&entry.path(), &root);

		let mut golden_path = entry.path();
		golden_path.set_extension("jsonnet.golden");

		let mut golden_path2 = entry.path();
		golden_path2.set_extension("golden");

		let golden_override =
			root_override.join(golden_path.file_name().expect("file has basename"));

		// .jsonnet.golden for C++ tests
		let mut golden = read_file(&golden_path)?;
		// .golden for Go tests
		if golden.is_none() && let Some(golden_path) = read_file(&golden_path2)? {
			golden = Some(golden_path);
		}

		// Any of them can be overriden by overrides
		if let Some(golden_path) = read_file(&golden_override)? {
			golden = Some(golden_path);
		}

		// ir-parser has its own override layer
		#[cfg(feature = "ir-parser")]
		let ir_parser_override_path = {
			let p = root_tests
				.join(format!("{root_dir}_golden_override_ir_parser"))
				.join(golden_path.file_name().expect("file has basename"));
			if let Some(golden_path) = read_file(&p)? {
				golden = Some(golden_path);
			}
			p
		};

		// Otherwise assume test should just not fail and return true.
		let golden = golden.unwrap_or_else(|| "true".to_owned());

		#[cfg(feature = "ir-parser")]
		let update_golden_path = &ir_parser_override_path;
		#[cfg(not(feature = "ir-parser"))]
		let update_golden_path = &golden_override;

		match (serde_json::from_str::<serde_json::Value>(&result), serde_json::from_str::<serde_json::Value>(&golden)) {
			(Err(_), Ok(_)) => panic!(
				"unexpected error for golden {}:\n<got>\n{result}\n</got>\n<golden>\n{golden}\n</golden>",
				entry.path().display()
			),
			(Ok(_), Err(_)) => panic!(
				"expected error for golden {}:\n<got>\n{result}\n</got>\n<golden>\n{golden}\n</golden>",
				entry.path().display()
			),
			(Ok(result_v), Ok(golden_v)) => {
				if result_v != golden_v {
					if env::var_os("UPDATE_GOLDEN").is_some() {
						fs::write(update_golden_path, result)?;
					} else {
						panic!(
							"Result \n{result_v:#}\n\
								and golden \n{golden_v:#}\n\
								did not match structurally\n\
								for golden {}",
							entry.path().display()
						);
					}
				}
			}
			(Err(_), Err(_)) => {
				if result != golden.trim_end() {
					if env::var_os("UPDATE_GOLDEN").is_some() {
						fs::write(update_golden_path, result)?;
					} else {
						panic!(
						"golden didn't match for {}:\n<got>\n{result}\n</got>\n<golden>\n{golden}\n</golden>",
						entry.path().display()
					)
					}
				}
			}
		}
	}
	}

	Ok(())
}
