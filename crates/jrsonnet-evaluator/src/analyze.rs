//! Static analysis of jsonnet IR.
//!
//! Walks the IR tree and produces a lowered IR (`LExpr`) with named locals resolved to numeric [`LocalId`] and
//! dependency analysis markers for every expression describing which objects and locals the expression depends on
//!
//! ```jsonnet
//! {
//!     a: $, // `a` is top-object-dependent.
//!     b: {
//!         // `b` is NOT object-dependent for the top object: it only references
//!         // things inside itself. `b` is built once per top-level object.
//!         a: $,
//!     },
//! }
//! ```

use std::rc::Rc;

use drop_bomb::DropBomb;
use jrsonnet_gcmodule::Acyclic;
use jrsonnet_interner::IStr;
use jrsonnet_ir::{
	ArgsDesc, AssertExpr, AssertStmt, BinaryOp, BinaryOpType, BindSpec, CompSpec, Destruct, Expr,
	ExprParams, FieldName, ForSpecData, IfElse, IfSpecData, ImportKind, LiteralType, NumValue,
	ObjBody, ObjComp, ObjMembers, Slice, SliceDesc, Span, Spanned, UnaryOpType, Visibility,
	function::FunctionSignature,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::error::{format_found, suggest_names};

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct AnalysisResult {
	/// Highest object, on which identity the value is dependent. `u32::MAX` = not dependent at all
	pub object_dependent_depth: u32,
	/// Highest local frame, on which this value depends. `u32::MAX` = not dependent at all
	pub local_dependent_depth: u32,
}

impl Default for AnalysisResult {
	fn default() -> Self {
		Self {
			object_dependent_depth: u32::MAX,
			local_dependent_depth: u32::MAX,
		}
	}
}

impl AnalysisResult {
	fn depend_on_object(&mut self, depth: u32) {
		if depth < self.object_dependent_depth {
			self.object_dependent_depth = depth;
		}
	}
	fn depend_on_local(&mut self, depth: u32) {
		if depth < self.local_dependent_depth {
			self.local_dependent_depth = depth;
		}
	}
	fn taint_by(&mut self, other: AnalysisResult) {
		self.depend_on_object(other.object_dependent_depth);
		self.depend_on_local(other.local_dependent_depth);
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Acyclic)]
pub struct LocalId(pub u32);

impl LocalId {
	fn idx(self) -> usize {
		self.0 as usize
	}
	fn defined_before(self, other: Self) -> bool {
		self.0 < other.0
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Acyclic)]
pub enum LSlot {
	/// Enclosing frame locals (sibling letrec, params, etc.).
	Local(LocalSlot),
	/// Enclosing closure's capture pack.
	Capture(CaptureSlot),
}

#[derive(Debug, Acyclic)]
pub struct ClosureShape {
	pub captures: Box<[LSlot]>,
	pub n_locals: u16,
}

struct LocalDefinition {
	name: IStr,
	span: Option<Span>,
	/// At which frame depth this local was defined
	defined_at_depth: u32,
	/// Min frame depth, at which this local was used. `u32::MAX` = not used at all.
	/// This check won't catch unused argument closures, i.e:
	/// ```jsonnet
	/// local
	///     a = b,
	///     b = a,
	/// ; 2 + 2
	///
	/// ```
	/// Both `a` and `b` here are "used", but the whole closure was not used here.
	used_at_depth: u32,
	/// Used as part of the current frame closure
	used_by_sibling: bool,
	/// Analysys result for value of this local
	analysis: AnalysisResult,
	/// Has `analysis` been filled in?
	/// For sanity checking, locals are initialized in batchs, use `first_uninitialized_local`
	analyzed: bool,
	/// During walk over uninitialized vars, we can't refer to analysis results of other locals,
	/// but we need to. To make that work, for each variable in variable frame we capture its closure,
	/// by looking at referenced variables.
	scratch_referenced: bool,
}

impl LocalDefinition {
	fn use_at(&mut self, depth: u32) {
		if depth == self.defined_at_depth {
			self.used_by_sibling = true;
			return;
		}
		if depth < self.used_at_depth {
			self.used_at_depth = depth;
		}
	}
}

#[derive(Debug, Acyclic)]
pub enum LExpr {
	Slot(LSlot),
	Null,
	Bool(bool),
	Str(IStr),
	Num(NumValue),
	Arr {
		shape: ClosureShape,
		items: Rc<Vec<LExpr>>,
	},
	ArrComp(Box<LArrComp>),
	Obj(LObjBody),
	ObjExtend(Box<LExpr>, LObjBody),
	UnaryOp(UnaryOpType, Box<LExpr>),
	BinaryOp {
		lhs: Box<LExpr>,
		op: BinaryOpType,
		rhs: Box<LExpr>,
	},
	AssertExpr {
		assert: Rc<LAssertStmt>,
		rest: Box<LExpr>,
	},
	Error(Span, Box<LExpr>),
	LocalExpr(Box<LLocalExpr>),
	Import {
		kind: Spanned<ImportKind>,
		kind_span: Span,
		path: IStr,
	},
	Apply {
		applicable: Box<LExpr>,
		args: Spanned<LArgsDesc>,
		tailstrict: bool,
	},
	Index {
		indexable: Box<LExpr>,
		parts: Vec<LIndexPart>,
	},
	Function(Rc<LFunction>),
	IdentityFunction,
	IfElse {
		cond: Box<LExpr>,
		cond_then: Box<LExpr>,
		cond_else: Option<Box<LExpr>>,
	},
	Slice(Box<LSliceExpr>),
	Super,

	/// Allows partial evaluation of broken expression tree,
	/// expressions with failed static analysis end up here
	BadLocal(&'static str),
}

#[derive(Debug, Acyclic)]
pub struct LLocalExpr {
	pub frame_shape: ClosureShape,
	pub binds: Vec<LBind>,
	pub body: LExpr,
}

#[derive(Debug, Acyclic)]
pub struct LFunction {
	pub name: Option<IStr>,
	pub params: Vec<LParam>,
	pub signature: FunctionSignature,

	pub body_shape: ClosureShape,
	pub body: Rc<LExpr>,
}

#[derive(Debug, Acyclic)]
pub struct LParam {
	pub name: Option<IStr>,
	pub destruct: LDestruct,

	pub default: Option<(ClosureShape, Rc<LExpr>)>,
}

#[derive(Debug, Acyclic)]
pub struct LBind {
	pub destruct: LDestruct,
	pub value_shape: ClosureShape,
	pub value: Rc<LExpr>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Acyclic)]
pub struct CaptureSlot(pub(crate) u16);
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Acyclic)]
pub struct LocalSlot(pub(crate) u16);

#[derive(Debug, Acyclic)]
pub enum LDestruct {
	Full(LocalSlot),
	#[cfg(feature = "exp-destruct")]
	Skip,
	#[cfg(feature = "exp-destruct")]
	Array {
		start: Vec<LDestruct>,
		rest: Option<LDestructRest>,
		end: Vec<LDestruct>,
	},
	#[cfg(feature = "exp-destruct")]
	Object {
		fields: Vec<LDestructField>,
		rest: Option<LDestructRest>,
	},
}

#[derive(Debug, Clone, Copy, Acyclic)]
pub enum LDestructRest {
	Keep(LocalSlot),
	Drop,
}

#[derive(Debug, Acyclic)]
pub struct LDestructField {
	pub name: IStr,
	pub into: Option<LDestruct>,
	pub default: Option<(ClosureShape, Rc<LExpr>)>,
}

impl LDestruct {
	pub fn each_slot<F: FnMut(LocalSlot)>(&self, f: &mut F) {
		match self {
			Self::Full(s) => f(*s),
			#[cfg(feature = "exp-destruct")]
			Self::Skip => {}
			#[cfg(feature = "exp-destruct")]
			Self::Array { start, rest, end } => {
				for d in start {
					d.each_slot(f);
				}
				if let Some(LDestructRest::Keep(s)) = rest {
					f(*s);
				}
				for d in end {
					d.each_slot(f);
				}
			}
			#[cfg(feature = "exp-destruct")]
			Self::Object { fields, rest } => {
				for field in fields {
					if let Some(into) = &field.into {
						into.each_slot(f);
					} else {
						unreachable!("shorthand object destruct must store `into`");
					}
				}
				if let Some(LDestructRest::Keep(s)) = rest {
					f(*s);
				}
			}
		}
	}

