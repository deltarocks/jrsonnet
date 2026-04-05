//! Modify VM values
//! Only tested with variables, which haven't altered by code before appearing here
//! In jrsonnet every value is immutable, and this code is probally broken

use std::{ffi::CStr, os::raw::c_char};

use jrsonnet_evaluator::{Thunk, Val};

use crate::VM;

/// Adds value to the end of the array `arr`.
///
/// # Safety
///
/// `arr` should be a pointer to array value allocated by `make_array`, or returned by other library call
/// `val` should be a pointer to value allocated using this library
#[no_mangle]
pub unsafe extern "C" fn jsonnet_json_array_append(_vm: &VM, arr: &mut Val, val: &Val) {
	match arr {
		Val::Arr(old) => {
			let mut new = Vec::new();
			for item in old.iter_lazy() {
				new.push(item);
			}

			new.push(Thunk::evaluated(val.clone()));
			*arr = Val::arr(new);
		}
		_ => panic!("should receive array"),
	}
}

/// Adds the field to the object, bound to value.
///
/// This shadows any previous binding of the field.
///
/// # Safety
///
/// `obj` should be a pointer to object value allocated by `make_object`, or returned by other library call
/// `name` should be NUL-terminated string
#[no_mangle]
pub unsafe extern "C" fn jsonnet_json_object_append(
	_vm: &VM,
	obj: &mut Val,
	name: *const c_char,
	val: &Val,
) {
	match obj {
		Val::Obj(old) => old
			.extend_field(unsafe { CStr::from_ptr(name).to_str().unwrap().into() })
			.value(val.clone()),
		_ => panic!("should receive object"),
	}
}
