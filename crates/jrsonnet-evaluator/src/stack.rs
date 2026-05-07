use std::{cell::Cell, marker::PhantomData};

use crate::error::{Error, ErrorKind};

struct StackLimit {
	max_stack_size: Cell<usize>,
	current_depth: Cell<usize>,
}

#[cfg(nightly)]
struct NightlyLocalKey<T>(pub T);
#[cfg(nightly)]
impl<T> NightlyLocalKey<T> {
	#[inline]
	fn with<U>(&self, v: impl FnOnce(&T) -> U) -> U {
		v(&self.0)
	}
}
#[cfg(not(nightly))]
#[allow(dead_code)]
type NightlyLocalKey<T> = std::thread::LocalKey<T>;

#[cfg(nightly)]
#[macro_export]
macro_rules! const_tls {
	(const $name:ident: $t:ty = $expr:expr;) => {
		#[thread_local]
		static $name: NightlyLocalKey<$t> = NightlyLocalKey($expr);
	};
}
#[cfg(not(nightly))]
#[macro_export]
macro_rules! const_tls {
	(const $name:ident: $t:ty = $expr:expr;) => {
		thread_local! {
			static $name: $t = const { $expr };
		}
	};
}

const_tls! {
	const STACK_LIMIT: StackLimit = StackLimit {
		max_stack_size: Cell::new(200),
		current_depth: Cell::new(0),
	};
}

pub struct StackOverflowError;
impl From<StackOverflowError> for ErrorKind {
	fn from(_: StackOverflowError) -> Self {
		Self::StackOverflow
	}
}
impl From<StackOverflowError> for Error {
	fn from(_: StackOverflowError) -> Self {
		ErrorKind::StackOverflow.into()
	}
}

/// Used to implement stack depth limitation
pub struct StackDepthGuard(PhantomData<()>);
impl Drop for StackDepthGuard {
	fn drop(&mut self) {
		STACK_LIMIT.with(|limit| limit.current_depth.set(limit.current_depth.get() - 1));
	}
}

// #[cfg(feature = "nightly")]
pub fn check_depth() -> Result<StackDepthGuard, StackOverflowError> {
	STACK_LIMIT.with(|limit| {
		let current = limit.current_depth.get();
		if current < limit.max_stack_size.get() {
			limit.current_depth.set(current + 1);
			Ok(StackDepthGuard(PhantomData))
		} else {
			Err(StackOverflowError)
		}
	})
}

pub struct StackDepthLimitOverrideGuard {
	old_limit: usize,
}
impl Drop for StackDepthLimitOverrideGuard {
	fn drop(&mut self) {
		STACK_LIMIT.with(|limit| limit.max_stack_size.set(self.old_limit));
	}
}

pub fn limit_stack_depth(depth_limit: usize) -> StackDepthLimitOverrideGuard {
	STACK_LIMIT.with(|limit| {
		let old_limit = limit.max_stack_size.get();
		let current_depth = limit.current_depth.get();

		limit.max_stack_size.set(current_depth + depth_limit);
		StackDepthLimitOverrideGuard { old_limit }
	})
}

/// Like [`limit_stack_depth`], but set depth is not guarded, and will be kept
///
/// Used to implement `set_max_stack` in C api, prefer to use [`limit_stack_depth`] instead
pub fn set_stack_depth_limit(depth_limit: usize) {
	std::mem::forget(limit_stack_depth(depth_limit));
}