	pub fn slots(&self) -> SmallVec<[LocalSlot; 1]> {
		let mut out = SmallVec::new();
		self.each_slot(&mut |s| out.push(s));
		out
	}
}

#[derive(Debug, Acyclic)]
pub struct LSliceExpr {
	pub value: LExpr,
	pub start: Option<LExpr>,
	pub end: Option<LExpr>,
	pub step: Option<LExpr>,
}

#[derive(Debug, Acyclic)]
pub struct LArgsDesc {
	pub unnamed: Vec<Rc<LExpr>>,
	pub names: Vec<IStr>,
	pub values: Vec<Rc<LExpr>>,
}

#[derive(Debug, Acyclic)]
pub struct LAssertStmt {
	pub cond: Spanned<LExpr>,
	pub message: Option<LExpr>,
}

#[derive(Debug, Acyclic)]
pub struct LIndexPart {
	pub span: Span,
	pub value: LExpr,
	#[cfg(feature = "exp-null-coaelse")]
	pub null_coaelse: bool,
}

#[derive(Debug, Acyclic)]
pub enum LObjBody {
	MemberList(LObjMembers),
	ObjComp(Box<LObjComp>),
}

#[derive(Debug, Acyclic)]
pub struct LObjMembers {
	pub frame_shape: ClosureShape,
	/// If current object identity (`super`/`this`/`$`) is used, `this` should
	/// be saved to the specified local slot.
	pub this: Option<LocalSlot>,
	/// Set if dollar should also be assigned to object identity, `this` should also be set (TODO: proper type-level validation)
	pub set_dollar: bool,
	/// True iff `super` is referenced by this object's members.
	pub uses_super: bool,

	pub locals: Rc<Vec<LBind>>,
	pub asserts: Option<Rc<LObjAsserts>>,
	pub fields: Vec<LFieldMember>,
}

#[derive(Debug, Acyclic)]
pub struct LObjComp {
	pub frame_shape: Rc<ClosureShape>,
	pub this: Option<LocalSlot>,
	pub set_dollar: bool,
	pub uses_super: bool,

	pub locals: Rc<Vec<LBind>>,
	pub field: LFieldMember,
	pub compspecs: Vec<LCompSpec>,
}

#[derive(Debug, Acyclic)]
pub struct LFieldMember {
	pub name: LFieldName,
	pub plus: bool,
	pub visibility: Visibility,
	pub value: Rc<(ClosureShape, LExpr)>,
}

#[derive(Debug, Acyclic)]
pub struct LClosure<T: Acyclic> {
	pub shape: ClosureShape,
	pub value: T,
}

#[derive(Debug, Acyclic)]
pub struct LObjAsserts {
	pub shape: ClosureShape,
	pub asserts: Vec<LAssertStmt>,
}

#[derive(Debug, Acyclic)]
pub enum LFieldName {
	Fixed(IStr),
	Dyn(LExpr),
}
impl LFieldName {
	fn function_name(&self) -> Option<IStr> {
		match self {
			LFieldName::Fixed(istr) => Some(istr.clone()),
			LFieldName::Dyn(_) => None,
		}
	}
}

#[derive(Debug, Acyclic)]
pub struct LArrComp {
	pub value_shape: ClosureShape,
	pub value: Rc<LExpr>,
	pub compspecs: Vec<LCompSpec>,
}

#[derive(Debug, Acyclic)]
pub enum LCompSpec {
	If(LExpr),
	For {
		frame_shape: ClosureShape,
		destruct: LDestruct,
		over: LExpr,
		/// Is `over` does not depend on any variable introduced by an earlier for-spec in this comprehension chain
		loop_invariant: bool,
	},
	#[cfg(feature = "exp-object-iteration")]
	ForObj {
		frame_shape: ClosureShape,
		key: LocalSlot,
		visibility: jrsonnet_ir::Visibility,
		value: LDestruct,
		over: LExpr,
		loop_invariant: bool,
	},
}

struct FrameAlloc<'s> {
	first_in_frame: LocalId,
	stack: &'s mut AnalysisStack,
	bomb: DropBomb,
}
impl<'s> FrameAlloc<'s> {
	fn new(stack: &'s mut AnalysisStack) -> Self {
		FrameAlloc {
			first_in_frame: stack.next_local_id(),
			stack,
			bomb: DropBomb::new("binding frame state"),
		}
	}

	fn push_locals_closure(&mut self) -> ClosureOnStack {
		self.stack.push_closure_a(self.first_in_frame)
	}

	fn define_local(&mut self, name: IStr, span: Option<Span>) -> Option<(LocalId, LocalSlot)> {
		let id = self.stack.next_local_id();
		let stack = self.stack.local_by_name.entry(name.clone()).or_default();
		if let Some(&existing) = stack.last()
			&& !existing.defined_before(self.first_in_frame)
		{
			self.stack.report_error(
				format!("local is already defined in the current frame: {name}"),
				span,
			);
			return None;
		}
		stack.push(id);
		self.stack.local_defs.push(LocalDefinition {
			name,
			span,
			defined_at_depth: self.stack.depth,
			used_at_depth: u32::MAX,
			used_by_sibling: false,
			analysis: AnalysisResult::default(),
			analyzed: false,
			scratch_referenced: false,
		});
		let def = self.stack.defining_closure_mut();
		Some((id, def.define_local(id)))
	}
	fn alloc_bind(&mut self, bind: &BindSpec) -> Option<LDestruct> {
		match bind {
			BindSpec::Field { into, .. } => self.alloc_destruct(into),
			BindSpec::Function { name, .. } => {
				let (_, id) = self.define_local(name.clone(), None)?;
				Some(LDestruct::Full(id))
			}
		}
	}
	fn alloc_destruct(&mut self, destruct: &Destruct) -> Option<LDestruct> {
		Some(match destruct {
			Destruct::Full(name) => {
				let (_, id) = self.define_local(name.value.clone(), Some(name.span.clone()))?;
				LDestruct::Full(id)
			}
			#[cfg(feature = "exp-destruct")]
			Destruct::Skip => LDestruct::Skip,
			#[cfg(feature = "exp-destruct")]
			Destruct::Array { start, rest, end } => {
				let start = start
					.iter()
					.map(|d| self.alloc_destruct(d))
					.collect::<Option<Vec<_>>>()?;
				let rest = match rest {
					Some(jrsonnet_ir::DestructRest::Keep(name)) => {
						let (_, id) = self.define_local(name.clone(), None)?;
						Some(LDestructRest::Keep(id))
					}
					Some(jrsonnet_ir::DestructRest::Drop) => Some(LDestructRest::Drop),
					None => None,
				};
				let end = end
					.iter()
					.map(|d| self.alloc_destruct(d))
					.collect::<Option<Vec<_>>>()?;
				LDestruct::Array { start, rest, end }
			}
			#[cfg(feature = "exp-destruct")]
			Destruct::Object { fields, rest } => {
				let mut l_fields: Vec<(IStr, LDestruct)> = Vec::with_capacity(fields.len());
				// Allocate destruct LocalIds, then analyse defaults
				for (name, into, _default) in fields {
					let into = if let Some(inner) = into {
						self.alloc_destruct(inner)?
					} else {
						let (_, id) = self.define_local(name.clone(), None)?;
						LDestruct::Full(id)
					};
					l_fields.push((name.clone(), into));
				}
				// All locals exist, so defaults can reference any sibling.
				let l_fields: Vec<LDestructField> = l_fields
					.into_iter()
					.zip(fields.iter())
					.map(|((name, into), (_n, _i, default))| {
						let default = match default {
							Some(e) => {
								let mut default_taint = AnalysisResult::default();
								Some(self.stack.in_using_closure(|stack| {
									Rc::new(analyze(&e.value, stack, &mut default_taint))
								}))
							}
							None => None,
						};
						LDestructField {
							name,
							into: Some(into),
							default,
						}
					})
					.collect();
				let rest = match rest {
					Some(jrsonnet_ir::DestructRest::Keep(name)) => {
						let (_, id) = self.define_local(name.clone(), None)?;
						Some(LDestructRest::Keep(id))
					}
					Some(jrsonnet_ir::DestructRest::Drop) => Some(LDestructRest::Drop),
					None => None,
				};
				LDestruct::Object {
					fields: l_fields,
					rest,
				}
			}
		})
	}

