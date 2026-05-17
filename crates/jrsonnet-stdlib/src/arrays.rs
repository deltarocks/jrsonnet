#![allow(non_snake_case)]

use jrsonnet_evaluator::{
	Either, IStr, ObjValue, ObjValueBuilder, Result, ResultExt, Thunk, Val, bail, error,
	function::{FoldKind, NativeFn, builtin},
	manifest::{ManifestFormat as _, ToStringFormat},
	typed::{BoundedUsize, Either2, FromUntyped},
	val::{ArrValue, IndexableVal, equals},
};

pub fn eval_on_empty(on_empty: Option<Thunk<Val>>) -> Result<Val> {
	if let Some(on_empty) = on_empty {
		on_empty.evaluate()
	} else {
		bail!("expected non-empty array")
	}
}

#[builtin]
pub fn builtin_make_array(sz: u32, func: NativeFn!((u32,) -> Val)) -> Result<ArrValue> {
	if sz == 0 {
		return Ok(ArrValue::empty());
	}
	// Try eager evaluation: call func(i) immediately for each element.
	'eager: {
		let mut out = Vec::with_capacity(sz as usize);
		for i in 0..sz {
			match func.call(i) {
				Ok(v) => out.push(v),
				Err(_) => break 'eager,
			}
		}
		return Ok(ArrValue::new(out));
	}
	Ok(ArrValue::make(sz, func))
}

#[builtin]
pub fn builtin_repeat(what: Either![IStr, ArrValue], count: u32) -> Result<Val> {
	Ok(match what {
		Either2::A(s) => Val::string(s.repeat(count as usize)),
		Either2::B(arr) => Val::Arr(
			ArrValue::repeated(arr, count).ok_or_else(|| error!("repeated length overflow"))?,
		),
	})
}

#[builtin]
pub fn builtin_slice(
	indexable: IndexableVal,
	index: Option<Option<i32>>,
	end: Option<Option<i32>>,
	step: Option<Option<BoundedUsize<1, { i32::MAX as usize }>>>,
) -> Result<Val> {
	indexable
		.slice32(index.flatten(), end.flatten(), step.flatten())
		.map(Val::from)
}

#[builtin]
pub fn builtin_map(func: NativeFn!((Val) -> Val), arr: IndexableVal) -> ArrValue {
	let arr = arr.to_array();
	arr.map(func)
}

#[builtin]
pub fn builtin_map_with_index(func: NativeFn!((u32, Val) -> Val), arr: IndexableVal) -> ArrValue {
	let arr = arr.to_array();
	arr.map_with_index(func)
}

#[builtin]
pub fn builtin_map_with_key(
	func: NativeFn!((IStr, Val) -> Val),
	obj: ObjValue,
) -> Result<ObjValue> {
	let mut out = ObjValueBuilder::new();
	for (k, v) in obj.iter(
		// Makes sense mapped object should be ordered the same way, should not break anything when the output is not ordered (the default).
		// The thrown error might be different, but jsonnet
		// does not specify the evaluation order.
		#[cfg(feature = "exp-preserve-order")]
		true,
	) {
		let v = v?;
		out.field(k.clone()).value(func.call(k, v)?);
	}
	Ok(out.build())
}

#[builtin]
pub fn builtin_flatmap(
	func: NativeFn!((Either![String, Val]) -> Val),
	arr: IndexableVal,
) -> Result<IndexableVal> {
	use std::fmt::Write;
	match arr {
		IndexableVal::Str(str) => {
			let mut out = String::new();
			for c in str.chars() {
				match func.call(Either2::A(c.to_string()))? {
					Val::Str(o) => write!(out, "{o}").unwrap(),
					Val::Null => {}
					_ => bail!("in std.join all items should be strings"),
				}
			}
			Ok(IndexableVal::Str(out.into()))
		}
		IndexableVal::Arr(a) => {
			let mut out = Vec::new();
			for el in a.iter() {
				let el = el?;
				match func.call(Either2::B(el))? {
					Val::Arr(o) => {
						for oe in o.iter() {
							out.push(oe?);
						}
					}
					Val::Null => {}
					_ => bail!("in std.join all items should be arrays"),
				}
			}
			Ok(IndexableVal::Arr(out.into()))
		}
	}
}

type FilterFunc = NativeFn!((Thunk<Val>) -> bool);

#[builtin]
pub fn builtin_filter(func: FilterFunc, arr: ArrValue) -> Result<ArrValue> {
	arr.filter(func)
}

#[builtin]
pub fn builtin_filter_map(
	filter_func: FilterFunc,
	map_func: NativeFn!((Val) -> Val),
	arr: ArrValue,
) -> Result<ArrValue> {
	Ok(arr.filter(filter_func)?.map(map_func))
}

