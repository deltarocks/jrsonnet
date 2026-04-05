use jrsonnet_ir::ExprParams;

use crate::{
	Context, ContextBuilder, Thunk,
	destructure::destruct,
	error::{ErrorKind::*, Result},
	evaluate_named_param,
};

/// Creates Context, which has all argument default values applied
/// and with unbound values causing error to be returned
pub fn parse_default_function_call(body_ctx: Context, params: &ExprParams) -> Result<Context> {
	let fctx = Context::new_future();

	let mut ctx = ContextBuilder::extend(body_ctx);

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
				&mut ctx,
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
				&mut ctx,
			)?;
		}
	}

	Ok(ctx.build().into_future(fctx))
}
