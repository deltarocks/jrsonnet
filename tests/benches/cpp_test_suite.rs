use std::{collections::HashMap, fs::read_dir, hint::black_box, path::Path};

use criterion::{Criterion, criterion_group, criterion_main};
use jrsonnet_evaluator::{
	FileImportResolver, State, apply_tla, manifest::JsonFormat, trace::PathResolver,
};

fn bench_entry(c: &mut Criterion, path: &Path) {
	c.bench_function(
		path.file_name()
			.expect("file path")
			.to_str()
			.expect("name is utf-8"),
		|b| {
			let mut s = State::builder();

			s.context_initializer(jrsonnet_stdlib::ContextInitializer::new(
				PathResolver::Absolute,
			))
			.import_resolver(FileImportResolver::new(vec![]));

			let s = s.build();
			let _s = s.enter();

			b.iter(|| {
				let imported = s.import(path).expect("evaluated");
				let res = apply_tla(&HashMap::new(), imported).expect("tla applied");
				black_box(res.manifest(JsonFormat::cli(3)))
			});
		},
	);
}
fn criterion_benchmark(c: &mut Criterion) {
	for entry in read_dir("go_builtin_benchmarks").expect("dir exists") {
		let entry = entry.expect("entry is valid");
		assert!(entry.metadata().expect("entry is valid").is_file());
		bench_entry(c, &entry.path());
	}
	for entry in read_dir("cpp_perf_tests").expect("dir exists") {
		let entry = entry.expect("entry is valid");
		assert!(entry.metadata().expect("entry is valid").is_file());
		bench_entry(c, &entry.path());
	}
	for entry in read_dir("cpp_benchmarks").expect("dir exists") {
		let entry = entry.expect("entry is valid");
		// Skip .gitignore
		if entry.path().extension().is_none() {
			continue;
		}
		assert!(entry.metadata().expect("entry is valid").is_file());
		bench_entry(c, &entry.path());
	}
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