#[builtin]
pub fn builtin_foldl(
	func: NativeFn!((Val, Either![Val, char]) -> Val),
	arr: Either![ArrValue, IStr],
	init: Val,
) -> Result<Val> {
	if let (Val::Str(init), Either2::A(arr)) = (&init, &arr)
		&& let Some(folder) = func.func().as_folder(FoldKind::Left)
	{
		let mut buf = String::new();
		init.into_flat_in(&mut buf);
		for i in arr.iter_lazy() {
			ToStringFormat.manifest_buf(&folder.eval(i)?, &mut buf)?;
		}
		return Ok(Val::string(buf));
	}

	let mut acc = init;
	match arr {
		Either2::A(arr) => {
			for i in arr.iter() {
				acc = func.call(acc, Either2::A(i?))?;
			}
		}
		Either2::B(arr) => {
			for c in arr.chars() {
				acc = func.call(acc, Either2::B(c))?;
			}
		}
	}
	Ok(acc)
}

#[builtin]
pub fn builtin_foldr(
	func: NativeFn!((Either![Val, char], Val) -> Val),
	arr: Either![ArrValue, IStr],
	init: Val,
) -> Result<Val> {
	if let (Val::Str(init), Either2::A(arr)) = (&init, &arr)
		&& let Some(folder) = func.func().as_folder(FoldKind::Right)
	{
		let mut buf = String::new();
		for i in arr.iter_lazy() {
			ToStringFormat.manifest_buf(&folder.eval(i)?, &mut buf)?;
		}
		init.into_flat_in(&mut buf);
		return Ok(Val::string(buf));
	}

	let mut acc = init;
	match arr {
		Either2::A(arr) => {
			for i in arr.iter().rev() {
				acc = func.call(Either2::A(i?), acc)?;
			}
		}
		Either2::B(arr) => {
			for c in arr.chars().rev() {
				acc = func.call(Either2::B(c), acc)?;
			}
		}
	}
	Ok(acc)
}

#[builtin]
pub fn builtin_range(from: i32, to: i32) -> Result<ArrValue> {
	if to < from {
		return Ok(ArrValue::empty());
	}
	Ok(ArrValue::range_inclusive(from, to))
}

#[builtin]
pub fn builtin_join(sep: IndexableVal, arr: ArrValue) -> Result<IndexableVal> {
	use std::fmt::Write;
	Ok(match sep {
		IndexableVal::Arr(joiner_items) => {
			let mut out = Vec::new();

			let mut first = true;
			for item in arr.iter() {
				let item = item?.clone();
				if let Val::Arr(items) = item {
					if !first {
						out.reserve(joiner_items.len());
						// TODO: extend
						for item in joiner_items.iter() {
							out.push(item?);
						}
					}
					first = false;
					out.reserve(items.len());
					for item in items.iter() {
						out.push(item?);
					}
				} else if matches!(item, Val::Null) {
				} else {
					bail!("in std.join all items should be arrays");
				}
			}

			IndexableVal::Arr(out.into())
		}
		IndexableVal::Str(sep) => {
			let mut out = String::new();

			let mut first = true;
			for item in arr.iter() {
				let item = item?.clone();
				if let Val::Str(item) = item {
					if !first {
						out += &sep;
					}
					first = false;
					write!(out, "{item}").unwrap();
				} else if matches!(item, Val::Null) {
				} else {
					bail!("in std.join all items should be strings");
				}
			}

			IndexableVal::Str(out.into())
		}
	})
}

#[builtin]
pub fn builtin_lines(arr: ArrValue) -> Result<IndexableVal> {
	builtin_join(
		IndexableVal::Str("\n".into()),
		ArrValue::extended(arr, ArrValue::new(vec![Val::string("")]))
			.ok_or_else(|| error!("array is too large"))?,
	)
}

#[builtin]
pub fn builtin_resolve_path(f: String, r: String) -> String {
	let Some(pos) = f.rfind('/') else {
		return r;
	};
	format!("{}{}", &f[..=pos], r)
}

pub fn deep_join_inner(out: &mut String, arr: IndexableVal) -> Result<()> {
	use std::fmt::Write;
	match arr {
		IndexableVal::Str(s) => write!(out, "{s}").expect("no error"),
		IndexableVal::Arr(arr) => {
			for ele in arr.iter() {
				let indexable = IndexableVal::from_untyped(ele?)?;
				deep_join_inner(out, indexable)?;
			}
		}
	}
	Ok(())
}

#[builtin]
pub fn builtin_deep_join(arr: IndexableVal) -> Result<String> {
	let mut out = String::new();
	deep_join_inner(&mut out, arr)?;
	Ok(out)
}

#[builtin]
pub fn builtin_reverse(arr: ArrValue) -> ArrValue {
	arr.reversed()
}

#[builtin]
pub fn builtin_any(arr: ArrValue) -> Result<bool> {
	for v in arr.iter() {
		let v = bool::from_untyped(v?)?;
		if v {
			return Ok(true);
		}
	}
	Ok(false)
}

#[builtin]
pub fn builtin_all(arr: ArrValue) -> Result<bool> {
	for v in arr.iter() {
		let v = bool::from_untyped(v?)?;
		if !v {
			return Ok(false);
		}
	}
	Ok(true)
}

