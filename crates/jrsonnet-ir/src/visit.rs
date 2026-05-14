use jrsonnet_interner::IStr;

#[cfg(feature = "exp-object-iteration")]
use crate::ForObjSpecData;
use crate::{
	ArgsDesc, AssertExpr, AssertStmt, BinaryOp, BindSpec, CompSpec, Destruct, Expr, ExprParam,
	ExprParams, FieldMember, FieldName, ForSpecData, IfElse, IfSpecData, ImportKind, IndexPart,
	ObjBody, ObjComp, ObjMembers, Slice, SliceDesc, TrivialVal,
};

/// Expr visitor
pub trait Visitor: Sized {
	/// The input contains an `Expr`.
	///
	/// The default implementation forwards to [`visit_expr`]
	fn visit_expr(&mut self, e: &Expr) {
		visit_expr(self, e)
	}
	/// Called for every encountered `import`/`importbin`/`importstr` expression
	///
	/// `as_expression` is `true` for non-str/bin `imports`
	fn visit_import(&mut self, _as_expression: bool, _value: IStr) {}
}

#[cfg(feature = "exp-destruct")]
pub fn visit_destruct_rest<V: Visitor>(_v: &mut V, destruct: &crate::DestructRest) {
	match destruct {
		crate::DestructRest::Keep(_name) => {}
		crate::DestructRest::Drop => {}
	}
}

/// Use [`Visitor`] to recursively visit [`Destruct`]
#[allow(unused_variables, reason = "used with exp-destruct")]
pub fn visit_destruct<V: Visitor>(v: &mut V, destruct: &Destruct) {
	match destruct {
		Destruct::Full(_istr) => {}
		#[cfg(feature = "exp-destruct")]
		Destruct::Skip => {}
		#[cfg(feature = "exp-destruct")]
		Destruct::Array { start, rest, end } => {
			for s in start {
				visit_destruct(v, s);
			}
			if let Some(rest) = rest {
				visit_destruct_rest(v, rest);
			}
			for s in end {
				visit_destruct(v, s);
			}
		}
		#[cfg(feature = "exp-destruct")]
		Destruct::Object { fields, rest } => {
			for (_name, into, default) in fields {
				if let Some(into) = into {
					visit_destruct(v, into);
				}
				if let Some(default) = default {
					v.visit_expr(default);
				}
				if let Some(rest) = rest {
					visit_destruct_rest(v, rest);
				}
			}
		}
	}
}

/// Use [`Visitor`] to recursively visit [`IfSpecData`]
pub fn visit_if_spec<V: Visitor>(v: &mut V, cond: &IfSpecData) {
	let IfSpecData { span: _, cond } = cond;
	v.visit_expr(cond);
}

/// Use [`Visitor`] to recursively visit [`CompSpec`]
pub fn visit_comp_spec<V: Visitor>(v: &mut V, c: &CompSpec) {
	match c {
		CompSpec::IfSpec(cond) => visit_if_spec(v, cond),
		CompSpec::ForSpec(for_spec_data) => {
			let ForSpecData { destruct, over } = for_spec_data;
			visit_destruct(v, destruct);
			v.visit_expr(over);
		}
		#[cfg(feature = "exp-object-iteration")]
		CompSpec::ForObjSpec(for_obj_spec_data) => {
			let ForObjSpecData {
				key: _,
				visibility: _,
				value,
				over,
			} = for_obj_spec_data;
			visit_destruct(v, value);
			v.visit_expr(over);
		}
	}
}

/// Use [`Visitor`] to recursively visit [`ExprParams`]
pub fn visit_params<V: Visitor>(v: &mut V, par: &ExprParams) {
	let ExprParams {
		exprs,
		signature: _,
		binds_len: _,
	} = par;
	for par in &**exprs {
		let ExprParam { destruct, default } = &par;
		visit_destruct(v, destruct);
		if let Some(default) = default {
			v.visit_expr(default);
		}
	}
}

/// Use [`Visitor`] to recursively visit [`BindSpec`]
pub fn visit_bind_spec<V: Visitor>(v: &mut V, bind: &BindSpec) {
	match bind {
		BindSpec::Field { into, value } => {
			visit_destruct(v, into);
			v.visit_expr(value);
		}
		BindSpec::Function {
			name: _,
			params,
			value,
		} => {
			visit_params(v, params);
			v.visit_expr(value);
		}
	}
}

/// Use [`Visitor`] to recursively visit [`FieldMember`]
pub fn visit_field_member<V: Visitor>(v: &mut V, mem: &FieldMember) {
	let FieldMember {
		name,
		plus: _,
		params,
		visibility: _,
		value,
	} = mem;
	match &**name {
		FieldName::Fixed(_istr) => {}
		FieldName::Dyn(expr) => v.visit_expr(expr),
	}
	if let Some(params) = params {
		visit_params(v, params);
	}
	v.visit_expr(value);
}

