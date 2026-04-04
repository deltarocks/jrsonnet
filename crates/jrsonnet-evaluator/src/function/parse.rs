use jrsonnet_ir::ExprParams;
use rustc_hash::FxHashMap;

use crate::{
	Context, Thunk,
	destructure::destruct,
	error::{ErrorKind::*, Result},
	evaluate_named_param,
	gc::WithCapacityExt as _,
};

/// Creates Context, which has all argument default values applied
/// and with unbound values causing error to be returned
pub fn parse_default_function_call(body_ctx: Context, params: &ExprParams) -> Result<Context> {
	let fctx = Context::new_future();

	let mut bindings = FxHashMap::with_capacity(params.binds_len());

	for param in params.exprs.iter() {
		if let Some(v) = &param.default {
			destruct(
				&param.destruct.clone(),
				{
					let ctx = fctx.clone();
					let name = param.destruct.name();
					let value = v.clone();
					Thunk!(move || evaluate_named_param(ctx.unwrap(), &value, name))
				},
				fctx.clone(),
				&mut bindings,
			)?;
		} else {
			destruct(
				&param.destruct,
				{
					let param_name = param.destruct.name();
					let params = params.clone();
					Thunk!(move || Err(FunctionParameterNotBoundInCall(
						param_name,
						params.signature
					)
					.into()))
				},
				fctx.clone(),
				&mut bindings,
			)?;
		}
	}

	Ok(body_ctx.extend_bindings(bindings).into_future(fctx))
}
