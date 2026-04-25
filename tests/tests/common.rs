use jrsonnet_evaluator::{
	ContextBuilder, ContextInitializer as ContextInitializerT, InitialContextBuilder,
	ObjValueBuilder, Result, Source, Thunk, Val, bail,
	function::{FuncVal, builtin},
};
use jrsonnet_gcmodule::Trace;

#[macro_export]
macro_rules! ensure_eq {
	($a:expr, $b:expr $(,)?) => {{
		let a = &$a;
		let b = &$b;
		if a != b {
			::jrsonnet_evaluator::bail!("assertion failed: a != b\na={a:#?}\nb={b:#?}")
		}
	}};
}

#[macro_export]
macro_rules! ensure {
	($v:expr $(,)?) => {
		if !$v {
			::jrsonnet_evaluator::bail!("assertion failed: {}", stringify!($v))
		}
	};
}

#[macro_export]
macro_rules! ensure_val_eq {
	($a:expr, $b:expr) => {{
		if !::jrsonnet_evaluator::val::equals(&$a.clone(), &$b.clone())? {
			use ::jrsonnet_evaluator::manifest::JsonFormat;
			::jrsonnet_evaluator::bail!(
				"assertion failed: a != b\na={:#?}\nb={:#?}",
				$a.manifest(JsonFormat::default())?,
				$b.manifest(JsonFormat::default())?,
			)
		}
	}};
}

#[builtin]
#[allow(dead_code)]
fn assert_throw(lazy: Thunk<Val>, message: String) -> Result<bool> {
	match lazy.evaluate() {
		Ok(_) => {
			bail!("expected argument to throw on evaluation, but it returned instead")
		}
		Err(e) => {
			let error = format!("{}", e.error());
			ensure_eq!(message, error);
		}
	}
	Ok(true)
}

#[builtin]
#[allow(dead_code)]
fn param_names(fun: FuncVal) -> Vec<String> {
	fun.params()
		.iter()
		.map(|v| v.name().as_str().unwrap_or("<unnamed>").to_owned())
		.collect()
}

#[derive(Trace)]
#[allow(dead_code)]
pub struct ContextInitializer;
impl ContextInitializerT for ContextInitializer {
	fn populate(&self, _for_file: Source, builder: &mut InitialContextBuilder) {
		let mut bobj = ObjValueBuilder::new();
		bobj.method("assertThrow", assert_throw {});
		bobj.method("paramNames", param_names {});

		builder.bind("test", Thunk::evaluated(Val::Obj(bobj.build())));
	}

	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
}
