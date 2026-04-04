mod common;

use jrsonnet_evaluator::{
	ContextBuilder, ContextInitializer, FileImportResolver, Result, State, Thunk, Val,
	function::{CallLocation, FuncVal, builtin, builtin::Builtin},
	parser::Source,
	trace::PathResolver,
	typed::FromUntyped,
};
use jrsonnet_gcmodule::Trace;
use jrsonnet_stdlib::ContextInitializer as StdContextInitializer;

#[builtin]
fn a() -> u32 {
	1
}

#[test]
fn basic_function() -> Result<()> {
	let a: a = a {};
	let v = u32::from_untyped(a.call(CallLocation::native(), &[])?)?;

	ensure_eq!(v, 1);
	Ok(())
}

#[builtin]
fn native_add(a: u32, b: u32) -> u32 {
	a + b
}
#[derive(Trace)]
struct NativeAddContextInitializer;
impl ContextInitializer for NativeAddContextInitializer {
	fn populate(&self, _for_file: Source, builder: &mut ContextBuilder) {
		builder.bind("nativeAdd", Thunk::evaluated(Val::function(native_add {})));
	}

	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
}

#[test]
fn call_from_code() -> Result<()> {
	let mut s = State::builder();
	s.context_initializer((
		StdContextInitializer::new(PathResolver::new_cwd_fallback()),
		NativeAddContextInitializer,
	))
	.import_resolver(FileImportResolver::default());
	let s = s.build();

	let v = s.evaluate_snippet(
		"snip".to_owned(),
		"
            assert nativeAdd(1, 2) == 3;
            assert nativeAdd(100, 200) == 300;
            null
        ",
	)?;
	ensure_val_eq!(v, Val::Null);
	Ok(())
}

#[builtin(fields(
    a: u32
))]
fn curried_add(this: &curried_add, b: u32) -> u32 {
	this.a + b
}

#[builtin]
fn curry_add(a: u32) -> FuncVal {
	FuncVal::builtin(curried_add { a })
}
#[derive(Trace)]
struct CurryAddContextInitializer;
impl ContextInitializer for CurryAddContextInitializer {
	fn populate(&self, _for_file: Source, builder: &mut ContextBuilder) {
		builder.bind("curryAdd", Thunk::evaluated(Val::function(curry_add {})));
	}

	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
}

#[test]
fn nonstatic_builtin() -> Result<()> {
	let mut s = State::builder();
	s.context_initializer((
		StdContextInitializer::new(PathResolver::new_cwd_fallback()),
		CurryAddContextInitializer,
	))
	.import_resolver(FileImportResolver::default());
	let s = s.build();

	let v = s.evaluate_snippet(
		"snip".to_owned(),
		"
            local a = curryAdd(1);
            local b = curryAdd(4);

            assert a(2) == 3;
            assert a(200) == 201;

            assert b(2) == 6;
            assert b(200) == 204;
            null
        ",
	)?;
	ensure_val_eq!(v, Val::Null);
	Ok(())
}
