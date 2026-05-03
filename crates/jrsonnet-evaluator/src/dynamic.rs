use std::{hash::Hasher, ptr::addr_of};

use jrsonnet_gcmodule::Cc;

pub fn identity_hash<T, H: Hasher>(v: &Cc<T>, hasher: &mut H) {
	hasher.write_usize(addr_of!(**v) as usize);
}
