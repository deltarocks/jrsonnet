use std::{
	cell::{Cell, OnceCell, RefCell},
	clone::Clone,
	fmt::{self, Debug},
};

use educe::Educe;
use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;

use crate::{
	Result, SupThis, Thunk, Val,
	analyze::{CaptureSlot, ClosureShape, LSlot, LocalId, LocalSlot},
	bail, error,
	error::ErrorKind::*,
};

#[derive(Debug, Trace, Clone, Educe)]
#[educe(PartialEq)]
pub struct Context(#[educe(PartialEq(method = Cc::ptr_eq))] pub(crate) Cc<ContextInternal>);

#[derive(Trace)]
pub(crate) struct ContextInternal {
	/// Immutable, packed at closure-create time.
	pub(crate) captures: Cc<Vec<Thunk<Val>>>,
	/// Filled during closure initialization
	pub(crate) locals: Cc<LocalsFrame>,
	pub(crate) sup_this: Option<SupThis>,
}

impl Debug for ContextInternal {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("ContextInternal")
			.field("captures", &self.captures.len())
			.field("locals", &self.locals)
			.field("sup_this", &self.sup_this.is_some())
			.finish()
	}
}

#[derive(Trace, Debug)]
pub(crate) struct IterFrame {
	slots: Vec<RefCell<Option<Thunk<Val>>>>,
	captured: Cell<bool>,
}
impl IterFrame {
	pub fn new(n: u16) -> IterFrame {
		let cells: Vec<RefCell<Option<Thunk<Val>>>> = (0..n).map(|_| RefCell::new(None)).collect();
		IterFrame {
			slots: cells,
			captured: Cell::new(false),
		}
	}
	pub fn set(&self, slot: LocalSlot, value: Thunk<Val>) {
		*self.slots[slot.0 as usize].borrow_mut() = Some(value);
	}
}

#[derive(Trace, Debug)]
pub(crate) enum LocalsFrame {
	Once1(OnceCell<Thunk<Val>>),
	/// Letrec/function/object/for frames - slots are filled during frame setup
	Once(Vec<OnceCell<Thunk<Val>>>),
	/// Comp-eager fast-path, cells are reset per iteration for the unique frames (i.e for the non-capturing thunks)
	Iter(IterFrame),
}
impl LocalsFrame {
	pub fn set(&self, slot: LocalSlot, value: Thunk<Val>) {
		match self {
			LocalsFrame::Once1(cell) => {
				debug_assert_eq!(slot.0, 0, "Once1 only holds slot 0");
				cell.set(value)
					.map_err(|_| ())
					.expect("slot already filled");
			}
			LocalsFrame::Once(cells) => {
				cells[slot.0 as usize]
					.set(value)
					.map_err(|_| ())
					.expect("slot already filled");
			}
			LocalsFrame::Iter(_) => unreachable!("iter frame has different constructors"),
		}
	}
}

impl LocalsFrame {
	pub(crate) fn new_once(n: u16) -> Cc<Self> {
		if n == 1 {
			return Cc::new(Self::Once1(OnceCell::new()));
		}
		let cells: Vec<OnceCell<Thunk<Val>>> = (0..n).map(|_| OnceCell::new()).collect();
		Cc::new(Self::Once(cells))
	}
}

pub(crate) struct IterContext {
	context: Context,
}
impl IterContext {
	pub(crate) fn create(&self, build: impl FnOnce(&IterFrame)) -> Result<Context> {
		if !Cc::is_unique(&self.context.0.locals) {
			bail!(EagerCompspecCaptured);
		}
		let LocalsFrame::Iter(frame) = &*self.context.0.locals else {
			unreachable!("IterContext is only created for Iter ctx");
		};
		if frame.captured.get() {
			bail!(EagerCompspecCaptured);
		}
		build(frame);
		Ok(self.context.clone())
	}
}

#[derive(Trace, Clone)]
pub(crate) struct PackedContext {
	captures: Cc<Vec<Thunk<Val>>>,
	n_locals: u16,
}
impl PackedContext {
	pub fn enter(self, sup_this: SupThis, build: impl FnOnce(&LocalsFrame, &Context)) -> Context {
		let locals = LocalsFrame::new_once(self.n_locals);
		let val = Context(Cc::new(ContextInternal {
			captures: self.captures,
			locals,
			sup_this: Some(sup_this),
		}));
		build(&val.0.locals, &val);
		val
	}
}
#[derive(Trace, Clone, Educe, Debug)]
#[educe(PartialEq)]
pub(crate) struct PackedContextSupThis {
	#[educe(PartialEq(method = Cc::ptr_eq))]
	captures: Cc<Vec<Thunk<Val>>>,
	n_locals: u16,
	sup_this: Option<SupThis>,
}
impl PackedContextSupThis {
	pub fn enter(self, build: impl FnOnce(&LocalsFrame, &Context)) -> Context {
		let locals = LocalsFrame::new_once(self.n_locals);
		let val = Context(Cc::new(ContextInternal {
			captures: self.captures.clone(),
			locals,
			sup_this: self.sup_this,
		}));
		build(&val.0.locals, &val);
		val
	}
}