	fn finish(self) -> PendingInit<'s> {
		let Self {
			first_in_frame,
			stack,
			bomb,
		} = self;
		let first_after_frame = stack.next_local_id();
		PendingInit {
			first_after_frame,
			stack,
			closures: Closures {
				referenced: vec![],
				spec_shapes: vec![],
				first_in_frame,
			},
			bomb,
		}
	}
}

/// Frame state: `LocalIds` allocated, values not yet analysed.
struct PendingInit<'s> {
	first_after_frame: LocalId,
	stack: &'s mut AnalysisStack,
	closures: Closures,
	bomb: DropBomb,
}

impl<'s> PendingInit<'s> {
	/// Record the analysis of a spec's value: stamp every id bound by the
	/// spec with `analysis`, collect the spec's same-frame references, and
	/// append them to `closures`.
	fn record_spec_init(&mut self, destruct: &LDestruct, analysis: AnalysisResult) {
		let mut refs: SmallVec<[LocalId; 4]> = SmallVec::new();
		for i in self.closures.first_in_frame.0..self.first_after_frame.0 {
			let def = &mut self.stack.local_defs[i as usize];
			if def.scratch_referenced {
				refs.push(LocalId(i));
				def.scratch_referenced = false;
			}
		}

		let mut ids_count = 0;
		let first_local = self.stack.top_defining_local();
		destruct.each_slot(&mut |slot| {
			ids_count += 1;
			let id = LocalId(first_local.0 + u32::from(slot.0));
			let def = &mut self.stack.local_defs[id.idx()];
			debug_assert!(!def.analyzed, "sanity: local {:?} analysed twice", def.name);
			def.analysis = analysis;
			def.analyzed = true;
		});
		self.closures.push_spec(ids_count, &refs);
	}
	/// After all specs are analysed, propagate dependency information between
	/// siblings to a fix-point, then switch to "body" mode.
	fn finish(self) -> PendingBody<'s> {
		let Self {
			first_after_frame,
			closures,
			stack,
			bomb,
		} = self;

		debug_assert_eq!(
			first_after_frame,
			stack.next_local_id(),
			"frame initialisation left unfinished locals"
		);

		debug_assert_eq!(
			closures.spec_shapes.iter().map(|(_, d)| *d).sum::<usize>(),
			(first_after_frame.0 - closures.first_in_frame.0) as usize,
			"closures destruct-id counts must match frame local count"
		);

		let mut changed = true;
		while changed {
			changed = false;
			for spec in closures.iter_specs() {
				for id_raw in spec.ids.clone() {
					let user = LocalId(id_raw);
					for &used in spec.references {
						changed |= stack.propagate_analysis(user, used);
					}
				}
			}
		}

		stack.depth += 1;
		PendingBody {
			first_after_frame,
			closures,
			stack,
			bomb,
		}
	}
}

/// Frame state: values analysed, body not yet walked.
struct PendingBody<'s> {
	first_after_frame: LocalId,
	closures: Closures,
	stack: &'s mut AnalysisStack,
	bomb: DropBomb,
}
impl<'s> PendingBody<'s> {
	/// After the body is processed, drop the frame's locals and emit any
	/// "unused local" warnings.
	fn finish(self) {
		let PendingBody {
			first_after_frame,
			closures,
			stack,
			mut bomb,
		} = self;
		bomb.defuse();
		stack.depth -= 1;

		debug_assert_eq!(
			first_after_frame,
			stack.next_local_id(),
			"nested scopes must be popped before outer frames"
		);

		let mut changed = true;
		while changed {
			changed = false;
			for spec in closures.iter_specs() {
				// Effective used_at_depth for the spec = min over its ids.
				let mut min_used_at = u32::MAX;
				for id_raw in spec.ids.clone() {
					min_used_at = min_used_at.min(stack.local_defs[id_raw as usize].used_at_depth);
				}
				if min_used_at == u32::MAX {
					continue;
				}
				for &used in spec.references {
					let used_def = &mut stack.local_defs[used.idx()];
					if min_used_at < used_def.used_at_depth {
						used_def.used_at_depth = min_used_at;
						changed = true;
					}
				}
			}
		}

		let drained: Vec<LocalDefinition> = stack
			.local_defs
			.drain(closures.first_in_frame.idx()..)
			.collect();
		for (i, def) in drained.iter().enumerate().rev() {
			let id = LocalId(closures.first_in_frame.0 + i as u32);
			let stack_locals = stack
				.local_by_name
				.get_mut(&def.name)
				.expect("local must be in name map");
			let popped = stack_locals.pop().expect("name stack should not be empty");
			debug_assert_eq!(popped, id, "name stack integrity");
			if stack_locals.is_empty() {
				stack.local_by_name.remove(&def.name);
			}

			if def.used_at_depth == u32::MAX {
				if def.used_by_sibling {
					stack.report_warning(
						format!("local is only referenced by unused siblings: {}", def.name),
						def.span.clone(),
					);
				} else {
					stack.report_warning(format!("unused local: {}", def.name), def.span.clone());
				}
			} else if def.analysis.local_dependent_depth > def.defined_at_depth
				&& def.analysis.object_dependent_depth > def.defined_at_depth
				&& def.defined_at_depth != 0
			{
				// The value doesn't depend on anything defined at or inside
				// this local's scope - can be hoisted, unfortunately not automatically.
				stack.report_warning(
					format!("local could be hoisted to an outer scope: {}", def.name),
					def.span.clone(),
				);
			}
		}
	}
}

struct Closures {
	/// All the referenced locals, maybe repeated multiple times
	/// It is recorded as continous vec of sets, I.e we have
	/// a = 1, 2, 3
	/// b = 3, 4, 5, 6
	/// And in `referenced` we have `[ 1, 2, 3, 3, 4, 5, 6 ]`. To actually get, which closure refers to which element, see `spec_shapes`...
	/// Flat concatenation of sibling-local references across all specs.
	referenced: Vec<LocalId>,
	/// Amount of elements per closure, for the above case it is a = 3, b = 4, so here
	/// lies `[ 3, 4 ]`
	/// ~~closures: Vec<usize>,~~
	/// Finally, we have destructs.
	/// Because single destruct references single closure, but destructs to multiple locals, we have even more complicated structure.
	/// Luckly, every destruct is not interleaved with each other, so here we can have full list...
	/// Imagine having (LocalId(20), LocalId(21)), we need to save it to the Map, but we know that the numbers are sequential, so here we store number of consequent elements
	/// for each destruct starting from `first_destruct_local`
	/// ~~destructs: Vec<usize>,~~
	///
	/// => two of those fields were merged, as there is currently no per-destruct tracking of closures.
	/// For each spec in order: `(references_count, destruct_ids_count)`.
	/// `references_count` tells us how many entries of `referenced` belong
	/// to this spec; `destruct_ids_count` tells us how many `LocalIds` it
	/// binds.
	spec_shapes: Vec<(usize, usize)>,
	/// This is not a related doccomment, just a continuation of docs for previous fields.
	/// Having
	/// ```jsonnet
	/// local
	///     [a, b, c] = [d, e, f],
	///     [d, e, f] = [a, b, c, h],
	///     h = 1,
	/// ;
	/// ```
	///
	/// We have total of 7 locals
	/// First local here is `a` => `first_destruct_local` = `a`
	/// For first closure `[a, b, c] = [d, e, f]` we have 3 referenced locals = [d, e, f] => `referenced += [d, e, f]`, `closures += 3`; 3 destructs = [a, b, c] => `destructs += 3`
	/// [d, e, f] = [a, b, c, h], => `referenced += [a, b, c, h]`, `closures += 4`, `destructs += 3` (Note that this destruct will fail at runtime,
	///                                                                                               this thing should not care about that, it only captures what the value are referencing)
	/// h = 1 => referenced += [], closures += 0, destructs += 1
	/// And the result is
	///
	/// ```rust,ignore
	/// Closures {
	///     referenced: vec![d, e, f, a, b, c, h]
	///     spec_shapes: vec![(3, 3), (4, 3), (0, 1)],
	///     first_destruct_label: a,
	/// }
	/// ```
	///
	/// Reconstruction of that:
	///
	/// We know that we start with a
	/// We get the first number from destructs: `destructs.shift() == 3` => `destructs = [3, 1]`
	/// 3 elements counting from a => [a, b, c]
	/// Then we take first number from closures: `closures.shift() == 3` => `closures = [4, 0]`
	/// Then we take 3 items from referenced: `referenced.shift()x3 == d, e, f` => `referenced = [a, b, c, h]`
	///
	/// Thus we have [a, b, c] = [d, e, f]
	first_in_frame: LocalId,
}

