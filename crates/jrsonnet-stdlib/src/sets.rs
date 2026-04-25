use std::cmp::Ordering;

use jrsonnet_evaluator::{Result, Thunk, Val, function::builtin, val::ArrValue};

use crate::keyf::KeyF;

#[builtin]
#[allow(non_snake_case)]
pub fn builtin_set_member(x: Thunk<Val>, arr: ArrValue, #[default] keyF: KeyF) -> Result<bool> {
	let mut low = 0;
	let mut high = arr.len();

	let x = keyF.eval(x)?;

	while low < high {
		let middle = u32::midpoint(high, low);
		let comp = keyF.eval(arr.get_lazy(middle).expect("in bounds"))?;
		match Val::try_cmp(&comp, &x)? {
			Ordering::Less => low = middle + 1,
			Ordering::Equal => return Ok(true),
			Ordering::Greater => high = middle,
		}
	}
	Ok(false)
}

#[builtin]
#[allow(non_snake_case, clippy::redundant_closure)]
pub fn builtin_set_inter(
	a: ArrValue,
	b: ArrValue,
	#[default] keyF: KeyF,
) -> Result<Vec<Thunk<Val>>> {
	let mut a = a.iter_lazy();
	let mut b = b.iter_lazy();

	let keyF = |v| keyF.eval(v);

	let mut av = a.next();
	let mut bv = b.next();
	let mut ak = av.clone().map(keyF).transpose()?;
	let mut bk = bv.map(keyF).transpose()?;

	let mut out = Vec::new();
	while let (Some(ac), Some(bc)) = (&ak, &bk) {
		match Val::try_cmp(ac, bc)? {
			Ordering::Less => {
				av = a.next();
				ak = av.clone().map(keyF).transpose()?;
			}
			Ordering::Greater => {
				bv = b.next();
				bk = bv.map(keyF).transpose()?;
			}
			Ordering::Equal => {
				out.push(av.clone().expect("ak != None => av != None"));
				av = a.next();
				ak = av.clone().map(keyF).transpose()?;
				bv = b.next();
				bk = bv.map(keyF).transpose()?;
			}
		}
	}
	Ok(out)
}

#[builtin]
#[allow(non_snake_case, clippy::redundant_closure)]
pub fn builtin_set_diff(
	a: ArrValue,
	b: ArrValue,
	#[default] keyF: KeyF,
) -> Result<Vec<Thunk<Val>>> {
	let mut a = a.iter_lazy();
	let mut b = b.iter_lazy();

	let keyF = |v| keyF.eval(v);

	let mut av = a.next();
	let mut bv = b.next();
	let mut ak = av.clone().map(keyF).transpose()?;
	let mut bk = bv.map(keyF).transpose()?;

	let mut out = Vec::new();
	while let (Some(ac), Some(bc)) = (&ak, &bk) {
		match Val::try_cmp(ac, bc)? {
			Ordering::Less => {
				// In a, but not in b
				out.push(av.clone().expect("ak != None"));
				av = a.next();
				ak = av.clone().map(keyF).transpose()?;
			}
			Ordering::Greater => {
				bv = b.next();
				bk = bv.map(keyF).transpose()?;
			}
			Ordering::Equal => {
				av = a.next();
				ak = av.clone().map(keyF).transpose()?;
				bv = b.next();
				bk = bv.map(keyF).transpose()?;
			}
		}
	}
	while let Some(_ac) = &ak {
		// In a, but not in b
		out.push(av.clone().expect("ak != None"));
		av = a.next();
		ak = av.clone().map(keyF).transpose()?;
	}
	Ok(out)
}

#[builtin]
#[allow(non_snake_case, clippy::redundant_closure)]
pub fn builtin_set_union(
	a: ArrValue,
	b: ArrValue,
	#[default] keyF: KeyF,
) -> Result<Vec<Thunk<Val>>> {
	let mut a = a.iter_lazy();
	let mut b = b.iter_lazy();

	let keyF = |v| keyF.eval(v);

	let mut av = a.next();
	let mut bv = b.next();
	let mut ak = av.clone().map(keyF).transpose()?;
	let mut bk = bv.clone().map(keyF).transpose()?;

	let mut out = Vec::new();
	while let (Some(ac), Some(bc)) = (&ak, &bk) {
		match Val::try_cmp(ac, bc)? {
			Ordering::Less => {
				out.push(av.clone().expect("ak != None"));
				av = a.next();
				ak = av.clone().map(keyF).transpose()?;
			}
			Ordering::Greater => {
				out.push(bv.clone().expect("bk != None"));
				bv = b.next();
				bk = bv.clone().map(keyF).transpose()?;
			}
			Ordering::Equal => {
				// NOTE: order matters, values in `a` win
				out.push(av.clone().expect("ak != None"));
				av = a.next();
				ak = av.clone().map(keyF).transpose()?;
				bv = b.next();
				bk = bv.clone().map(keyF).transpose()?;
			}
		}
	}
	// a.len() > b.len()
	while let Some(_ac) = &ak {
		out.push(av.clone().expect("ak != None"));
		av = a.next();
		ak = av.clone().map(keyF).transpose()?;
	}
	// b.len() > a.len()
	while let Some(_bc) = &bk {
		out.push(bv.clone().expect("ak != None"));
		bv = b.next();
		bk = bv.clone().map(keyF).transpose()?;
	}
	Ok(out)
}
