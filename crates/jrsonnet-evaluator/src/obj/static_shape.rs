use std::{fmt, ops::ControlFlow, rc::Rc};

use jrsonnet_gcmodule::{Acyclic, Trace, TraceBox};
use jrsonnet_interner::IStr;
use jrsonnet_ir::Span;

use super::{
	CcObjectAssertion, EnumFields, EnumFieldsHandler, FieldVisibility, GetFor,
	HasFieldIncludeHidden, ObjFieldFlags, ObjectCore, SupThis, Visibility,
	ordering::{FieldIndex, SuperDepth},
};
use crate::{MaybeUnbound, Result};

#[derive(Acyclic, Debug)]
pub struct ShapeField {
	pub name: IStr,
	pub flags: ObjFieldFlags,
	pub location: Option<Span>,
	pub index: FieldIndex,
}

#[derive(Acyclic, Debug)]
pub struct ObjShape {
	fields: Vec<ShapeField>,
}

impl ObjShape {
	#[must_use]
	pub fn new(fields: Vec<ShapeField>) -> Self {
		Self { fields }
	}

	#[inline]
	pub fn fields(&self) -> &[ShapeField] {
		&self.fields
	}

	#[inline]
	#[must_use]
	pub fn len(&self) -> usize {
		self.fields.len()
	}

	#[inline]
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.fields.is_empty()
	}

	#[inline]
	pub fn find_index(&self, name: &IStr) -> Option<usize> {
		self.fields.iter().position(|f| &f.name == name)
	}
}

#[derive(Trace)]
pub struct StaticShapeOopObject {
	shape: Rc<ObjShape>,
	bindings: TraceBox<[MaybeUnbound]>,
	assertion: Option<CcObjectAssertion>,
}

impl fmt::Debug for StaticShapeOopObject {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("StaticShapeOopObject")
			.field("shape", &self.shape)
			.field("has_assertion", &self.assertion.is_some())
			.finish_non_exhaustive()
	}
}

impl StaticShapeOopObject {
	pub fn new(
		shape: Rc<ObjShape>,
		bindings: Vec<MaybeUnbound>,
		assertion: Option<CcObjectAssertion>,
	) -> Self {
		debug_assert_eq!(
			shape.fields.len(),
			bindings.len(),
			"shape arity must match bindings"
		);
		Self {
			shape,
			bindings: TraceBox(bindings.into_boxed_slice()),
			assertion,
		}
	}

	#[inline]
	#[must_use]
	pub const fn shape(&self) -> &Rc<ObjShape> {
		&self.shape
	}
}

impl ObjectCore for StaticShapeOopObject {
	fn enum_fields_core(
		&self,
		super_depth: &mut SuperDepth,
		handler: &mut EnumFieldsHandler<'_>,
	) -> bool {
		for field in &self.shape.fields {
			if matches!(
				handler(
					*super_depth,
					field.index,
					field.name.clone(),
					EnumFields::Normal(field.flags.visibility()),
				),
				ControlFlow::Break(())
			) {
				return false;
			}
		}
		true
	}

	fn has_field_include_hidden_core(&self, name: IStr) -> HasFieldIncludeHidden {
		if self.shape.find_index(&name).is_some() {
			HasFieldIncludeHidden::Exists
		} else {
			HasFieldIncludeHidden::NotFound
		}
	}

	fn get_for_core(&self, key: IStr, sup_this: SupThis, omit_only: bool) -> Result<GetFor> {
		if omit_only {
			return Ok(GetFor::NotFound);
		}
		let Some(i) = self.shape.find_index(&key) else {
			return Ok(GetFor::NotFound);
		};
		let field = &self.shape.fields[i];
		let v = self.bindings[i].evaluate(sup_this)?;
		Ok(if field.flags.add() {
			GetFor::SuperPlus(v)
		} else {
			GetFor::Final(v)
		})
	}

	fn field_visibility_core(&self, field: IStr) -> FieldVisibility {
		self.shape
			.find_index(&field)
			.map_or(FieldVisibility::NotFound, |i| {
				FieldVisibility::Found(self.shape.fields[i].flags.visibility())
			})
	}

	fn run_assertions_core(&self, sup_this: SupThis) -> Result<()> {
		if let Some(assertion) = &self.assertion {
			assertion.0.run(sup_this)?;
		}
		Ok(())
	}

	fn has_assertion(&self) -> bool {
		self.assertion.is_some()
	}
}

pub struct ObjShapeBuilder {
	fields: Vec<ShapeField>,
	next_index: FieldIndex,
}

impl ObjShapeBuilder {
	#[must_use]
	pub fn new() -> Self {
		Self {
			fields: Vec::new(),
			next_index: FieldIndex::default(),
		}
	}

	#[must_use]
	pub fn with_capacity(cap: usize) -> Self {
		Self {
			fields: Vec::with_capacity(cap),
			next_index: FieldIndex::default(),
		}
	}

	pub fn field(
		&mut self,
		name: impl Into<IStr>,
		visibility: Visibility,
		add: bool,
		location: Option<Span>,
	) -> &mut Self {
		let index = self.next_index;
		self.next_index = self.next_index.next();
		self.fields.push(ShapeField {
			name: name.into(),
			flags: ObjFieldFlags::new(add, visibility),
			location,
			index,
		});
		self
	}

	#[must_use]
	pub fn build(self) -> ObjShape {
		ObjShape::new(self.fields)
	}
}

impl Default for ObjShapeBuilder {
	fn default() -> Self {
		Self::new()
	}
}
