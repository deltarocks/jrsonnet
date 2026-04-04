use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use jrsonnet_evaluator::{State, manifest::JsonFormat, trace::PathResolver};

fn criterion_benchmark(c: &mut Criterion) {
	c.bench_function("prim_comparison", |b| {
		let mut s = State::builder();

		s.context_initializer(jrsonnet_stdlib::ContextInitializer::new(
			PathResolver::Absolute,
		));

		let s = s.build();
		let _s = s.enter();

		b.iter(|| {
			black_box(
				s.evaluate_snippet(
					"snippet",
					"([ i < j for i in std.range(1, 1000) for j in std.range(1, 1000)])",
				)
				.expect("evaluated")
				.manifest(JsonFormat::cli(3))
				.expect("manifested"),
			)
		});
	});
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