struct Closure<'a> {
	references: &'a [LocalId],
	ids: std::ops::Range<u32>,
}

impl Closures {
	fn push_spec(&mut self, destruct_ids_count: usize, refs: &[LocalId]) {
		self.referenced.extend_from_slice(refs);
		self.spec_shapes.push((refs.len(), destruct_ids_count));
	}

	fn iter_specs(&self) -> impl Iterator<Item = Closure<'_>> {
		let mut refs = self.referenced.as_slice();
		let mut next_id = self.first_in_frame.0;
		self.spec_shapes.iter().map(move |(refs_len, dest_count)| {
			let (this_refs, rest) = refs.split_at(*refs_len);
			refs = rest;
			let start = next_id;
			next_id += *dest_count as u32;
			Closure {
				references: this_refs,
				ids: start..next_id,
			}
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
	Error,
	Warning,
}

#[derive(Debug, Clone, Acyclic)]
pub struct Diagnostic {
	pub level: DiagLevel,
	pub message: String,
	pub span: Option<Span>,
}

struct DefiningClosure {
	first_local: LocalId,
	n_locals: u16,
}

impl DefiningClosure {
	fn resolve(&self, target: LocalId) -> Option<LocalSlot> {
		let end = self.first_local.0 + u32::from(self.n_locals);
		if target.0 >= self.first_local.0 && target.0 < end {
			Some(LocalSlot(
				u16::try_from(target.0 - self.first_local.0).expect("local slots overflow"),
			))
		} else {
			None
		}
	}
	fn define_local(&mut self, local: LocalId) -> LocalSlot {
		let slot = self.n_locals;
		let id = self.first_local.0 + u32::from(slot);
		debug_assert_eq!(local.0, id);
		self.n_locals = self.n_locals.checked_add(1).expect("local slots overflow");
		LocalSlot(slot)
	}
}

/// Per-closure capture computation state.
struct ClosureFrame {
	/// Closure may allocate locals
	defining: Option<DefiningClosure>,
	/// `LocalId` => capture index
	captures: FxHashMap<LocalId, CaptureSlot>,
	/// Capture sources in insertion order; consumed by `pop_closure_frame`.
	capture_sources: Vec<LSlot>,
}

#[allow(clippy::struct_excessive_bools)]
pub struct AnalysisStack {
	local_defs: Vec<LocalDefinition>,
	/// Shadowing isn't used in jsonnet much, 2 because `SmallVec` allows to store 2 ptr-sized without overhead.
	/// TODO: Add test for this assumption (sizeof(SmallVec<[usize; 1]>) == sizeof(SmallVec<[usize; 2]>))
	local_by_name: FxHashMap<IStr, SmallVec<[LocalId; 2]>>,

	/// Depth of the current locals frame.
	depth: u32,
	/// Last depth, at which object has appeared. `u32::MAX` = not appeared at all
	last_object_depth: u32,
	/// First depth, at which object has appeared. `u32::MAX` = not appeared at all
	/// $ refers to this object.
	first_object_depth: u32,

	/// `LocalId` bound to the innermost object's `this`
	this_local: Option<LocalId>,
	/// Outermost object `this`, aka `$`
	dollar_alias: Option<LocalId>,
	/// True iff `self` has been referenced in the current object immediate members (not nested children).
	cur_self_used: bool,
	/// True iff `super` has been referenced in the current object immediate members.
	cur_super_used: bool,
	/// True iff `$` has been referenced anywhere since the outermost object's scope was entered.
	dollar_used: bool,

	/// Stack of closure frames (innermost on top).
	closure_stack: Vec<ClosureFrame>,

	diagnostics: Vec<Diagnostic>,
	/// Whenever analysis would be broken due to static analysis error.
	errored: bool,
}

#[must_use]
struct ClosureOnStack {
	bomb: DropBomb,
}

impl AnalysisStack {
	pub fn new() -> Self {
		Self {
			local_defs: Vec::new(),
			local_by_name: FxHashMap::default(),
			depth: 0,
			last_object_depth: u32::MAX,
			first_object_depth: u32::MAX,
			this_local: None,
			dollar_alias: None,
			cur_self_used: false,
			cur_super_used: false,
			dollar_used: false,
			closure_stack: Vec::new(),
			diagnostics: Vec::new(),
			errored: false,
		}
	}

	fn push_root_closure(&mut self, externals: u16) -> ClosureOnStack {
		assert!(
			self.closure_stack.is_empty(),
			"root is only possible with empty stack"
		);

		self.closure_stack.push(ClosureFrame {
			defining: Some(DefiningClosure {
				first_local: LocalId(0),
				n_locals: externals,
			}),
			captures: FxHashMap::default(),
			capture_sources: Vec::new(),
		});

		ClosureOnStack {
			bomb: DropBomb::new("root closure"),
		}
	}

	fn push_closure_a(&mut self, first_local: LocalId) -> ClosureOnStack {
		self.closure_stack.push(ClosureFrame {
			defining: Some(DefiningClosure {
				first_local,
				n_locals: 0,
			}),
			captures: FxHashMap::default(),
			capture_sources: Vec::new(),
		});
		ClosureOnStack {
			bomb: DropBomb::new("closure with locals"),
		}
	}

	#[inline]
	fn in_using_closure<T>(
		&mut self,
		inner: impl FnOnce(&mut AnalysisStack) -> T,
	) -> (ClosureShape, T) {
		fn push_closure_b(stack: &mut AnalysisStack) -> ClosureOnStack {
			stack.closure_stack.push(ClosureFrame {
				defining: None,
				captures: FxHashMap::default(),
				capture_sources: Vec::new(),
			});
			ClosureOnStack {
				bomb: DropBomb::new("closure with locals"),
			}
		}
		let closure = push_closure_b(self);
		let v = inner(self);
		let shape = self.pop_closure(closure);
		(shape, v)
	}

	fn pop_closure(&mut self, mut closure: ClosureOnStack) -> ClosureShape {
		closure.bomb.defuse();
		let frame = self.closure_stack.pop().expect("closure frame");
		ClosureShape {
			captures: frame.capture_sources.into_boxed_slice(),
			n_locals: frame.defining.map(|d| d.n_locals).unwrap_or_default(),
		}
	}

	/// Resolve a `LocalId` reference to an `LSlot` against the innermost
	/// closure frame. May insert capture entries up the closure stack as
	/// needed.
	fn resolve_to_slot(&mut self, target: LocalId) -> LSlot {
		let top = self.closure_stack.len();
		debug_assert!(top > 0, "resolve_to_slot called with no closure frame");
		Self::resolve_at(&mut self.closure_stack, top - 1, target)
	}

	fn resolve_at(stack: &mut [ClosureFrame], idx: usize, target: LocalId) -> LSlot {
		if let Some(def) = &stack[idx].defining {
			if let Some(resolved) = def.resolve(target) {
				return LSlot::Local(resolved);
			}
		} else {
			// A sibling letrec slot must never be packed as a capture, or
			// it would read an empty `OnceCell`.
			for j in (0..idx).rev() {
				if let Some(def) = &stack[j].defining {
					if let Some(resolved) = def.resolve(target) {
						return LSlot::Local(resolved);
					}
					break;
				}
			}
		}
		if let Some(&cap_idx) = stack[idx].captures.get(&target) {
			return LSlot::Capture(cap_idx);
		}
		debug_assert!(idx > 0, "no enclosing closure frame for target {target:?}");
		let parent_slot = Self::resolve_at(stack, idx - 1, target);
		let frame = &mut stack[idx];
		let cap_idx = CaptureSlot(
			frame
				.capture_sources
				.len()
				.try_into()
				.expect("frame has more than u16::MAX captures"),
		);
		frame.capture_sources.push(parent_slot);
		frame.captures.insert(target, cap_idx);
		LSlot::Capture(cap_idx)
	}

	fn next_local_id(&self) -> LocalId {
		LocalId(self.local_defs.len() as u32)
	}

	fn report_error(&mut self, msg: impl Into<String>, span: Option<Span>) {
		self.errored = true;
		self.diagnostics.push(Diagnostic {
			level: DiagLevel::Error,
			message: msg.into(),
			span,
		});
	}
	fn report_warning(&mut self, msg: impl Into<String>, span: Option<Span>) {
		self.diagnostics.push(Diagnostic {
			level: DiagLevel::Warning,
			message: msg.into(),
			span,
		});
	}

	fn use_local(&mut self, name: &IStr, span: Span, taint: &mut AnalysisResult) -> Option<LSlot> {
		let Some(ids) = self.local_by_name.get(name) else {
			let names = suggest_names(name, self.local_by_name.keys());
			self.report_error(
				format!("undefined local: {name}{}", format_found(&names, "local")),
				Some(span),
			);
			return None;
		};
		let id = *ids.last().expect("empty stacks should be removed");
		let depth = self.depth;
		let def = &mut self.local_defs[id.idx()];
		def.use_at(depth);
		taint.depend_on_local(def.defined_at_depth);
		if def.analyzed {
			taint.taint_by(def.analysis);
		} else {
			def.scratch_referenced = true;
		}
		Some(self.resolve_to_slot(id))
	}

	/// Assign name to the value provided externally, e.g `std`.
	pub fn define_external_local(&mut self, name: IStr, id: LocalId) {
		assert!(
			self.local_defs.iter().all(|d| d.analyzed),
			"external locals must be defined before the root expression is analysed"
		);
		assert_eq!(
			id,
			self.next_local_id(),
			"external local id mismatch for {name} (externals must be defined in allocation order)"
		);
		self.local_defs.push(LocalDefinition {
			name: name.clone(),
			span: None,
			defined_at_depth: 0,
			used_at_depth: u32::MAX,
			used_by_sibling: false,
			analysis: AnalysisResult::default(),
			analyzed: true,
			scratch_referenced: false,
		});
		self.local_by_name.entry(name).or_default().push(id);
	}

	fn defining_closure_mut(&mut self) -> &mut DefiningClosure {
		self.closure_stack
			.iter_mut()
			.rev()
			.find_map(|c| c.defining.as_mut())
			.expect("no enclosing defining closure frame")
	}
	fn defining_closure(&self) -> &DefiningClosure {
		self.closure_stack
			.iter()
			.rev()
			.find_map(|c| c.defining.as_ref())
			.expect("no enclosing defining closure frame")
	}
}

impl Default for AnalysisStack {
	fn default() -> Self {
		Self::new()
	}
}

impl AnalysisStack {
	fn top_defining_local(&self) -> LocalId {
		self.defining_closure().first_local
	}

	/// Merge `used`'s analysis into `user`'s analysis and record that `user`
	/// transitively depends on `used` (same-frame sibling reference).
	/// Returns `true` if `user`'s analysis changed.
	fn propagate_analysis(&mut self, user: LocalId, used: LocalId) -> bool {
		let (used_analysis, used_defined_at_depth) = {
			let u = &self.local_defs[used.idx()];
			(u.analysis, u.defined_at_depth)
		};
		let user_def = &mut self.local_defs[user.idx()];
		let before_obj = user_def.analysis.object_dependent_depth;
		let before_loc = user_def.analysis.local_dependent_depth;
		user_def.analysis.taint_by(used_analysis);
		user_def.analysis.depend_on_local(used_defined_at_depth);
		before_obj != user_def.analysis.object_dependent_depth
			|| before_loc != user_def.analysis.local_dependent_depth
	}
}

mod names {
	use crate::names;

	names! {
		this: "this",
	}
}

// Object scope helpers
impl AnalysisStack {
	#[inline]
	fn in_object_scope<T>(
		&mut self,
		inner: impl FnOnce(&mut AnalysisStack) -> T,
	) -> (ObjectUsage, ClosureShape, T) {
		fn enter_object_scope(stack: &mut AnalysisStack) -> ObjectScope {
			let is_outermost = stack.first_object_depth == u32::MAX;
			let this_id = stack.next_local_id();
			let closure = stack.push_closure_a(this_id);
			let pushed = stack.push_pseudo_local(names::this());
			debug_assert_eq!(pushed, this_id, "this pseudo-local id");
			let scope = ObjectScope {
				this_id,
				is_outermost,
				prev_this_local: stack.this_local,
				prev_dollar_alias: stack.dollar_alias,
				prev_cur_self_used: stack.cur_self_used,
				prev_cur_super_used: stack.cur_super_used,
				prev_dollar_used: is_outermost.then_some(stack.dollar_used),
				prev_last_object: stack.last_object_depth,
				prev_first_object: stack.first_object_depth,
				closure,
			};

			stack.this_local = Some(scope.this_id);
			if is_outermost {
				stack.dollar_alias = Some(scope.this_id);
				stack.first_object_depth = stack.depth;
				stack.dollar_used = false;
			}
			stack.last_object_depth = stack.depth;
			stack.cur_self_used = false;
			stack.cur_super_used = false;
			scope
		}

		fn leave_object_scope(
			stack: &mut AnalysisStack,
			scope: ObjectScope,
		) -> (ObjectUsage, ClosureShape) {
			let ObjectScope {
				this_id,
				is_outermost,
				prev_this_local,
				prev_dollar_alias,
				prev_cur_self_used,
				prev_cur_super_used,
				prev_dollar_used,
				prev_last_object,
				prev_first_object,
				closure,
			} = scope;
			let _ = stack.local_defs.pop().expect("this pseudo-local exists");
			debug_assert_eq!(stack.local_defs.len(), this_id.0 as usize);

			let set_dollar = is_outermost && stack.dollar_used;
			let usage = ObjectUsage {
				this_used: stack.cur_self_used || stack.cur_super_used || set_dollar,
				uses_super: stack.cur_super_used,
				set_dollar,
			};

			stack.this_local = prev_this_local;
			stack.dollar_alias = prev_dollar_alias;
			stack.cur_self_used = prev_cur_self_used;
			stack.cur_super_used = prev_cur_super_used;
			if let Some(prev) = prev_dollar_used {
				stack.dollar_used = prev;
			}
			stack.last_object_depth = prev_last_object;
			stack.first_object_depth = prev_first_object;

			let frame_shape = stack.pop_closure(closure);
			(usage, frame_shape)
		}
		let scope = enter_object_scope(self);
		let v = inner(self);
		let (usage, shape) = leave_object_scope(self, scope);
		(usage, shape, v)
	}

	fn push_pseudo_local(&mut self, name: IStr) -> LocalId {
		let id = self.next_local_id();
		self.local_defs.push(LocalDefinition {
			name,
			span: None,
			defined_at_depth: self.depth,
			used_at_depth: u32::MAX,
			used_by_sibling: false,
			analysis: AnalysisResult::default(),
			analyzed: true,
			scratch_referenced: false,
		});
		{
			let def = self.defining_closure_mut();
			let _ = def.define_local(id);
		}
		id
	}

	fn use_this(&mut self, taint: &mut AnalysisResult) -> Option<LSlot> {
		let id = self.this_local?;
		self.cur_self_used = true;
		self.use_pseudo_local(id, taint);
		Some(self.resolve_to_slot(id))
	}

	fn use_super(&mut self, taint: &mut AnalysisResult) -> Option<()> {
		let id = self.this_local?;
		self.cur_super_used = true;
		self.use_pseudo_local(id, taint);
		Some(())
	}

	fn use_dollar(&mut self, taint: &mut AnalysisResult) -> Option<LSlot> {
		let id = self.dollar_alias?;
		self.dollar_used = true;
		self.use_pseudo_local(id, taint);
		Some(self.resolve_to_slot(id))
	}

	// TODO: Dedicated type for object references instead of "pseudo local" BS, idk
	fn use_pseudo_local(&mut self, id: LocalId, taint: &mut AnalysisResult) {
		let depth = self.depth;
		let def = &mut self.local_defs[id.idx()];
		def.use_at(depth);
		taint.depend_on_local(def.defined_at_depth);
		taint.depend_on_object(def.defined_at_depth);
	}
}

#[must_use]
struct ObjectScope {
	this_id: LocalId,
	is_outermost: bool,
	prev_this_local: Option<LocalId>,
	prev_dollar_alias: Option<LocalId>,
	prev_cur_self_used: bool,
	prev_cur_super_used: bool,
	prev_dollar_used: Option<bool>,
	prev_last_object: u32,
	prev_first_object: u32,
	closure: ClosureOnStack,
}

struct ObjectUsage {
	this_used: bool,
	uses_super: bool,
	set_dollar: bool,
}

fn analyze_assert(
	stmt: &AssertStmt,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LAssertStmt {
	let cond = analyze(&stmt.assertion.value, stack, taint);
	let message = stmt.message.as_ref().map(|m| analyze(m, stack, taint));
	LAssertStmt {
		cond: Spanned::new(cond, stmt.assertion.span.clone()),
		message,
	}
}

#[allow(clippy::too_many_lines)]
pub fn analyze_named(
	name: IStr,
	expr: &Expr,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LExpr {
	if let Expr::Function(params, body) = expr {
		return analyze_function(Some(name), params, body, stack, taint);
	}
	analyze(expr, stack, taint)
}
#[allow(clippy::too_many_lines)]
pub fn analyze(expr: &Expr, stack: &mut AnalysisStack, taint: &mut AnalysisResult) -> LExpr {
	match expr {
		Expr::Literal(l) => match l {
			LiteralType::This => stack.use_this(taint).map_or_else(
				|| {
					stack.report_error("`self` used outside of object", None);
					LExpr::BadLocal("self")
				},
				LExpr::Slot,
			),
			LiteralType::Super => {
				if stack.use_super(taint).is_some() {
					LExpr::Super
				} else {
					stack.report_error("`super` used outside of object", None);
					LExpr::BadLocal("super")
				}
			}
			LiteralType::Dollar => stack.use_dollar(taint).map_or_else(
				|| {
					stack.report_error("`$` used outside of object", None);
					LExpr::BadLocal("$")
				},
				LExpr::Slot,
			),
			LiteralType::Null => LExpr::Null,
			LiteralType::True => LExpr::Bool(true),
			LiteralType::False => LExpr::Bool(false),
		},
		Expr::Str(s) => LExpr::Str(s.clone()),
		Expr::Num(n) => LExpr::Num(*n),
		Expr::Var(v) => stack
			.use_local(&v.value, v.span.clone(), taint)
			.map_or_else(|| LExpr::BadLocal("ref"), LExpr::Slot),
		Expr::Arr(a) => {
			let (shape, items) = stack
				.in_using_closure(|stack| a.iter().map(|v| analyze(v, stack, taint)).collect());
			LExpr::Arr {
				shape,
				items: Rc::new(items),
			}
		}
		Expr::ArrComp(inner, comp) => analyze_arr_comp(inner, comp, stack, taint),
		Expr::Obj(obj) => LExpr::Obj(analyze_obj_body(obj, stack, taint)),
		Expr::ObjExtend(base, obj) => LExpr::ObjExtend(
			Box::new(analyze(base, stack, taint)),
			analyze_obj_body(obj, stack, taint),
		),
		Expr::UnaryOp(op, value) => LExpr::UnaryOp(*op, Box::new(analyze(value, stack, taint))),
		Expr::BinaryOp(op) => {
			let BinaryOp {
				lhs,
				op: optype,
				rhs,
			} = &**op;
			LExpr::BinaryOp {
				lhs: Box::new(analyze(lhs, stack, taint)),
				op: *optype,
				rhs: Box::new(analyze(rhs, stack, taint)),
			}
		}
		Expr::AssertExpr(assert) => {
			let AssertExpr { assert, rest } = &**assert;
			let assert = Rc::new(analyze_assert(assert, stack, taint));
			let rest = Box::new(analyze(rest, stack, taint));
			LExpr::AssertExpr { assert, rest }
		}
		Expr::LocalExpr(binds, body) => analyze_local_expr(binds, body, stack, taint),
		Expr::Import(kind, path_expr) => {
			let Expr::Str(path) = &**path_expr else {
				stack.report_error(
					"import path must be a string literal",
					Some(kind.span.clone()),
				);
				return LExpr::BadLocal("bad import");
			};
			LExpr::Import {
				kind: kind.clone(),
				kind_span: kind.span.clone(),
				path: path.clone(),
			}
		}
		Expr::ErrorStmt(span, inner) => {
			LExpr::Error(span.clone(), Box::new(analyze(inner, stack, taint)))
		}
		Expr::Apply(applicable, args, tailstrict) => {
			let app = analyze(applicable, stack, taint);
			let ArgsDesc {
				unnamed,
				names,
				values,
			} = &args.value;
			let unnamed_l = unnamed
				.iter()
				.map(|a| Rc::new(analyze(a, stack, taint)))
				.collect();
			let values_l = values
				.iter()
				.map(|a| Rc::new(analyze(a, stack, taint)))
				.collect();
			LExpr::Apply {
				applicable: Box::new(app),
				args: Spanned::new(
					LArgsDesc {
						unnamed: unnamed_l,
						names: names.clone(),
						values: values_l,
					},
					args.span.clone(),
				),
				tailstrict: *tailstrict,
			}
		}
		Expr::Index { indexable, parts } => {
			let idx = analyze(indexable, stack, taint);
			let parts_l = parts
				.iter()
				.map(|p| {
					let value = analyze(&p.value, stack, taint);
					LIndexPart {
						span: p.span.clone(),
						value,
						#[cfg(feature = "exp-null-coaelse")]
						null_coaelse: p.null_coaelse,
					}
				})
				.collect();
			LExpr::Index {
				indexable: Box::new(idx),
				parts: parts_l,
			}
		}
		Expr::Function(params, body) => analyze_function(None, params, body, stack, taint),
		Expr::IfElse(ifelse) => {
			let IfElse {
				cond,
				cond_then,
				cond_else,
			} = &**ifelse;
			let cond_l = analyze(&cond.cond, stack, taint);
			let then_l = analyze(cond_then, stack, taint);
			let else_l = cond_else
				.as_ref()
				.map(|e| Box::new(analyze(e, stack, taint)));
			LExpr::IfElse {
				cond: Box::new(cond_l),
				cond_then: Box::new(then_l),
				cond_else: else_l,
			}
		}
		Expr::Slice(slice) => {
			let Slice {
				value,
				slice: SliceDesc { start, end, step },
			} = &**slice;
			let value_l = analyze(value, stack, taint);
			let start_l = start.as_ref().map(|e| analyze(&e.value, stack, taint));
			let end_l = end.as_ref().map(|e| analyze(&e.value, stack, taint));
			let step_l = step.as_ref().map(|e| analyze(&e.value, stack, taint));
			LExpr::Slice(Box::new(LSliceExpr {
				value: value_l,
				start: start_l,
				end: end_l,
				step: step_l,
			}))
		}
	}
}

fn analyze_local_expr(
	binds: &[BindSpec],
	body: &Expr,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LExpr {
	if binds.is_empty() {
		return analyze(body, stack, taint);
	}
	let frame_start = stack.next_local_id();
	let closure = stack.push_closure_a(frame_start);
	let (l_binds, body_expr) = process_local_frame(binds, stack, taint, |stack, taint| {
		analyze(body, stack, taint)
	});
	let frame_shape = stack.pop_closure(closure);
	LExpr::LocalExpr(Box::new(LLocalExpr {
		frame_shape,
		binds: l_binds,
		body: body_expr,
	}))
}

fn analyze_bind_value(
	bind: &BindSpec,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LExpr {
	match bind {
		BindSpec::Field {
			value: Expr::Function(params, value),
			into: Destruct::Full(name),
		} => analyze_function(Some(name.value.clone()), params, value, stack, taint),
		BindSpec::Field { value, .. } => analyze(value, stack, taint),
		BindSpec::Function {
			params,
			value,
			name,
		} => analyze_function(Some(name.clone()), params, value, stack, taint),
	}
}

fn process_local_frame<R>(
	binds: &[BindSpec],
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
	body_fn: impl FnOnce(&mut AnalysisStack, &mut AnalysisResult) -> R,
) -> (Vec<LBind>, R) {
	let mut alloc = FrameAlloc::new(stack);

	let mut destructs: Vec<Option<LDestruct>> = Vec::with_capacity(binds.len());
	for bind in binds {
		destructs.push(alloc.alloc_bind(bind));
	}
	let mut pending = alloc.finish();

	let mut l_binds: Vec<LBind> = Vec::with_capacity(binds.len());
	for (bind, destruct) in binds.iter().zip(destructs.into_iter()) {
		let mut value_taint = AnalysisResult::default();
		let (value_shape, value) = pending
			.stack
			.in_using_closure(|stack| analyze_bind_value(bind, stack, &mut value_taint));
		taint.taint_by(value_taint);
		if let Some(destruct) = destruct {
			pending.record_spec_init(&destruct, value_taint);
			l_binds.push(LBind {
				destruct,
				value_shape,
				value: Rc::new(value),
			});
		} else {
			pending.closures.push_spec(0, &[]);
		}
	}

	let body_frame = pending.finish();
	let result = body_fn(body_frame.stack, taint);
	body_frame.finish();

	(l_binds, result)
}

fn analyze_function(
	name: Option<IStr>,
	params: &ExprParams,
	body: &Expr,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LExpr {
	let mut alloc = FrameAlloc::new(stack);
	let closure = alloc.push_locals_closure();

	let mut param_destructs: Vec<Option<LDestruct>> = Vec::with_capacity(params.exprs.len());
	for p in &params.exprs {
		param_destructs.push(alloc.alloc_destruct(&p.destruct));
	}

	let mut pending = alloc.finish();

	let mut l_params: Vec<LParam> = Vec::with_capacity(params.exprs.len());
	for (p, destruct) in params.exprs.iter().zip(param_destructs.into_iter()) {
		let mut value_taint = AnalysisResult::default();
		let default = p.default.as_ref().map_or_else(
			|| None,
			|d| {
				Some(
					pending
						.stack
						.in_using_closure(|stack| Rc::new(analyze(d, stack, &mut value_taint))),
				)
			},
		);
		taint.taint_by(value_taint);
		if let Some(destruct) = destruct {
			let name = match &p.destruct {
				Destruct::Full(n) => Some(n.value.clone()),
				#[cfg(feature = "exp-destruct")]
				_ => None,
			};
			pending.record_spec_init(&destruct, value_taint);
			l_params.push(LParam {
				name,
				destruct,
				default,
			});
		} else {
			pending.closures.push_spec(0, &[]);
		}
	}

	let body_frame = pending.finish();
	let body_expr = analyze(body, body_frame.stack, taint);
	body_frame.finish();
	let body_shape = stack.pop_closure(closure);

	// function(x) x is an identity function
	if l_params.len() == 1 && l_params[0].default.is_none() {
		stack.report_warning(
			"do not define identity functions manually, use std.id instead",
			None,
		);
		#[allow(irrefutable_let_patterns, reason = "refutable with exp-destruct")]
		if let LDestruct::Full(param_slot) = &l_params[0].destruct
			&& let LExpr::Slot(LSlot::Local(s)) = &body_expr
			&& s == param_slot
		{
			return LExpr::IdentityFunction {};
		}
	}

	LExpr::Function(Rc::new(LFunction {
		name,
		params: l_params,
		signature: params.signature.clone(),
		body_shape,
		body: Rc::new(body_expr),
	}))
}

fn analyze_obj_body(
	obj: &ObjBody,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LObjBody {
	match obj {
		ObjBody::MemberList(members) => {
			LObjBody::MemberList(analyze_obj_members(members, stack, taint))
		}
		ObjBody::ObjComp(comp) => LObjBody::ObjComp(Box::new(analyze_obj_comp(comp, stack, taint))),
	}
}

fn analyze_obj_members(
	members: &ObjMembers,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LObjMembers {
	let ObjMembers {
		locals,
		asserts,
		fields,
	} = members;

	// Names are analyzed in enclosing scope, they can't depend on locals or self/super
	let field_names: Vec<LFieldName> = fields
		.iter()
		.map(|f| match &f.name.value {
			FieldName::Fixed(s) => LFieldName::Fixed(s.clone()),
			FieldName::Dyn(e) => LFieldName::Dyn(analyze(e, stack, taint)),
		})
		.collect();

	let (usage, frame_shape, (l_binds, (l_asserts_opt, l_fields))) =
		stack.in_object_scope(|stack| {
			process_local_frame(locals, stack, taint, |stack, taint| {
				let l_asserts_opt = if asserts.is_empty() {
					None
				} else {
					let (shape, l_asserts) = stack.in_using_closure(|stack| {
						let mut l_asserts = Vec::with_capacity(asserts.len());
						for a in asserts {
							let mut assert_taint = AnalysisResult::default();
							l_asserts.push(analyze_assert(a, stack, &mut assert_taint));
							taint.taint_by(assert_taint);
						}
						l_asserts
					});
					Some(Rc::new(LObjAsserts {
						shape,
						asserts: l_asserts,
					}))
				};
				let mut l_fields = Vec::with_capacity(fields.len());
				for (f, name) in fields.iter().zip(field_names) {
					let value = stack.in_using_closure(|stack| {
						if let Some(params) = &f.params {
							analyze_function(name.function_name(), params, &f.value, stack, taint)
						} else {
							analyze(&f.value, stack, taint)
						}
					});
					l_fields.push(LFieldMember {
						name,
						plus: f.plus,
						visibility: f.visibility,
						value: Rc::new(value),
					});
				}
				(l_asserts_opt, l_fields)
			})
		});
	// `this` was allocated as the first local of the object's frame,
	// so its slot is 0 within that frame.
	let this_slot = usage.this_used.then_some(LocalSlot(0));
	LObjMembers {
		frame_shape,
		this: this_slot,
		set_dollar: usage.set_dollar,
		uses_super: usage.uses_super,
		locals: Rc::new(l_binds),
		asserts: l_asserts_opt,
		fields: l_fields,
	}
}

fn analyze_obj_comp(
	comp: &ObjComp,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LObjComp {
	let res = analyze_comp_specs(&comp.compspecs, stack, taint, |stack, taint| {
		let field_name = match &comp.field.name.value {
			FieldName::Fixed(s) => LFieldName::Fixed(s.clone()),
			FieldName::Dyn(e) => LFieldName::Dyn(analyze(e, stack, taint)),
		};

		let (usage, frame_shape, body) = stack.in_object_scope(|stack| {
			process_local_frame(&comp.locals, stack, taint, |stack, taint| {
				let value = stack.in_using_closure(|stack| {
					if let Some(params) = &comp.field.params {
						analyze_function(None, params, &comp.field.value, stack, taint)
					} else {
						analyze(&comp.field.value, stack, taint)
					}
				});
				LFieldMember {
					name: field_name,
					plus: comp.field.plus,
					visibility: comp.field.visibility,
					value: Rc::new(value),
				}
			})
		});
		(usage, frame_shape, body)
	});
	let (usage, frame_shape, (locals, field)) = res.inner;
	let this_slot = usage.this_used.then_some(LocalSlot(0));
	LObjComp {
		frame_shape: Rc::new(frame_shape),
		this: this_slot,
		set_dollar: usage.set_dollar,
		uses_super: usage.uses_super,
		locals: Rc::new(locals),
		field,
		compspecs: res.compspecs,
	}
}

fn analyze_arr_comp(
	inner: &Expr,
	specs: &[CompSpec],
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LExpr {
	let res = analyze_comp_specs(specs, stack, taint, |stack, taint| {
		stack.in_using_closure(|stack| analyze(inner, stack, taint))
	});
	let (value_shape, value) = res.inner;
	LExpr::ArrComp(Box::new(LArrComp {
		value_shape,
		value: Rc::new(value),
		compspecs: res.compspecs,
	}))
}

fn analyze_comp_specs<R>(
	specs: &[CompSpec],
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
	inside: impl FnOnce(&mut AnalysisStack, &mut AnalysisResult) -> R,
) -> CompSpecResult<R> {
	fn go<R>(
		idx: usize,
		specs: &[CompSpec],
		outer_depth: u32,
		stack: &mut AnalysisStack,
		taint: &mut AnalysisResult,
		inside: impl FnOnce(&mut AnalysisStack, &mut AnalysisResult) -> R,
	) -> (R, Vec<LCompSpec>) {
		if idx >= specs.len() {
			return (inside(stack, taint), Vec::new());
		}
		match &specs[idx] {
			CompSpec::IfSpec(IfSpecData { cond, .. }) => {
				let cond_l = analyze(cond, stack, taint);
				let (r, mut rest) = go(idx + 1, specs, outer_depth, stack, taint, inside);
				rest.insert(0, LCompSpec::If(cond_l));
				(r, rest)
			}
			CompSpec::ForSpec(ForSpecData { destruct, over }) => {
				let mut over_taint = AnalysisResult::default();
				let over_l = analyze(over, stack, &mut over_taint);
				let loop_invariant = over_taint.local_dependent_depth > outer_depth;
				taint.taint_by(over_taint);

				let mut alloc = FrameAlloc::new(stack);
				let closure = alloc.push_locals_closure();
				let Some(l_destruct) = alloc.alloc_destruct(destruct) else {
					stack.pop_closure(closure);
					return go(idx + 1, specs, outer_depth, stack, taint, inside);
				};
				let mut pending = alloc.finish();

				let var_analysis = AnalysisResult::default();
				pending.record_spec_init(&l_destruct, var_analysis);

				let body_frame = pending.finish();
				let (r, mut rest) =
					go(idx + 1, specs, outer_depth, body_frame.stack, taint, inside);
				body_frame.finish();
				let frame_shape = stack.pop_closure(closure);

				rest.insert(
					0,
					LCompSpec::For {
						frame_shape,
						destruct: l_destruct,
						over: over_l,
						loop_invariant,
					},
				);
				(r, rest)
			}
			#[cfg(feature = "exp-object-iteration")]
			CompSpec::ForObjSpec(data) => {
				let mut over_taint = AnalysisResult::default();
				let over_l = analyze(&data.over, stack, &mut over_taint);
				let loop_invariant = over_taint.local_dependent_depth > outer_depth;
				taint.taint_by(over_taint);

				let mut alloc = FrameAlloc::new(stack);
				let closure = alloc.push_locals_closure();
				let Some((_, key_slot)) = alloc.define_local(data.key.clone(), None) else {
					stack.pop_closure(closure);
					return go(idx + 1, specs, outer_depth, stack, taint, inside);
				};
				let Some(l_value) = alloc.alloc_destruct(&data.value) else {
					stack.pop_closure(closure);
					return go(idx + 1, specs, outer_depth, stack, taint, inside);
				};
				let mut pending = alloc.finish();

				let var_analysis = AnalysisResult::default();
				pending.record_spec_init(&LDestruct::Full(key_slot), var_analysis);
				pending.record_spec_init(&l_value, var_analysis);

				let body_frame = pending.finish();
				let (r, mut rest) =
					go(idx + 1, specs, outer_depth, body_frame.stack, taint, inside);
				body_frame.finish();
				let frame_shape = stack.pop_closure(closure);

				rest.insert(
					0,
					LCompSpec::ForObj {
						frame_shape,
						key: key_slot,
						visibility: data.visibility,
						value: l_value,
						over: over_l,
						loop_invariant,
					},
				);
				(r, rest)
			}
		}
	}
	let outer_depth = stack.depth;
	let (r, compspecs) = go(0, specs, outer_depth, stack, taint, inside);
	CompSpecResult {
		inner: r,
		compspecs,
	}
}

struct CompSpecResult<R> {
	inner: R,
	compspecs: Vec<LCompSpec>,
}

pub fn analyze_root(expr: &Expr, ctx: Vec<(IStr, LocalId)>) -> AnalysisReport {
	let mut stack = AnalysisStack::new();
	for (name, id) in ctx {
		stack.define_external_local(name, id);
	}

	let externals_count: u16 = stack
		.local_defs
		.len()
		.try_into()
		.expect("more than u16::MAX externals");
	let closure = stack.push_root_closure(externals_count);

	let mut taint = AnalysisResult::default();
	let lir = analyze(expr, &mut stack, &mut taint);

	let root_shape = stack.pop_closure(closure);
	debug_assert!(
		stack.closure_stack.is_empty(),
		"closure stack imbalance after analyze"
	);

	AnalysisReport {
		lir,
		root_shape,
		root_analysis: taint,
		diagnostics_list: stack.diagnostics,
		errored: stack.errored,
	}
}

pub struct AnalysisReport {
	pub lir: LExpr,
	pub root_shape: ClosureShape,
	pub root_analysis: AnalysisResult,
	pub diagnostics_list: Vec<Diagnostic>,
	pub errored: bool,
}

#[cfg(test)]
mod tests {
	#[test]
	#[cfg(not(feature = "exp-null-coaelse"))]
	fn snapshots() {
		use std::fs;

		use insta::{assert_snapshot, glob};
		use jrsonnet_ir::Source;

		use super::*;

		fn render_diagnostics(src: &str, diags: &[Diagnostic]) -> String {
			use std::fmt::Write;

			use hi_doc::{Formatting, SnippetBuilder, Text};

			let mut out = String::new();
			let mut unspanned = Vec::new();
			let mut spanned: Vec<&Diagnostic> = Vec::new();
			for d in diags {
				if d.span.is_some() {
					spanned.push(d);
				} else {
					unspanned.push(d);
				}
			}
			if !spanned.is_empty() {
				let mut builder = SnippetBuilder::new(src);
				for d in spanned {
					let span = d.span.as_ref().expect("spanned");
					let ab = match d.level {
						DiagLevel::Error => {
							builder.error(Text::fragment(d.message.clone(), Formatting::default()))
						}
						DiagLevel::Warning => builder
							.warning(Text::fragment(d.message.clone(), Formatting::default())),
					};
					ab.range(span.range()).build();
				}
				out.push_str(&hi_doc::source_to_ansi(&builder.build()));
			}
			for d in unspanned {
				let prefix = match d.level {
					DiagLevel::Error => "error",
					DiagLevel::Warning => "warning",
				};
				writeln!(out, "{prefix}: {}", d.message).expect("fmt");
			}
			out
		}
		fn fmt_depth(d: u32) -> String {
			if d == u32::MAX {
				"none".into()
			} else {
				d.to_string()
			}
		}

		glob!("analysis_tests/*.jsonnet", |path| {
			let code = fs::read_to_string(path).expect("read test file");
			let src = Source::new_virtual("<test>".into(), code.clone().into());
			let expr = crate::parse_jsonnet(&code, src.clone()).expect("parse");
			let report = analyze_root(&expr, Vec::new());

			let diagnostics = render_diagnostics(src.code(), &report.diagnostics_list);
			// Strip ANSI escapes from diagnostics so snapshots are readable.
			let diagnostics = strip_ansi_escapes::strip_str(&diagnostics);
			let rendered = format!(
				"--- source ---\n{}\n--- root analysis ---\nobject_dependent_depth: {}\nlocal_dependent_depth: {}\nerrored: {}\n--- diagnostics ---\n{}--- lir ---\n{:#?}\n",
				code.trim_end(),
				fmt_depth(report.root_analysis.object_dependent_depth),
				fmt_depth(report.root_analysis.local_dependent_depth),
				report.errored,
				diagnostics,
				report.lir,
			);
			assert_snapshot!(rendered);
		});
	}
}
