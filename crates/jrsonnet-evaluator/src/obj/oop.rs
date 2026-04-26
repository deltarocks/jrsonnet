use std::{
	cell::{Cell, RefCell},
	fmt, mem,
	ops::ControlFlow,
};

use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_ir::IStr;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
	CcObjectAssertion, CcObjectCore, EnumFields, EnumFieldsHandler, FieldVisibility, GetFor,
	HasFieldIncludeHidden, ObjMember, ObjMemberBuilder, ObjValue, ObjValueInner, ObjectAssertion,
	ObjectCore, OmitFieldsCore, SupThis,
	ordering::{FieldIndex, SuperDepth},
};
use crate::{
	CcUnbound, MaybeUnbound, Result, Thunk, Unbound, Val, bail,
	error::ErrorKind::*,
	function::{CallLocation, FuncVal},
	gc::WithCapacityExt as _,
	in_frame,
};

#[allow(clippy::module_name_repetitions)]
#[derive(Trace, Default)]
#[trace(tracking(force))]
pub struct OopObject {
	assertion: Option<CcObjectAssertion>,
	this_entries: FxHashMap<IStr, (ObjMember, FieldIndex)>,
}
impl fmt::Debug for OopObject {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("OopObject")
			.field("this_entries", &self.this_entries)
			.finish_non_exhaustive()
	}
}
impl OopObject {
	fn is_empty(&self) -> bool {
		self.assertion.is_none() && self.this_entries.is_empty()
	}
}
impl OopObject {
	pub fn new(
		this_entries: FxHashMap<IStr, (ObjMember, FieldIndex)>,
		assertion: Option<CcObjectAssertion>,
	) -> Self {
		Self {
			assertion,
			this_entries,
		}
	}
}

impl ObjectCore for OopObject {
	fn enum_fields_core(
		&self,
		super_depth: &mut SuperDepth,
		handler: &mut EnumFieldsHandler<'_>,
	) -> bool {
		for (name, (member, idx)) in &self.this_entries {
			if matches!(
				handler(
					*super_depth,
					*idx,
					name.clone(),
					EnumFields::Normal(member.flags.visibility()),
				),
				ControlFlow::Break(())
			) {
				return false;
			}
		}
		true
	}

	fn has_field_include_hidden_core(&self, name: IStr) -> HasFieldIncludeHidden {
		if self.this_entries.contains_key(&name) {
			HasFieldIncludeHidden::Exists
		} else {
			HasFieldIncludeHidden::NotFound
		}
	}

	fn get_for_core(&self, key: IStr, sup_this: SupThis, omit_only: bool) -> Result<GetFor> {
		if omit_only {
			return Ok(GetFor::NotFound);
		}
		match self.this_entries.get(&key) {
			Some((k, _)) => {
				let v = k.invoke.evaluate(sup_this)?;
				Ok(if k.flags.add() {
					GetFor::SuperPlus(v)
				} else {
					GetFor::Final(v)
				})
			}
			None => Ok(GetFor::NotFound),
		}
	}
	fn field_visibility_core(&self, name: IStr) -> FieldVisibility {
		self.this_entries
			.get(&name)
			.map_or(FieldVisibility::NotFound, |(f, _)| {
				FieldVisibility::Found(f.flags.visibility())
			})
	}

	fn run_assertions_core(&self, sup_this: SupThis) -> Result<()> {
		if let Some(assertion) = &self.assertion {
			assertion.0.run(sup_this)?;
		}
		Ok(())
	}
}

#[allow(clippy::module_name_repetitions)]
pub struct ObjValueBuilder {
	sup: Vec<CcObjectCore>,
	has_assertions: bool,

	new: OopObject,
	next_field_index: FieldIndex,
}
impl ObjValueBuilder {
	pub fn new() -> Self {
		Self::with_capacity(0)
	}
	pub fn with_capacity(capacity: usize) -> Self {
		Self {
			sup: Vec::new(),
			has_assertions: false,
			new: OopObject::new(FxHashMap::with_capacity(capacity), None),
			next_field_index: FieldIndex::default(),
		}
	}
	pub fn reserve_fields(&mut self, capacity: usize) {
		self.new.this_entries.reserve(capacity);
	}
	pub fn with_super(&mut self, super_obj: ObjValue) -> &mut Self {
		self.has_assertions |= super_obj.0.has_assertions;
		self.sup.clone_from(&super_obj.0.cores);
		self
	}

