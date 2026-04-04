use jrsonnet_evaluator::{State, function::builtin, trace::PathResolver};
use jrsonnet_stdlib::ContextInitializer;

#[builtin]
fn example_native(a: u32, b: u32) -> u32 {
	a + b
}

#[test]
fn std_native() {
	let mut state = State::builder();
	let std = ContextInitializer::new(PathResolver::Absolute);
	std.add_native("example", example_native {});
	state.context_initializer(std);
	let state = state.build();

	assert!(
		state
			.evaluate_snippet("test", "std.native('example')(1, 3) == 4")
			.unwrap()
			.as_bool()
			.expect("boolean output")
	);
}