impl Context {
	#[inline]
	pub fn slot(&self, slot: LSlot) -> Thunk<Val> {
		match slot {
			LSlot::Local(i) => self.local(i),
			LSlot::Capture(i) => self.capture(i),
		}
	}
	/// Read a local slot from the shared locals frame.
	///
	/// # Panics
	/// If the slot has not yet been filled. The analyzer guarantees
	/// that slot indices are in range and that letrec setup completes
	/// before the first read. A panic indicates an analyzer/runtime
	/// invariant violation, not a user error.
	#[inline]
	pub fn local(&self, slot: LocalSlot) -> Thunk<Val> {
		match &*self.0.locals {
			LocalsFrame::Once1(cell) => {
				debug_assert_eq!(slot.0, 0, "Once1 only holds slot 0");
				cell.get().expect("local read before letrec init").clone()
			}
			LocalsFrame::Once(cells) => cells[slot.0 as usize]
				.get()
				.expect("local read before letrec init")
				.clone(),
			LocalsFrame::Iter(cells) => cells.slots[slot.0 as usize]
				.borrow()
				.as_ref()
				.expect("iter local read before iteration filled it")
				.clone(),
		}
	}

	/// Read a captured slot from this closure's capture pack.
	#[inline]
	pub fn capture(&self, slot: CaptureSlot) -> Thunk<Val> {
		(*self.0.captures)[slot.0 as usize].clone()
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

	/// Build a root context: empty captures, externals filled into a
	/// fresh Once locals frame in declaration order. Used once at
	/// program entry to construct the context the analyzed root LIR
	/// runs against.
	pub(crate) fn root(externals: Vec<Thunk<Val>>) -> Self {
		let n: u16 = externals
			.len()
			.try_into()
			.expect("more than u16::MAX externals");
		let cells: Vec<OnceCell<Thunk<Val>>> = externals
			.into_iter()
			.map(|t| {
				let cell = OnceCell::new();
				cell.set(t).map_err(|_| ()).expect("fresh cell");
				cell
			})
			.collect();
		debug_assert_eq!(cells.len(), n as usize);
		let locals = Cc::new(LocalsFrame::Once(cells));
		Self(Cc::new(ContextInternal {
			captures: Cc::new(Vec::new()),
			locals,
			sup_this: None,
		}))
	}

	pub(crate) fn pack_captures(&self, shape: &ClosureShape) -> PackedContext {
		PackedContext {
			captures: Cc::new(pack_captures(self, &shape.captures)),
			n_locals: shape.n_locals,
		}
	}
	pub(crate) fn pack_captures_sup_this(&self, shape: &ClosureShape) -> PackedContextSupThis {
		PackedContextSupThis {
			captures: Cc::new(pack_captures(self, &shape.captures)),
			n_locals: shape.n_locals,
			sup_this: self.0.sup_this.clone(),
		}
	}

	pub(crate) fn enter_iter(
		parent: &Context,
		shape: &ClosureShape,
		cb: impl FnOnce(IterContext) -> Result<()>,
	) -> Result<()> {
		let captures = Cc::new(pack_captures(parent, &shape.captures));
		let locals = IterFrame::new(shape.n_locals);
		cb(IterContext {
			context: Self(Cc::new(ContextInternal {
				captures,
				locals: Cc::new(LocalsFrame::Iter(locals)),
				sup_this: parent.0.sup_this.clone(),
			})),
		})
	}

	pub(crate) fn enter_using(parent: &Context, shape: &ClosureShape) -> Self {
		debug_assert_eq!(shape.n_locals, 0);
		if shape.captures.is_empty() {
			if let LocalsFrame::Iter(i) = &*parent.0.locals {
				i.captured.set(true);
			}
			// Value never uses captures, thus evaluating it against the parent gives the same result
			return parent.clone();
		}
		let captures = Cc::new(pack_captures(parent, &shape.captures));
		Self(Cc::new(ContextInternal {
			captures,
			locals: parent.0.locals.clone(),
			sup_this: parent.0.sup_this.clone(),
		}))
	}
}

fn pack_captures(parent: &Context, sources: &[LSlot]) -> Vec<Thunk<Val>> {
	sources.iter().map(|src| parent.slot(*src)).collect()
}

pub struct InitialContextBuilder {
	externals: Vec<(IStr, LocalId)>,
	values: Vec<Thunk<Val>>,
	next_id: u32,
}

impl InitialContextBuilder {
	pub(crate) fn new() -> Self {
		Self {
			externals: Vec::new(),
			values: Vec::new(),
			next_id: 0,
		}
	}

	pub fn bind(&mut self, name: impl Into<IStr>, value: Thunk<Val>) {
		let name = name.into();
		let id = LocalId(self.next_id);
		self.next_id += 1;
		self.externals.push((name, id));
		self.values.push(value);
	}

	pub(crate) fn build(self) -> (Vec<(IStr, LocalId)>, Vec<Thunk<Val>>) {
		(self.externals, self.values)
	}
}

impl Default for InitialContextBuilder {
	fn default() -> Self {
		Self::new()
	}
}