	pub fn assert(&mut self, assertion: impl ObjectAssertion + 'static) -> &mut Self {
		assert!(
			self.new.assertion.is_none(),
			"one OopObject can only have one assertion"
		);
		self.has_assertions = true;
		self.new.assertion = Some(CcObjectAssertion::new(assertion));
		self
	}
	pub fn field(&mut self, name: impl Into<IStr>) -> ObjMemberBuilder<ValueBuilder<'_>> {
		let field_index = self.next_field_index;
		self.next_field_index = self.next_field_index.next();
		ObjMemberBuilder::new(ValueBuilder(self), name.into(), field_index)
	}
	/// Preset for common method definiton pattern:
	/// Create a hidden field with the function value.
	///
	/// `.field(name).hide().value(Val::function(value))`
	pub fn method(&mut self, name: impl Into<IStr>, value: impl Into<FuncVal>) -> &mut Self {
		self.field(name).hide().value(Val::Func(value.into()));
		self
	}
	pub fn try_method(
		&mut self,
		name: impl Into<IStr>,
		value: impl Into<FuncVal>,
	) -> Result<&mut Self> {
		self.field(name).hide().try_value(Val::Func(value.into()))?;
		Ok(self)
	}

	pub fn extend_with_core(&mut self, core: impl ObjectCore) {
		self.commit();
		self.sup.push(CcObjectCore::new(core));
	}

	fn commit(&mut self) {
		if !self.new.is_empty() {
			self.sup.push(CcObjectCore::new(mem::take(&mut self.new)));
		}
		self.next_field_index = FieldIndex::default();
	}

	pub fn with_fields_omitted(&mut self, omit: FxHashSet<IStr>) {
		self.commit();
		self.sup.push(CcObjectCore::new(OmitFieldsCore {
			omit,
			prev_layers: self.sup.len(),
		}));
	}

	pub fn build(mut self) -> ObjValue {
		self.commit();
		if self.sup.is_empty() {
			return ObjValue::empty();
		}
		let has_assertions = self.has_assertions;
		ObjValue(Cc::new(ObjValueInner {
			cores: self.sup,
			assertions_ran: Cell::new(!has_assertions),
			has_assertions,
			value_cache: RefCell::default(),
		}))
	}
}
impl Default for ObjValueBuilder {
	fn default() -> Self {
		Self::with_capacity(0)
	}
}

pub struct ValueBuilder<'v>(&'v mut ObjValueBuilder);
impl ObjMemberBuilder<ValueBuilder<'_>> {
	/// Inserts value, replacing if it is already defined
	pub fn value(self, value: impl Into<Val>) {
		let (receiver, name, idx, member) =
			self.build_member(MaybeUnbound::Bound(Thunk::evaluated(value.into())));
		let entry = receiver.0.new.this_entries.entry(name);
		entry.insert_entry((member, idx));
	}
	/// Inserts thunk, replacing if it is already defined
	pub fn thunk(self, value: impl Into<Thunk<Val>>) {
		let (receiver, name, idx, member) = self.build_member(MaybeUnbound::Bound(value.into()));
		let entry = receiver.0.new.this_entries.entry(name);
		entry.insert_entry((member, idx));
	}

	/// Tries to insert value, returns an error if it was already defined
	pub fn try_value(self, value: impl Into<Val>) -> Result<()> {
		self.try_thunk(Thunk::evaluated(value.into()))
	}
	pub fn try_thunk(self, value: impl Into<Thunk<Val>>) -> Result<()> {
		self.binding(MaybeUnbound::Bound(value.into()))
	}
	pub fn bindable(self, bindable: impl Unbound<Bound = Val>) -> Result<()> {
		self.binding(MaybeUnbound::Unbound(CcUnbound::new(bindable)))
	}
	pub fn binding(self, binding: MaybeUnbound) -> Result<()> {
		let (receiver, name, idx, member) = self.build_member(binding);
		let location = member.location.clone();
		let old = receiver
			.0
			.new
			.this_entries
			.insert(name.clone(), (member, idx));
		if old.is_some() {
			in_frame(
				CallLocation(location.as_ref()),
				|| format!("field <{}> initializtion", name.clone()),
				|| bail!(DuplicateFieldName(name.clone())),
			)?;
		}
		Ok(())
	}
}
