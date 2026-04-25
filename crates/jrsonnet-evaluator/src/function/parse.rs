use std::rc::Rc;

use crate::{
	Context, ContextBuilder, Result, Thunk,
	analyze::LFunction,
	evaluate::{destructure::destruct, evaluate},
};

/// Creates Context with all argument default values applied
/// and with unbound values causing error to be returned.
pub fn parse_default_function_call(body_ctx: Context, func: &Rc<LFunction>) -> Result<Context> {
	let fctx = Context::new_future();
	let mut builder = ContextBuilder::extend(body_ctx, func.params.len());

	for param in &func.params {
		if let Some(default_expr) = &param.default {
			let default_expr = default_expr.clone();
			let fctxc = fctx.clone();
			let thunk = Thunk!(move || {
				let ctx = fctxc.unwrap();
				evaluate(ctx, &default_expr)
			});
			destruct(&param.destruct, thunk, fctx.clone(), &mut builder);
		} else {
			let name = param.name.clone().unwrap_or_else(|| "<param>".into());
			let thunk = Thunk::errored(
				crate::error::ErrorKind::FunctionParameterNotBoundInCall(
					jrsonnet_ir::function::ParamName::Named(name),
					jrsonnet_ir::function::FunctionSignature::empty(),
				)
				.into(),
			);
			destruct(&param.destruct, thunk, fctx.clone(), &mut builder);
		}
	}

	let ctx = builder.build().into_future(fctx);
	Ok(ctx)
}