#[builtin]
pub fn builtin_member(arr: IndexableVal, x: Val) -> Result<bool> {
	match arr {
		IndexableVal::Str(str) => {
			let x: IStr = IStr::from_untyped(x)?;
			Ok(!x.is_empty() && str.contains(&*x))
		}
		IndexableVal::Arr(a) => {
			for item in a.iter() {
				let item = item?;
				if equals(&item, &x)? {
					return Ok(true);
				}
			}
			Ok(false)
		}
	}
}

#[builtin]
pub fn builtin_find(value: Val, arr: ArrValue) -> Result<Vec<usize>> {
	let mut out = Vec::new();
	for (i, ele) in arr.iter().enumerate() {
		let ele = ele?;
		if equals(&ele, &value)? {
			out.push(i);
		}
	}
	Ok(out)
}

#[builtin]
pub fn builtin_contains(arr: IndexableVal, elem: Val) -> Result<bool> {
	builtin_member(arr, elem)
}

#[builtin]
pub fn builtin_count(arr: ArrValue, x: Val) -> Result<usize> {
	let mut count = 0;
	for item in arr.iter() {
		if equals(&item?, &x)? {
			count += 1;
		}
	}
	Ok(count)
}

#[builtin]
pub fn builtin_avg(arr: Vec<f64>, onEmpty: Option<Thunk<Val>>) -> Result<Val> {
	if arr.is_empty() {
		return eval_on_empty(onEmpty);
	}
	#[expect(
		clippy::cast_precision_loss,
		reason = "array sizes are bounded to i32 len"
	)]
	Ok(Val::try_num(arr.iter().sum::<f64>() / (arr.len() as f64))?)
}

#[builtin]
pub fn builtin_remove_at(arr: ArrValue, at: i32) -> Result<ArrValue> {
	let newArrLeft = arr.clone().slice32(None, Some(at), None);
	let newArrRight = arr.slice32(Some(at + 1), None, None);

	ArrValue::extended(newArrLeft, newArrRight).ok_or_else(|| error!("array is too large"))
}

#[builtin]
pub fn builtin_remove(arr: ArrValue, elem: Val) -> Result<ArrValue> {
	for (index, item) in arr.iter().enumerate() {
		if equals(&item?, &elem)? {
			#[expect(
				clippy::cast_possible_truncation,
				clippy::cast_possible_wrap,
				reason = "array sizes are bounded to i32 len"
			)]
			return builtin_remove_at(arr.clone(), index as i32);
		}
	}
	Ok(arr)
}

#[builtin]
pub fn builtin_flatten_arrays(arrs: Vec<ArrValue>) -> Result<ArrValue> {
	pub fn flatten_inner(values: &[ArrValue]) -> Result<ArrValue> {
		if values.len() == 1 {
			return Ok(values[0].clone());
		} else if values.len() == 2 {
			return ArrValue::extended(values[0].clone(), values[1].clone())
				.ok_or_else(|| error!("array is too large"));
		}
		let (a, b) = values.split_at(values.len() / 2);
		ArrValue::extended(flatten_inner(a)?, flatten_inner(b)?)
			.ok_or_else(|| error!("array is too large"))
	}
	if arrs.is_empty() {
		return Ok(ArrValue::empty());
	} else if arrs.len() == 1 {
		return Ok(arrs.into_iter().next().expect("single"));
	}
	flatten_inner(&arrs)
}

#[builtin]
pub fn builtin_flatten_deep_array(value: Val) -> Result<Vec<Val>> {
	fn process(value: Val, out: &mut Vec<Val>) -> Result<()> {
		match value {
			Val::Arr(arr) => {
				for ele in arr.iter() {
					process(ele?, out)?;
				}
			}
			_ => out.push(value),
		}
		Ok(())
	}
	let mut out = Vec::new();
	process(value, &mut out)?;
	Ok(out)
}

#[builtin]
pub fn builtin_prune(
	a: Val,

	#[default(false)]
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
) -> Result<Val> {
	fn is_content(val: &Val) -> bool {
		match val {
			Val::Null => false,
			Val::Arr(a) => !a.is_empty(),
			Val::Obj(o) => !o.is_empty(),
			_ => true,
		}
	}
	Ok(match a {
		Val::Arr(a) => {
			let mut out = Vec::new();
			for (i, ele) in a.iter().enumerate() {
				let ele = ele
					.and_then(|v| {
						builtin_prune(
							v,
							#[cfg(feature = "exp-preserve-order")]
							preserve_order,
						)
					})
					.with_description(|| format!("elem <{i}> pruning"))?;
				if is_content(&ele) {
					out.push(ele);
				}
			}
			Val::arr(out)
		}
		Val::Obj(o) => {
			let mut out = ObjValueBuilder::new();
			for (name, value) in o.iter(
				#[cfg(feature = "exp-preserve-order")]
				preserve_order,
			) {
				let value = value
					.and_then(|v| {
						builtin_prune(
							v,
							#[cfg(feature = "exp-preserve-order")]
							preserve_order,
						)
					})
					.with_description(|| format!("field <{name}> pruning"))?;
				if !is_content(&value) {
					continue;
				}
				out.field(name).value(value);
			}
			Val::Obj(out.build())
		}
		_ => a,
	})
}
