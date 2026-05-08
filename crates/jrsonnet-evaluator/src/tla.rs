use std::{collections::HashMap, hash::BuildHasher};

use jrsonnet_gcmodule::Trace;
use jrsonnet_interner::IStr;
use jrsonnet_ir::{SourceFifo, SourcePath};

use crate::{
	Result, Thunk, Val, ensure_sufficient_stack,
	function::{CallLocation, PreparedFuncVal},
	in_description_frame, with_state,
};

#[derive(Clone, Trace)]
pub enum TlaArg {
	String(IStr),
	Val(Val),
	Lazy(Thunk<Val>),
	Import(String),
	ImportStr(String),
	InlineCode(String),
}
impl TlaArg {
	pub fn evaluate_tailstrict(&self) -> Result<Val> {
		ensure_sufficient_stack(|| match self {
			Self::String(s) => Ok(Val::string(s.clone())),
			Self::Val(val) => Ok(val.clone()),
			Self::Lazy(lazy) => Ok(lazy.evaluate()?),
			Self::Import(p) => with_state(|s| {
				let resolved = s.resolve_from_default(&p.as_str())?;
				s.import_resolved(resolved)
			}),
			Self::ImportStr(p) => with_state(|s| {
				let resolved = s.resolve_from_default(&p.as_str())?;
				s.import_resolved_str(resolved).map(Val::string)
			}),
			Self::InlineCode(p) => with_state(|s| {
				let resolved =
					SourcePath::new(SourceFifo("<inline code>".to_owned(), p.as_bytes().into()));
				s.import_resolved(resolved)
			}),
		})
	}
	pub fn evaluate(&self) -> Result<Thunk<Val>> {
		match self {
			Self::String(s) => Ok(Thunk::evaluated(Val::string(s.clone()))),
			Self::Val(val) => Ok(Thunk::evaluated(val.clone())),
			Self::Lazy(lazy) => Ok(lazy.clone()),
			Self::Import(p) => with_state(|s| {
				let resolved = s.resolve_from_default(&p.as_str())?;
				Ok(Thunk!(move || s.import_resolved(resolved)))
			}),
			Self::ImportStr(p) => with_state(|s| {
				let resolved = s.resolve_from_default(&p.as_str())?;
				Ok(Thunk!(move || s
					.import_resolved_str(resolved)
					.map(Val::string)))
			}),
			Self::InlineCode(p) => with_state(|s| {
				let resolved =
					SourcePath::new(SourceFifo("<inline code>".to_owned(), p.as_bytes().into()));
				Ok(Thunk!(move || s.import_resolved(resolved)))
			}),
		}
	}
}

pub fn apply_tla<H: BuildHasher>(args: &HashMap<IStr, TlaArg, H>, val: Val) -> Result<Val> {
	Ok(if let Val::Func(func) = val {
		in_description_frame(
			|| "during TLA call".to_owned(),
			|| {
				let mut names = Vec::with_capacity(args.len());
				let mut values = Vec::with_capacity(args.len());
				for (name, value) in args {
					names.push(name.clone());
					values.push(value.evaluate()?);
				}
				let prepared = PreparedFuncVal::new(func, 0, &names)?;
				prepared.call(CallLocation::native(), &[], &values)
			},
		)?
	} else {
		val
	})
}