/// Use [`Visitor`] to recursively visit [`ObjBody`]
pub fn visit_obj_body<V: Visitor>(v: &mut V, obj_body: &ObjBody) {
	match obj_body {
		ObjBody::MemberList(obj_members) => {
			let ObjMembers {
				locals,
				asserts,
				fields,
			} = obj_members;
			for local in &**locals {
				visit_bind_spec(v, local);
			}
			for assert in &**asserts {
				visit_assert_stmt(v, assert);
			}
			for field in fields {
				visit_field_member(v, field);
			}
		}
		ObjBody::ObjComp(obj_comp) => {
			let ObjComp {
				locals,
				field,
				compspecs,
			} = obj_comp;
			for local in &**locals {
				visit_bind_spec(v, local);
			}
			visit_field_member(v, field);
			for compspec in compspecs {
				visit_comp_spec(v, compspec);
			}
		}
	}
}

/// Use [`Visitor`] to recursively visit [`AssertStmt`]
pub fn visit_assert_stmt<V: Visitor>(v: &mut V, ass: &AssertStmt) {
	let AssertStmt { assertion, message } = ass;
	v.visit_expr(assertion);
	if let Some(message) = message {
		v.visit_expr(message);
	}
}

/// Use [`Visitor`] to recursively visit [`Expr`]
pub fn visit_expr<V: Visitor>(v: &mut V, e: &Expr) {
	match e {
		Expr::Identity(_span, _identity_kind) => {}
		Expr::Trivial(_) => {}
		Expr::Var(_spanned) => {}
		Expr::Arr(exprs) => {
			for e in &**exprs {
				v.visit_expr(e);
			}
		}
		Expr::ArrComp(expr, comp_specs) => {
			v.visit_expr(expr);
			for ele in comp_specs {
				visit_comp_spec(v, ele);
			}
		}
		Expr::Obj(obj_body) => visit_obj_body(v, obj_body),
		Expr::ObjExtend(expr, obj_body) => {
			v.visit_expr(expr);
			visit_obj_body(v, obj_body);
		}
		Expr::UnaryOp(_unary_op_type, expr) => {
			v.visit_expr(expr);
		}
		Expr::BinaryOp(binary_op) => {
			let BinaryOp { lhs, op: _, rhs } = &**binary_op;
			v.visit_expr(lhs);
			v.visit_expr(rhs);
		}
		Expr::AssertExpr(assert_expr) => {
			let AssertExpr { assert, rest } = &**assert_expr;
			visit_assert_stmt(v, assert);
			v.visit_expr(rest);
		}
		Expr::LocalExpr(bind_specs, expr) => {
			for local in bind_specs {
				visit_bind_spec(v, local);
			}
			v.visit_expr(expr);
		}
		Expr::Import(kind, expr) => {
			v.visit_expr(expr);

			if let Expr::Trivial(TrivialVal::Str(expr)) = &**expr {
				v.visit_import(matches!(**kind, ImportKind::Normal), expr.clone());
			}
		}
		Expr::ErrorStmt(_span, expr) => {
			v.visit_expr(expr);
		}
		Expr::Apply(expr, spanned, _) => {
			v.visit_expr(expr);
			let ArgsDesc {
				unnamed,
				names: _,
				values,
			} = &**spanned;
			for unnamed in unnamed {
				v.visit_expr(unnamed);
			}
			for named in values {
				v.visit_expr(named);
			}
		}
		Expr::Index { indexable, parts } => {
			v.visit_expr(indexable);

			for part in parts {
				let IndexPart {
					span: _,
					value,
					#[cfg(feature = "exp-null-coaelse")]
						null_coaelse: _,
				} = part;
				v.visit_expr(value);
			}
		}
		Expr::Function(_span, expr_params, expr) => {
			visit_params(v, expr_params);
			v.visit_expr(expr);
		}
		Expr::IfElse(if_else) => {
			let IfElse {
				cond,
				cond_then,
				cond_else,
			} = &**if_else;
			visit_if_spec(v, cond);
			v.visit_expr(cond_then);
			if let Some(cond_else) = cond_else {
				v.visit_expr(cond_else);
			}
		}
		Expr::Slice(slice) => {
			let Slice { value, slice } = &**slice;
			v.visit_expr(value);
			let SliceDesc { start, end, step } = slice;

			if let Some(start) = start {
				v.visit_expr(start);
			}
			if let Some(end) = end {
				v.visit_expr(end);
			}
			if let Some(step) = step {
				v.visit_expr(step);
			}
		}
	}
}
