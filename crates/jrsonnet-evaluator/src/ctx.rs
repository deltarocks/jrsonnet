use std::{clone::Clone, fmt::Debug};

use educe::Educe;
use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;

use crate::{Pending, Result, SupThis, Thunk, Val, analyze::LocalId, error, error::ErrorKind::*};

#[derive(Debug, Trace, Clone, Educe)]
#[educe(PartialEq)]
pub struct Context(#[educe(PartialEq(method = Cc::ptr_eq))] Cc<ContextInternal>);

#[derive(Debug, Trace, Clone)]
struct ContextInternal {
	sup_this: Option<SupThis>,
	/// `bindings[i]` corresponds to `LocalId(offset + i)`.
	bindings: Vec<Option<Thunk<Val>>>,
	offset: u32,
	parent: Option<Context>,
}

impl Context {
	pub fn new_future() -> Pending<Self> {
		Pending::new()
	}

	pub fn sup_this(&self) -> Option<&SupThis> {
		self.0.sup_this.as_ref()
	}

	pub fn try_sup_this(&self) -> Result<SupThis> {
		self.0
			.sup_this
			.clone()
			.ok_or_else(|| error!(CantUseSelfSupOutsideOfObject))
	}

	/// Update binding in `CoW` fashion. Only useful for eager comprehension
	/// fast-path, as it requires Cc refcount to be 1; Use `ContextBuilder` otherwise.
	pub(crate) fn cow_fill_binding(&mut self, id: LocalId, value: Thunk<Val>) {
		let mut value = Some(Some(value));

		self.0.update_with(|inner| {
			let local_idx = (id.0 - inner.offset) as usize;
			while inner.bindings.len() <= local_idx {
				inner.bindings.push(None);
			}
			inner.bindings[local_idx] = value.take().expect("called once");
		});
	}

	pub fn binding(&self, id: LocalId) -> Option<Thunk<Val>> {
		let id_num = id.0;
		if id_num >= self.0.offset {
			let local_idx = (id_num - self.0.offset) as usize;
			if let Some(Some(thunk)) = self.0.bindings.get(local_idx) {
				return Some(thunk.clone());
			}
		}
		if let Some(parent) = &self.0.parent {
			return parent.binding(id);
		}
		None
	}

	#[must_use]
	pub fn into_future(self, ctx: Pending<Self>) -> Self {
		{
			ctx.clone().fill(self);
		}
		ctx.unwrap()
	}
}

#[derive(Clone)]
pub struct ContextBuilder {
	sup_this: Option<SupThis>,
	bindings: Vec<Option<Thunk<Val>>>,
	offset: u32,
	parent: Option<Context>,
}

impl ContextBuilder {
	pub fn new() -> Self {
		Self {
			sup_this: None,
			bindings: Vec::new(),
			offset: 0,
			parent: None,
		}
	}

	pub(crate) fn extend(parent: Context, capacity: usize) -> Self {
		let offset = parent.0.offset + parent.0.bindings.len() as u32;
		Self {
			sup_this: parent.0.sup_this.clone(),
			bindings: Vec::with_capacity(capacity),
			offset,
			parent: Some(parent),
		}
	}

	pub(crate) fn bind(&mut self, id: LocalId, value: Thunk<Val>) {
		debug_assert!(
			id.0 >= self.offset,
			"cannot bind {id:?} below offset {}",
			self.offset,
		);
		let local_idx = (id.0 - self.offset) as usize;
		self.bindings.reserve(local_idx);
		while self.bindings.len() <= local_idx {
			self.bindings.push(None);
		}
		self.bindings[local_idx] = Some(value);
	}

	pub(crate) fn build(self) -> Context {
		Context(Cc::new(ContextInternal {
			sup_this: self.sup_this,
			bindings: self.bindings,
			offset: self.offset,
			parent: self.parent,
		}))
	}

	pub(crate) fn build_sup_this(mut self, st: SupThis) -> Context {
		self.sup_this = Some(st);
		self.build()
	}
}

impl Default for ContextBuilder {
	fn default() -> Self {
		Self::new()
	}
}

pub struct InitialContextBuilder {
	builder: ContextBuilder,
	externals: Vec<(IStr, LocalId)>,
	next_id: u32,
}

impl InitialContextBuilder {
	pub(crate) fn new() -> Self {
		Self {
			builder: ContextBuilder::new(),
			externals: Vec::new(),
			next_id: 0,
		}
	}

	pub fn bind(&mut self, name: impl Into<IStr>, value: Thunk<Val>) {
		let name = name.into();
		let id = LocalId(self.next_id);
		self.next_id += 1;
		self.externals.push((name, id));
		self.builder.bind(id, value);
	}

	pub(crate) fn build(self) -> (ContextBuilder, Vec<(IStr, LocalId)>) {
		(self.builder, self.externals)
	}
}

impl Default for InitialContextBuilder {
	fn default() -> Self {
		Self::new()
	}
}
