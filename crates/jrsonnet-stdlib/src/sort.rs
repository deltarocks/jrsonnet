#![allow(non_snake_case)]

use std::cmp::Ordering;

use jrsonnet_evaluator::{
	Result, Thunk, Val, bail,
	function::builtin,
	val::{ArrValue, equals},
};

use crate::{eval_on_empty, keyf::KeyF};

#[derive(Copy, Clone)]
enum SortKeyType {
	Number,
	String,
	Unspecialized,
	Unknown,
}

fn get_sort_type<T>(values: &[T], key_getter: impl Fn(&T) -> &Val) -> Result<SortKeyType> {
	let mut sort_type = SortKeyType::Unknown;
	for i in values {
		let i = key_getter(i);
		match (i, sort_type) {
			(Val::Str(_), SortKeyType::Unknown) => sort_type = SortKeyType::String,
			(Val::Num(_), SortKeyType::Unknown) => sort_type = SortKeyType::Number,
			(Val::Str(_), SortKeyType::String) | (Val::Num(_), SortKeyType::Number) => {}
			(Val::Str(_) | Val::Num(_), _) => {
				bail!("sort elements should have the same types")
			}
			(_, _) => return Ok(SortKeyType::Unspecialized),
		}
	}
	Ok(sort_type)
}

fn sort_identity(mut values: Vec<Val>) -> Result<Vec<Val>> {
	// Fast path, identity key getter
	let sort_type = get_sort_type(&values, |k| k)?;
	match sort_type {
		SortKeyType::Number => values.sort_unstable_by_key(|v| match v {
			Val::Num(n) => *n,
			_ => unreachable!(),
		}),
		SortKeyType::String => values.sort_unstable_by_key(|v| match v {
			Val::Str(s) => s.clone(),
			_ => unreachable!(),
		}),
		SortKeyType::Unknown | SortKeyType::Unspecialized => {
			let mut err = None;
			// evaluate_compare_op will never return equal on types, which are different from
			// jsonnet perspective
			values.sort_unstable_by(|a, b| match Val::try_cmp(a, b) {
				Ok(ord) => ord,
				Err(e) if err.is_none() => {
					let _ = err.insert(e);
					Ordering::Equal
				}
				Err(_) => Ordering::Equal,
			});
			if let Some(err) = err {
				return Err(err);
			}
		}
	}
	Ok(values)
}

fn sort_keyf(values: ArrValue, keyf: KeyF) -> Result<Vec<Thunk<Val>>> {
	// Slow path, user provided key getter
	let mut vk = Vec::with_capacity(values.len() as usize);
	for value in values.iter_lazy() {
		vk.push((value.clone(), keyf.eval(value)?));
	}
	let sort_type = get_sort_type(&vk, |v| &v.1)?;
	match sort_type {
		SortKeyType::Number => vk.sort_by_key(|v| match v.1 {
			Val::Num(n) => n,
			_ => unreachable!(),
		}),
		SortKeyType::String => vk.sort_by_key(|v| match &v.1 {
			Val::Str(s) => s.clone(),
			_ => unreachable!(),
		}),
		SortKeyType::Unknown | SortKeyType::Unspecialized => {
			let mut err = None;
			// evaluate_compare_op will never return equal on types, which are different from
			// jsonnet perspective
			vk.sort_by(|(_a, ak), (_b, bk)| match Val::try_cmp(ak, bk) {
				Ok(ord) => ord,
				Err(e) if err.is_none() => {
					let _ = err.insert(e);
					Ordering::Equal
				}
				Err(_) => Ordering::Equal,
			});
			if let Some(err) = err {
				return Err(err);
			}
		}
	}
	Ok(vk.into_iter().map(|v| v.0).collect())
}

/// * `key_getter` - None, if identity sort required
pub fn sort(values: ArrValue, key_getter: KeyF) -> Result<ArrValue> {
	if values.len() <= 1 {
		return Ok(values);
	}
	if key_getter.is_identity() {
		Ok(ArrValue::new(sort_identity(
			values.iter().collect::<Result<Vec<Val>>>()?,
		)?))
	} else {
		Ok(ArrValue::new(sort_keyf(values, key_getter)?))
	}
}

#[builtin]
pub fn builtin_sort(arr: ArrValue, #[default] keyF: KeyF) -> Result<ArrValue> {
	super::sort::sort(arr, keyF)
}

fn uniq_identity(arr: Vec<Val>) -> Result<Vec<Val>> {
	let mut out = Vec::new();
	let mut last = arr[0].clone();
	out.push(last.clone());
	for next in arr.into_iter().skip(1) {
		if !equals(&last, &next)? {
			out.push(next.clone());
		}
		last = next;
	}
	Ok(out)
}

fn uniq_keyf(arr: ArrValue, keyf: KeyF) -> Result<Vec<Thunk<Val>>> {
	let mut out = Vec::new();
	let last_value = arr.get_lazy(0).unwrap();
	let mut last_key = keyf.eval(last_value.clone())?;
	out.push(last_value);

	for next in arr.iter_lazy().skip(1) {
		let next_key = keyf.eval(next.clone())?;
		if !equals(&last_key, &next_key)? {
			out.push(next.clone());
		}
		last_key = next_key;
	}
	Ok(out)
}

#[builtin]
#[allow(non_snake_case)]
pub fn builtin_uniq(arr: ArrValue, #[default] keyF: KeyF) -> Result<ArrValue> {
	if arr.len() <= 1 {
		return Ok(arr);
	}
	if keyF.is_identity() {
		Ok(ArrValue::new(uniq_identity(
			arr.iter().collect::<Result<Vec<Val>>>()?,
		)?))
	} else {
		Ok(ArrValue::new(uniq_keyf(arr, keyF)?))
	}
}

#[builtin]
#[allow(non_snake_case)]
pub fn builtin_set(arr: ArrValue, #[default] keyF: KeyF) -> Result<ArrValue> {
	if arr.len() <= 1 {
		return Ok(arr);
	}
	if keyF.is_identity() {
		let arr = arr.iter().collect::<Result<Vec<Val>>>()?;
		let arr = sort_identity(arr)?;
		let arr = uniq_identity(arr)?;
		Ok(ArrValue::new(arr))
	} else {
		let arr = sort_keyf(arr, keyF.clone())?;
		let arr = uniq_keyf(ArrValue::new(arr), keyF)?;
		Ok(ArrValue::new(arr))
	}
}

fn array_top1(arr: ArrValue, keyf: KeyF, ordering: Ordering) -> Result<Val> {
	let mut iter = arr.iter();
	let mut min = iter.next().expect("not empty")?;
	let mut min_key = keyf.eval(Thunk::evaluated(min.clone()))?;
	for item in iter {
		let cur = item?;
		let cur_key = keyf.eval(Thunk::evaluated(cur.clone()))?;
		if Val::try_cmp(&cur_key, &min_key)? == ordering {
			min = cur;
			min_key = cur_key;
		}
	}
	Ok(min)
}

#[builtin]
pub fn builtin_min_array(
	arr: ArrValue,
	#[default] keyF: KeyF,
	onEmpty: Option<Thunk<Val>>,
) -> Result<Val> {
	if arr.is_empty() {
		return eval_on_empty(onEmpty);
	}
	array_top1(arr, keyF, Ordering::Less)
}
#[builtin]
pub fn builtin_max_array(
	arr: ArrValue,
	#[default] keyF: KeyF,
	onEmpty: Option<Thunk<Val>>,
) -> Result<Val> {
	if arr.is_empty() {
		return eval_on_empty(onEmpty);
	}
	array_top1(arr, keyF, Ordering::Greater)
}
