use std::{cell::RefCell, collections::BTreeSet};

use jrsonnet_evaluator::{
	bail,
	error::{ErrorKind::*, Result},
	function::{builtin, CallLocation, FuncVal},
	manifest::JsonFormat,
	typed::{Either2, Either4},
	val::{equals, ArrValue},
	Either, IStr, ObjValue, ObjValueBuilder, ResultExt, Thunk, Val,
};
use jrsonnet_gcmodule::Cc;

use crate::Settings;

#[builtin]
pub fn builtin_length(x: Either![IStr, ArrValue, ObjValue, FuncVal]) -> u32 {
	use Either4::*;
	match x {
		A(x) => x.chars().count() as u32,
		B(x) => x.len(),
		C(x) => x.len(),
		D(f) => f.params_len(),
	}
}

#[builtin]
pub fn builtin_get(
	o: ObjValue,
	f: IStr,
	default: Option<Thunk<Val>>,
	#[default(true)] inc_hidden: bool,
) -> Result<Val> {
	let do_default = move || {
		let Some(default) = default else {
			return Ok(Val::Null);
		};
		default.evaluate()
	};
	// Happy path for invisible fields
	if !inc_hidden && !o.has_field_ex(f.clone(), false) {
		return do_default();
	}
	let Some(v) = o.get(f)? else {
		return do_default();
	};
	Ok(v)
}

#[builtin(fields(
	settings: Cc<RefCell<Settings>>,
))]
pub fn builtin_ext_var(this: &builtin_ext_var, x: IStr) -> Result<Val> {
	this.settings
		.borrow()
		.ext_vars
		.get(&x)
		.cloned()
		.ok_or_else(|| UndefinedExternalVariable(x))?
		.evaluate_tailstrict()
}

#[builtin(fields(
	settings: Cc<RefCell<Settings>>,
))]
pub fn builtin_native(this: &builtin_native, x: IStr) -> Val {
	this.settings
		.borrow()
		.ext_natives
		.get(&x)
		.cloned()
		.map_or(Val::Null, Val::Func)
}

#[builtin(fields(
	settings: Cc<RefCell<Settings>>,
))]
pub fn builtin_trace(
	this: &builtin_trace,
	loc: CallLocation,
	str: Val,
	rest: Option<Thunk<Val>>,
) -> Result<Val> {
	this.settings.borrow().trace_printer.print_trace(
		loc,
		match &str {
			Val::Str(s) => s.clone().into_flat(),
			Val::Func(f) => format!("{f:?}").into(),
			v => v.manifest(JsonFormat::debug())?.into(),
		},
	);
	rest.map_or_else(|| Ok(str), |rest| rest.evaluate())
}

#[allow(clippy::comparison_chain)]
#[builtin]
pub fn builtin_starts_with(a: Either![IStr, ArrValue], b: Either![IStr, ArrValue]) -> Result<bool> {
	Ok(match (a, b) {
		(Either2::A(a), Either2::A(b)) => a.starts_with(b.as_str()),
		(Either2::B(a), Either2::B(b)) => {
			if b.len() > a.len() {
				return Ok(false);
			} else if b.len() == a.len() {
				return equals(&Val::Arr(a), &Val::Arr(b));
			}
			for (a, b) in a.iter().take(b.len() as usize).zip(b.iter()) {
				let a = a?;
				let b = b?;
				if !equals(&a, &b)? {
					return Ok(false);
				}
			}
			true
		}
		_ => bail!("both arguments should be of the same type"),
	})
}

#[allow(clippy::comparison_chain)]
#[builtin]
pub fn builtin_ends_with(a: Either![IStr, ArrValue], b: Either![IStr, ArrValue]) -> Result<bool> {
	Ok(match (a, b) {
		(Either2::A(a), Either2::A(b)) => a.ends_with(b.as_str()),
		(Either2::B(a), Either2::B(b)) => {
			if b.len() > a.len() {
				return Ok(false);
			} else if b.len() == a.len() {
				return equals(&Val::Arr(a), &Val::Arr(b));
			}
			let a_len = a.len();
			for (a, b) in a.iter().skip((a_len - b.len()) as usize).zip(b.iter()) {
				let a = a?;
				let b = b?;
				if !equals(&a, &b)? {
					return Ok(false);
				}
			}
			true
		}
		_ => bail!("both arguments should be of the same type"),
	})
}

#[builtin]
pub fn builtin_assert_equal(a: Val, b: Val) -> Result<bool> {
	if equals(&a, &b)? {
		return Ok(true);
	}
	// TODO: Use debug output format
	let format = JsonFormat::std_to_json(
		"  ".to_owned(),
		"\n",
		": ",
		#[cfg(feature = "exp-preserve-order")]
		true,
	);
	let a = if let Some(a) = a.as_str() {
		format!("<A>\n{a}\n</A>")
	} else {
		a.manifest(&format).description("<a> manifestification")?
	};
	let b = if let Some(b) = b.as_str() {
		format!("<B>\n{b}\n</B>")
	} else {
		b.manifest(&format).description("<b> manifestification")?
	};
	bail!("assertion failed: A != B\nA: {a}\nB: {b}")
}

#[builtin]
pub fn builtin_merge_patch(target: Val, patch: Val) -> Result<Val> {
	let Some(patch) = patch.as_obj() else {
		return Ok(patch);
	};
	let target = target.as_obj().unwrap_or_else(ObjValue::empty);
	let target_fields = target
		.fields(
			// FIXME: Makes no sense to preserve order for BTreeSet, it would be better to use IndexSet here?
			// But IndexSet won't allow fast ordered union...
			// // Makes sense to preserve source ordering where possible.
			// // May affect evaluation order, but it is not specified by jsonnet spec.
			// #[cfg(feature = "exp-preserve-order")]
			// true,
			#[cfg(feature = "exp-preserve-order")]
			false,
		)
		.into_iter()
		.collect::<BTreeSet<IStr>>();
	let patch_fields = patch
		.fields(
			// No need to look at the patch field order, I think?
			// New fields (that will be appended at the end) will be alphabeticaly-ordered,
			// but it is fine for jsonpatch, I don't think people write jsonpatch in jsonnet,
			// when they can use mixins.
			#[cfg(feature = "exp-preserve-order")]
			false,
		)
		.into_iter()
		.collect::<BTreeSet<IStr>>();

	let mut out = ObjValueBuilder::new();
	for field in target_fields.union(&patch_fields) {
		let Some(field_patch) = patch.get(field.clone())? else {
			// All lazy fields might be unified into a single filtered object core instead of creating a thunk per, but this implementation is good enough.
			let target_field = target.get_lazy(field.clone()).expect(
				"we're iterating over fields union, if field is missing in patch - it exists in target",
			);
			out.field(field.clone()).thunk(target_field);
			continue;
		};
		if matches!(field_patch, Val::Null) {
			continue;
		}
		let field_target = target.get(field.clone())?.unwrap_or(Val::Null);
		out.field(field.clone())
			.value(builtin_merge_patch(field_target, field_patch)?);
	}
	Ok(out.build().into())
}
