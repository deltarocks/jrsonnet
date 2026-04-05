use std::fmt::Debug;

use educe::Educe;
use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
	ObjValue, Pending, Result, SupThis, Thunk, Val, bail, error::ErrorKind::*,
	gc::WithCapacityExt as _,
};
/// Context keeps information about current lexical code location
///
/// This information includes local variables, top-level object (`$`), current object (`this`), and super object (`super`)
#[derive(Debug, Trace, Clone, Educe)]
#[educe(PartialEq)]
pub struct Context(#[educe(PartialEq(method = Cc::ptr_eq))] Cc<ContextInternal>);

#[derive(Debug, Trace)]
struct ContextInternal {
	dollar: Option<ObjValue>,
	sup_this: Option<SupThis>,
	bindings: FxHashMap<IStr, Thunk<Val>>,

	branch_point: Option<Context>,
}
impl Context {
	pub fn new_future() -> Pending<Self> {
		Pending::new()
	}

	pub fn dollar(&self) -> Option<&ObjValue> {
		self.0.dollar.as_ref()
	}

	pub fn try_dollar(&self) -> Result<ObjValue> {
		self.0
			.dollar
			.clone()
			.ok_or_else(|| CantUseSelfSupOutsideOfObject.into())
	}

	pub fn this(&self) -> Option<&ObjValue> {
		self.0.sup_this.as_ref().map(SupThis::this)
	}

	pub fn try_this(&self) -> Result<ObjValue> {
		self.0
			.sup_this
			.as_ref()
			.ok_or_else(|| CantUseSelfSupOutsideOfObject.into())
			.map(SupThis::this)
			.cloned()
	}

	pub fn sup_this(&self) -> Option<&SupThis> {
		self.0.sup_this.as_ref()
	}

	pub fn try_sup_this(&self) -> Result<SupThis> {
		self.0
			.sup_this
			.clone()
			.ok_or_else(|| CantUseSelfSupOutsideOfObject.into())
	}

	pub fn binding(&self, name: IStr) -> Result<Thunk<Val>> {
		use std::cmp::Ordering;

		use crate::bail;

		if let Some(val) = self.0.bindings.get(&name).cloned() {
			return Ok(val);
		}

		if let Some(branch_point) = &self.0.branch_point {
			return branch_point.binding(name);
		}

		let mut heap = Vec::new();
		for k in self.0.bindings.keys() {
			let conf = strsim::jaro_winkler(k as &str, &name as &str);
			if conf < 0.8 {
				continue;
			}
			heap.push((conf, k.clone()));
		}
		heap.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

		bail!(VariableIsNotDefined(
			name,
			heap.into_iter().map(|(_, k)| k).collect()
		))
	}
	pub fn contains_binding(&self, name: IStr) -> bool {
		self.0.bindings.contains_key(&name)
	}
	#[must_use]
	pub fn into_future(self, ctx: Pending<Self>) -> Self {
		{
			ctx.clone().fill(self);
		}
		ctx.unwrap()
	}

	#[must_use]
	pub fn branch_point(self) -> Self {
		if self.0.bindings.is_empty() {
			self
		} else {
			ContextBuilder::extend(self).build()
		}
	}
}

#[derive(Clone)]
pub struct ContextBuilder {
	dollar: Option<ObjValue>,
	sup_this: Option<SupThis>,
	bindings: FxHashMap<IStr, Thunk<Val>>,
	filled: FxHashSet<IStr>,
	branch_point: Option<Context>,
}

impl ContextBuilder {
	pub fn new() -> Self {
		Self {
			dollar: None,
			sup_this: None,
			bindings: FxHashMap::new(),
			filled: FxHashSet::new(),
			branch_point: None,
		}
	}

	pub fn extend_fast(parent: Context) -> Self {
		Self {
			dollar: parent.0.dollar.clone(),
			sup_this: parent.0.sup_this.clone(),
			bindings: parent.0.bindings.clone(),
			filled: FxHashSet::new(),
			branch_point: parent.0.branch_point.clone(),
		}
	}

	pub fn extend(parent: Context) -> Self {
		Self {
			dollar: parent.0.dollar.clone(),
			sup_this: parent.0.sup_this.clone(),
			bindings: FxHashMap::new(),
			filled: FxHashSet::new(),
			branch_point: Some(parent.clone()),
		}
	}

	pub fn bind(&mut self, name: impl Into<IStr>, value: Thunk<Val>) {
		let _ = self.bindings.insert(name.into(), value);
	}
	/// After commit, binds would shadow the previous declarations
	#[must_use]
	pub fn commit(mut self) -> Self {
		self.filled.clear();
		self
	}
	pub fn try_bind(&mut self, name: impl Into<IStr>, value: Thunk<Val>) -> Result<()> {
		let name = name.into();
		if !self.filled.insert(name.clone()) {
			bail!(DuplicateLocalVar(name))
		}
		self.bind(name, value);
		Ok(())
	}
	pub fn build(self) -> Context {
		Context(Cc::new(ContextInternal {
			dollar: self.dollar,
			sup_this: self.sup_this,
			bindings: self.bindings,
			branch_point: self.branch_point,
		}))
	}
	pub fn build_sup_this(mut self, st: SupThis) -> Context {
		if self.dollar.is_none() {
			self.dollar = Some(st.this().clone());
		}
		self.sup_this = Some(st);
		self.build()
	}
}

impl Default for ContextBuilder {
	fn default() -> Self {
		Self::new()
	}
}
