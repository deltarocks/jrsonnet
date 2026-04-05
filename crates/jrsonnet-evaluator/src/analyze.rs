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

use std::{fmt::Write, rc::Rc};

use drop_bomb::DropBomb;
use hi_doc::{Formatting, SnippetBuilder, Text};
use jrsonnet_gcmodule::Acyclic;
use jrsonnet_interner::IStr;
use jrsonnet_ir::{
	function::FunctionSignature, ArgsDesc, AssertExpr, AssertStmt, BinaryOp, BinaryOpType,
	BindSpec, CompSpec, Destruct, Expr, ExprParams, FieldName, ForSpecData, IfElse, IfSpecData,
	ImportKind, LiteralType, NumValue, ObjBody, ObjComp, ObjMembers, Slice, SliceDesc, Span,
	Spanned, UnaryOpType, Visibility,
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
	Local(LocalId),
	Null,
	Bool(bool),
	Str(IStr),
	Num(NumValue),
	Arr(Rc<Vec<LExpr>>),
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
	LocalExpr {
		binds: Vec<LBind>,
		body: Box<LExpr>,
	},
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
pub struct LFunction {
	pub name: Option<IStr>,
	pub params: Vec<LParam>,
	pub signature: FunctionSignature,
	pub body: Rc<LExpr>,
}

#[derive(Debug, Acyclic)]
pub struct LParam {
	pub name: Option<IStr>,
	pub destruct: LDestruct,
	pub default: Option<Rc<LExpr>>,
}

#[derive(Debug, Acyclic)]
pub struct LBind {
	pub destruct: LDestruct,
	pub value: Rc<LExpr>,
}

#[derive(Debug, Clone, Acyclic)]
pub enum LDestruct {
	Full(LocalId),
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
	Keep(LocalId),
	Drop,
}

#[derive(Debug, Clone, Acyclic)]
pub struct LDestructField {
	pub name: IStr,
	pub into: Option<LDestruct>,
	pub default: Option<Rc<LExpr>>,
}

impl LDestruct {
	pub fn each_id<F: FnMut(LocalId)>(&self, f: &mut F) {
		match self {
			Self::Full(id) => f(*id),
			#[cfg(feature = "exp-destruct")]
			Self::Skip => {}
			#[cfg(feature = "exp-destruct")]
			Self::Array { start, rest, end } => {
				for d in start {
					d.each_id(f);
				}
				if let Some(LDestructRest::Keep(id)) = rest {
					f(*id);
				}
				for d in end {
					d.each_id(f);
				}
			}
			#[cfg(feature = "exp-destruct")]
			Self::Object { fields, rest } => {
				for field in fields {
					if let Some(into) = &field.into {
						into.each_id(f);
					} else {
						unreachable!("shorthand object destruct must store `into`");
					}
				}
				if let Some(LDestructRest::Keep(id)) = rest {
					f(*id);
				}
			}
		}
	}

	pub fn ids(&self) -> SmallVec<[LocalId; 1]> {
		let mut out = SmallVec::new();
		self.each_id(&mut |id| out.push(id));
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
	/// If current object identity (`super`/`this`/`$`) is used, `this` should be saved to the specified local
	pub this: Option<LocalId>,
	/// Set if dollar should also be assigned to object identity, `this` should also be set (TODO: proper type-level validation)
	pub set_dollar: bool,
	/// True iff `super` is referenced by this object's members.
	pub uses_super: bool,

	pub locals: Rc<Vec<LBind>>,
	pub asserts: Rc<Vec<LAssertStmt>>,
	pub fields: Vec<LFieldMember>,
}

#[derive(Debug, Acyclic)]
pub struct LObjComp {
	pub this: Option<LocalId>,
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
	pub value: Rc<LExpr>,
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
	pub value: Rc<LExpr>,
	pub compspecs: Vec<LCompSpec>,
}

#[derive(Debug, Acyclic)]
pub enum LCompSpec {
	If(LExpr),
	For {
		destruct: LDestruct,
		over: LExpr,
		/// Is `over` does not depend on any variable introduced by an earlier for-spec in this comprehension chain
		loop_invariant: bool,
	},
}

// TODO: Binding frame state machine:
// Pending => AllocIds => Initialize => Body => Exit

/// Frame state: `LocalIds` allocated, values not yet analysed.
struct PendingInit {
	first_in_frame: LocalId,
	first_after_frame: LocalId,
	bomb: DropBomb,
}

/// Frame state: values analysed, body not yet walked.
struct PendingBody {
	first_in_frame: LocalId,
	first_after_frame: LocalId,
	closures: Closures,
	bomb: DropBomb,
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
	/// ```
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
	fn new(first_in_frame: LocalId) -> Self {
		Self {
			referenced: Vec::new(),
			spec_shapes: Vec::new(),
			first_in_frame,
		}
	}

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

	diagnostics: Vec<Diagnostic>,
	/// Whenever analysis would be broken due to static analysis error.
	errored: bool,
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
			diagnostics: Vec::new(),
			errored: false,
		}
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

	fn use_local(
		&mut self,
		name: &IStr,
		span: Span,
		taint: &mut AnalysisResult,
	) -> Option<LocalId> {
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
		Some(id)
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

	/// Define a new local inside a frame currently being built.
	fn define_local(
		&mut self,
		name: IStr,
		span: Option<Span>,
		frame_start: LocalId,
	) -> Option<LocalId> {
		let id = self.next_local_id();
		let stack = self.local_by_name.entry(name.clone()).or_default();
		if let Some(&existing) = stack.last() {
			if !existing.defined_before(frame_start) {
				self.report_error(
					format!("local is already defined in the current frame: {name}"),
					span,
				);
				return None;
			}
		}
		stack.push(id);
		self.local_defs.push(LocalDefinition {
			name,
			span,
			defined_at_depth: self.depth,
			used_at_depth: u32::MAX,
			used_by_sibling: false,
			analysis: AnalysisResult::default(),
			analyzed: false,
			scratch_referenced: false,
		});
		Some(id)
	}
}

impl Default for AnalysisStack {
	fn default() -> Self {
		Self::new()
	}
}

impl AnalysisStack {
	fn alloc_destruct(&mut self, destruct: &Destruct, frame_start: LocalId) -> Option<LDestruct> {
		match destruct {
			Destruct::Full(name) => {
				let id =
					self.define_local(name.value.clone(), Some(name.span.clone()), frame_start)?;
				Some(LDestruct::Full(id))
			}
			#[cfg(feature = "exp-destruct")]
			Destruct::Skip => Some(LDestruct::Skip),
			#[cfg(feature = "exp-destruct")]
			Destruct::Array { start, rest, end } => {
				let start = start
					.iter()
					.map(|d| self.alloc_destruct(d, frame_start))
					.collect::<Option<Vec<_>>>()?;
				let rest = match rest {
					Some(jrsonnet_ir::DestructRest::Keep(name)) => {
						let id = self.define_local(name.clone(), None, frame_start)?;
						Some(LDestructRest::Keep(id))
					}
					Some(jrsonnet_ir::DestructRest::Drop) => Some(LDestructRest::Drop),
					None => None,
				};
				let end = end
					.iter()
					.map(|d| self.alloc_destruct(d, frame_start))
					.collect::<Option<Vec<_>>>()?;
				Some(LDestruct::Array { start, rest, end })
			}
			#[cfg(feature = "exp-destruct")]
			Destruct::Object { fields, rest } => {
				let mut l_fields: Vec<(IStr, LDestruct)> = Vec::with_capacity(fields.len());
				// Two passes: first allocate ALL destruct LocalIds, then
				// analyse defaults (which may reference later fields).
				let mut l_fields: Vec<(IStr, LDestruct)> = Vec::with_capacity(fields.len());
				for (name, into, _default) in fields {
					let into = if let Some(inner) = into {
						self.alloc_destruct(inner, frame_start)?
					} else {
						let id = self.define_local(name.clone(), None, frame_start)?;
						LDestruct::Full(id)
					};
					l_fields.push((name.clone(), into));
				}
				// Second pass: all locals exist, so defaults can reference
				// any sibling.
				let l_fields: Vec<LDestructField> = l_fields
					.into_iter()
					.zip(fields.iter())
					.map(|((name, into), (_n, _i, default))| {
						let default = default.as_ref().map(|e| {
							let mut default_taint = AnalysisResult::default();
							Rc::new(analyze(&e.value, self, &mut default_taint))
						});
						LDestructField {
							name,
							into: Some(into),
							default,
						}
					})
					.collect();
				let rest = match rest {
					Some(jrsonnet_ir::DestructRest::Keep(name)) => {
						let id = self.define_local(name.clone(), None, frame_start)?;
						Some(LDestructRest::Keep(id))
					}
					Some(jrsonnet_ir::DestructRest::Drop) => Some(LDestructRest::Drop),
					None => None,
				};
				Some(LDestruct::Object {
					fields: l_fields,
					rest,
				})
			}
		}
	}

	// TODO: Proper state machine
	fn begin_frame_alloc(&mut self) -> LocalId {
		self.next_local_id()
	}

	fn finish_frame_alloc(&mut self, first_in_frame: LocalId) -> PendingInit {
		let first_after_frame = self.next_local_id();
		PendingInit {
			first_in_frame,
			first_after_frame,
			bomb: DropBomb::new("PendingInit must be passed to finish_frame_init"),
		}
	}

	/// Record the analysis of a spec's value: stamp every id bound by the
	/// spec with `analysis`, collect the spec's same-frame references, and
	/// append them to `closures`.
	fn record_spec_init(
		&mut self,
		pending: &PendingInit,
		destruct: &LDestruct,
		analysis: AnalysisResult,
		closures: &mut Closures,
	) {
		let mut refs: SmallVec<[LocalId; 4]> = SmallVec::new();
		for i in pending.first_in_frame.0..pending.first_after_frame.0 {
			let def = &mut self.local_defs[i as usize];
			if def.scratch_referenced {
				refs.push(LocalId(i));
				def.scratch_referenced = false;
			}
		}

		let mut ids_count = 0;
		destruct.each_id(&mut |id| {
			ids_count += 1;
			let def = &mut self.local_defs[id.idx()];
			debug_assert!(!def.analyzed, "sanity: local {:?} analysed twice", def.name);
			def.analysis = analysis;
			def.analyzed = true;
		});
		closures.push_spec(ids_count, &refs);
	}

	/// After all specs are analysed, propagate dependency information between
	/// siblings to a fix-point, then switch to "body" mode.
	fn finish_frame_init(&mut self, pending: PendingInit, closures: Closures) -> PendingBody {
		let PendingInit {
			first_in_frame,
			first_after_frame,
			mut bomb,
		} = pending;
		bomb.defuse();

		debug_assert_eq!(
			first_after_frame,
			self.next_local_id(),
			"frame initialisation left unfinished locals"
		);

		debug_assert_eq!(
			closures.spec_shapes.iter().map(|(_, d)| *d).sum::<usize>(),
			(first_after_frame.0 - first_in_frame.0) as usize,
			"closures destruct-id counts must match frame local count"
		);

		let mut changed = true;
		while changed {
			changed = false;
			for spec in closures.iter_specs() {
				for id_raw in spec.ids.clone() {
					let user = LocalId(id_raw);
					for &used in spec.references {
						changed |= self.propagate_analysis(user, used);
					}
				}
			}
		}

		self.depth += 1;
		PendingBody {
			first_in_frame,
			first_after_frame,
			closures,
			bomb: DropBomb::new("PendingBody must be passed to finish_frame_body"),
		}
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

	/// After the body is processed, drop the frame's locals and emit any
	/// "unused local" warnings.
	fn finish_frame_body(&mut self, pending: PendingBody) {
		let PendingBody {
			first_in_frame,
			first_after_frame,
			closures,
			mut bomb,
		} = pending;
		bomb.defuse();
		self.depth -= 1;

		debug_assert_eq!(
			first_after_frame,
			self.next_local_id(),
			"nested scopes must be popped before outer frames"
		);

		let mut changed = true;
		while changed {
			changed = false;
			for spec in closures.iter_specs() {
				// Effective used_at_depth for the spec = min over its ids.
				let mut min_used_at = u32::MAX;
				for id_raw in spec.ids.clone() {
					min_used_at = min_used_at.min(self.local_defs[id_raw as usize].used_at_depth);
				}
				if min_used_at == u32::MAX {
					continue;
				}
				for &used in spec.references {
					let used_def = &mut self.local_defs[used.idx()];
					if min_used_at < used_def.used_at_depth {
						used_def.used_at_depth = min_used_at;
						changed = true;
					}
				}
			}
		}

		let drained: Vec<LocalDefinition> = self.local_defs.drain(first_in_frame.idx()..).collect();
		for (i, def) in drained.iter().enumerate().rev() {
			let id = LocalId(first_in_frame.0 + i as u32);
			let stack = self
				.local_by_name
				.get_mut(&def.name)
				.expect("local must be in name map");
			let popped = stack.pop().expect("name stack should not be empty");
			debug_assert_eq!(popped, id, "name stack integrity");
			if stack.is_empty() {
				self.local_by_name.remove(&def.name);
			}

			if def.used_at_depth == u32::MAX {
				if def.used_by_sibling {
					self.report_warning(
						format!("local is only referenced by unused siblings: {}", def.name),
						def.span.clone(),
					);
				} else {
					self.report_warning(format!("unused local: {}", def.name), def.span.clone());
				}
			} else if def.analysis.local_dependent_depth > def.defined_at_depth
				&& def.analysis.object_dependent_depth > def.defined_at_depth
				&& def.defined_at_depth != 0
			{
				// The value doesn't depend on anything defined at or inside
				// this local's scope - can be hoisted, unfortunately not automatically.
				self.report_warning(
					format!("local could be hoisted to an outer scope: {}", def.name),
					def.span.clone(),
				);
			}
		}
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
	// TODO: proper state machine
	fn enter_object_scope(&mut self) -> ObjectScope {
		let is_outermost = self.first_object_depth == u32::MAX;
		let scope = ObjectScope {
			this_id: self.push_pseudo_local(names::this()),
			is_outermost,
			prev_this_local: self.this_local,
			prev_dollar_alias: self.dollar_alias,
			prev_cur_self_used: self.cur_self_used,
			prev_cur_super_used: self.cur_super_used,
			prev_dollar_used: is_outermost.then_some(self.dollar_used),
			prev_last_object: self.last_object_depth,
			prev_first_object: self.first_object_depth,
		};

		self.this_local = Some(scope.this_id);
		if is_outermost {
			self.dollar_alias = Some(scope.this_id);
			self.first_object_depth = self.depth;
			self.dollar_used = false;
		}
		self.last_object_depth = self.depth;
		self.cur_self_used = false;
		self.cur_super_used = false;
		scope
	}

	fn leave_object_scope(&mut self, scope: ObjectScope) -> ObjectUsage {
		let _ = self.local_defs.pop().expect("this pseudo-local exists");
		debug_assert_eq!(self.local_defs.len(), scope.this_id.0 as usize);

		let set_dollar = scope.is_outermost && self.dollar_used;
		let usage = ObjectUsage {
			this_id: scope.this_id,
			this_used: self.cur_self_used || self.cur_super_used || set_dollar,
			uses_super: self.cur_super_used,
			set_dollar,
		};

		self.this_local = scope.prev_this_local;
		self.dollar_alias = scope.prev_dollar_alias;
		self.cur_self_used = scope.prev_cur_self_used;
		self.cur_super_used = scope.prev_cur_super_used;
		if let Some(prev) = scope.prev_dollar_used {
			self.dollar_used = prev;
		}
		self.last_object_depth = scope.prev_last_object;
		self.first_object_depth = scope.prev_first_object;

		usage
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
		id
	}

	fn use_this(&mut self, taint: &mut AnalysisResult) -> Option<LocalId> {
		let id = self.this_local?;
		self.cur_self_used = true;
		self.use_pseudo_local(id, taint);
		Some(id)
	}

	fn use_super(&mut self, taint: &mut AnalysisResult) -> Option<()> {
		let id = self.this_local?;
		self.cur_super_used = true;
		self.use_pseudo_local(id, taint);
		Some(())
	}

	fn use_dollar(&mut self, taint: &mut AnalysisResult) -> Option<LocalId> {
		let id = self.dollar_alias?;
		self.dollar_used = true;
		self.use_pseudo_local(id, taint);
		Some(id)
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
}

struct ObjectUsage {
	this_id: LocalId,
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
				LExpr::Local,
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
				LExpr::Local,
			),
			LiteralType::Null => LExpr::Null,
			LiteralType::True => LExpr::Bool(true),
			LiteralType::False => LExpr::Bool(false),
		},
		Expr::Str(s) => LExpr::Str(s.clone()),
		Expr::Num(n) => LExpr::Num(*n),
		Expr::Var(v) => stack
			.use_local(&v.value, v.span.clone(), taint)
			.map_or_else(|| LExpr::BadLocal("ref"), LExpr::Local),
		Expr::Arr(a) => LExpr::Arr(Rc::new(
			a.iter().map(|v| analyze(v, stack, taint)).collect(),
		)),
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
	let (_frame_start, l_binds, body_expr) =
		process_local_frame(binds, stack, taint, |stack, taint| {
			analyze(body, stack, taint)
		});
	LExpr::LocalExpr {
		binds: l_binds,
		body: Box::new(body_expr),
	}
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

fn alloc_bind_destruct(
	bind: &BindSpec,
	stack: &mut AnalysisStack,
	frame_start: LocalId,
) -> Option<LDestruct> {
	match bind {
		BindSpec::Field { into, .. } => stack.alloc_destruct(into, frame_start),
		BindSpec::Function { name, .. } => stack
			.define_local(name.clone(), None, frame_start)
			.map(LDestruct::Full),
	}
}

fn process_local_frame<R>(
	binds: &[BindSpec],
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
	body_fn: impl FnOnce(&mut AnalysisStack, &mut AnalysisResult) -> R,
) -> (LocalId, Vec<LBind>, R) {
	let frame_start = stack.begin_frame_alloc();

	let mut destructs: Vec<Option<LDestruct>> = Vec::with_capacity(binds.len());
	for bind in binds {
		destructs.push(alloc_bind_destruct(bind, stack, frame_start));
	}
	let pending = stack.finish_frame_alloc(frame_start);

	let mut closures = Closures::new(frame_start);
	let mut l_binds: Vec<LBind> = Vec::with_capacity(binds.len());
	for (bind, destruct) in binds.iter().zip(destructs.into_iter()) {
		let mut value_taint = AnalysisResult::default();
		let value = analyze_bind_value(bind, stack, &mut value_taint);
		taint.taint_by(value_taint);
		if let Some(destruct) = destruct {
			stack.record_spec_init(&pending, &destruct, value_taint, &mut closures);
			l_binds.push(LBind {
				destruct,
				value: Rc::new(value),
			});
		} else {
			closures.push_spec(0, &[]);
		}
	}

	let body_frame = stack.finish_frame_init(pending, closures);
	let result = body_fn(stack, taint);
	stack.finish_frame_body(body_frame);

	(frame_start, l_binds, result)
}

fn analyze_function(
	name: Option<IStr>,
	params: &ExprParams,
	body: &Expr,
	stack: &mut AnalysisStack,
	taint: &mut AnalysisResult,
) -> LExpr {
	let frame_start = stack.begin_frame_alloc();

	let mut param_destructs: Vec<Option<LDestruct>> = Vec::with_capacity(params.exprs.len());
	for p in &params.exprs {
		param_destructs.push(stack.alloc_destruct(&p.destruct, frame_start));
	}

	let pending = stack.finish_frame_alloc(frame_start);

	let mut closures = Closures::new(frame_start);
	let mut l_params: Vec<LParam> = Vec::with_capacity(params.exprs.len());
	for (p, destruct) in params.exprs.iter().zip(param_destructs.into_iter()) {
		let mut value_taint = AnalysisResult::default();
		let default = p
			.default
			.as_ref()
			.map(|d| Rc::new(analyze(d, stack, &mut value_taint)));
		taint.taint_by(value_taint);
		if let Some(destruct) = destruct {
			let name = match &p.destruct {
				Destruct::Full(n) => Some(n.value.clone()),
				#[cfg(feature = "exp-destruct")]
				_ => None,
			};
			stack.record_spec_init(&pending, &destruct, value_taint, &mut closures);
			l_params.push(LParam {
				name,
				destruct,
				default,
			});
		} else {
			closures.push_spec(0, &[]);
		}
	}

	let body_frame = stack.finish_frame_init(pending, closures);
	let body_expr = analyze(body, stack, taint);
	stack.finish_frame_body(body_frame);

	LExpr::Function(Rc::new(LFunction {
		name,
		params: l_params,
		signature: params.signature.clone(),
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

	let scope = stack.enter_object_scope();
	let (_frame_start, l_binds, (l_asserts, l_fields)) =
		process_local_frame(locals, stack, taint, |stack, taint| {
			let mut l_asserts = Vec::with_capacity(asserts.len());
			for a in asserts {
				let mut assert_taint = AnalysisResult::default();
				l_asserts.push(analyze_assert(a, stack, &mut assert_taint));
				taint.taint_by(assert_taint);
			}
			let mut l_fields = Vec::with_capacity(fields.len());
			for (f, name) in fields.iter().zip(field_names) {
				let value = if let Some(params) = &f.params {
					analyze_function(name.function_name(), params, &f.value, stack, taint)
				} else {
					analyze(&f.value, stack, taint)
				};
				l_fields.push(LFieldMember {
					name,
					plus: f.plus,
					visibility: f.visibility,
					value: Rc::new(value),
				});
			}
			(l_asserts, l_fields)
		});
	let usage = stack.leave_object_scope(scope);
	LObjMembers {
		this: usage.this_used.then_some(usage.this_id),
		set_dollar: usage.set_dollar,
		uses_super: usage.uses_super,
		locals: Rc::new(l_binds),
		asserts: Rc::new(l_asserts),
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

		let scope = stack.enter_object_scope();
		let body = process_local_frame(&comp.locals, stack, taint, |stack, taint| {
			let value = if let Some(params) = &comp.field.params {
				analyze_function(None, params, &comp.field.value, stack, taint)
			} else {
				analyze(&comp.field.value, stack, taint)
			};
			LFieldMember {
				name: field_name,
				plus: comp.field.plus,
				visibility: comp.field.visibility,
				value: Rc::new(value),
			}
		});
		let usage = stack.leave_object_scope(scope);
		(usage, body)
	});
	let (usage, (_frame_start, locals, field)) = res.inner;
	LObjComp {
		this: usage.this_used.then_some(usage.this_id),
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
		analyze(inner, stack, taint)
	});
	LExpr::ArrComp(Box::new(LArrComp {
		value: Rc::new(res.inner),
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

				let frame_start = stack.begin_frame_alloc();
				let Some(l_destruct) = stack.alloc_destruct(destruct, frame_start) else {
					return go(idx + 1, specs, outer_depth, stack, taint, inside);
				};
				let pending = stack.finish_frame_alloc(frame_start);

				let var_analysis = AnalysisResult::default();
				let mut closures = Closures::new(frame_start);
				stack.record_spec_init(&pending, &l_destruct, var_analysis, &mut closures);

				let body_frame = stack.finish_frame_init(pending, closures);
				let (r, mut rest) = go(idx + 1, specs, outer_depth, stack, taint, inside);
				stack.finish_frame_body(body_frame);

				rest.insert(
					0,
					LCompSpec::For {
						destruct: l_destruct,
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

	let mut taint = AnalysisResult::default();
	let lir = analyze(expr, &mut stack, &mut taint);

	AnalysisReport {
		lir,
		root_analysis: taint,
		diagnostics_list: stack.diagnostics,
		errored: stack.errored,
	}
}

fn render_diagnostics(src: &str, diags: &[Diagnostic]) -> String {
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
				DiagLevel::Warning => {
					builder.warning(Text::fragment(d.message.clone(), Formatting::default()))
				}
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

pub struct AnalysisReport {
	pub lir: LExpr,
	pub root_analysis: AnalysisResult,
	pub diagnostics_list: Vec<Diagnostic>,
	pub errored: bool,
}

#[cfg(test)]
mod tests {
	use std::fs;

	use insta::{assert_snapshot, glob};
	use jrsonnet_ir::Source;

	use super::*;

	#[test]
	fn snapshots() {
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

	fn fmt_depth(d: u32) -> String {
		if d == u32::MAX {
			"none".into()
		} else {
			d.to_string()
		}
	}
}
