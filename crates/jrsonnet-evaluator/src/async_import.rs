use std::{any::Any, cell::RefCell, future::Future, rc::Rc};

use jrsonnet_gcmodule::Acyclic;
use jrsonnet_ir::{IStr, Source, SourcePath, visit::Visitor};
use rustc_hash::FxHashMap;

use crate::{AsPathLike, FileData, ImportResolver, ResolvePathOwned, Result, State};

pub struct Import {
	path: ResolvePathOwned,
	expression: bool,
}

pub struct FoundImports(Vec<Import>);
impl Visitor for FoundImports {
	fn visit_import(&mut self, expression: bool, value: IStr) {
		self.0.push(Import {
			path: ResolvePathOwned::Str(value.to_string()),
			expression,
		});
	}
}

pub trait AsyncImportResolver {
	type Error;
	/// Resolves file path, e.g. `(/home/user/manifests, b.libjsonnet)` can correspond
	/// both to `/home/user/manifests/b.libjsonnet` and to `/home/user/${vendor}/b.libjsonnet`
	/// where `${vendor}` is a library path.
	///
	/// `from` should only be returned from [`ImportResolver::resolve`],
	/// or from other defined file, any other value may result in panic
	fn resolve_from(
		&self,
		from: &SourcePath,
		path: &dyn AsPathLike,
	) -> impl Future<Output = Result<SourcePath, Self::Error>>;
	fn resolve_from_default(
		&self,
		path: &dyn AsPathLike,
	) -> impl Future<Output = Result<SourcePath, Self::Error>> {
		async { self.resolve_from(&SourcePath::default(), path).await }
	}

	/// Load resolved file
	/// This should only be called with value returned
	/// from [`ImportResolver::resolve_file`]/[`ImportResolver::resolve`],
	/// this cannot be resolved using associated type,
	/// as the evaluator uses object instead of generic for [`ImportResolver`]
	fn load_file_contents(
		&self,
		resolved: &SourcePath,
	) -> impl Future<Output = Result<Vec<u8>, Self::Error>>;
}

#[derive(Acyclic, Default)]
pub struct ResolvedImportResolver {
	resolved: RefCell<FxHashMap<(SourcePath, ResolvePathOwned), (SourcePath, bool)>>,
}
impl ResolvedImportResolver {
	pub fn new() -> Self {
		Self::default()
	}
}
impl ImportResolver for ResolvedImportResolver {
	fn load_file_contents(&self, _resolved: &SourcePath) -> crate::Result<Vec<u8>> {
		unreachable!("all files should be loaded at this point");
	}

	fn resolve_from(&self, from: &SourcePath, path: &dyn AsPathLike) -> crate::Result<SourcePath> {
		Ok(self
			.resolved
			.borrow()
			.get(&(from.clone(), path.as_path().to_owned()))
			.expect("all imports should be resolved at this point")
			.0
			.clone())
	}

	fn resolve_from_default(&self, path: &dyn AsPathLike) -> crate::Result<SourcePath> {
		self.resolve_from(&SourcePath::default(), path)
	}
}

enum Job {
	LoadFile { path: SourcePath, parse: bool },
	ParseFile(SourcePath),
	ResolveImport { from: SourcePath, import: Import },
}

#[allow(clippy::future_not_send)]
pub async fn async_import<H>(
	s: State,
	handler: H,
	from: &SourcePath,
	path: &dyn AsPathLike,
) -> Result<SourcePath, H::Error>
where
	H: AsyncImportResolver,
{
	let resolved = (s.import_resolver() as &dyn Any)
		.downcast_ref::<ResolvedImportResolver>()
		.expect("for async imports, import_resolver should be set to ResolvedImportResolver");

	let entry = handler.resolve_from(from, path).await?;
	let mut queue = vec![Job::LoadFile {
		path: entry.clone(),
		parse: true,
	}];
	while let Some(job) = queue.pop() {
		match job {
			Job::LoadFile { path, parse } => {
				if !s.0.file_cache.borrow().contains_key(&path) {
					let data = handler.load_file_contents(&path).await?;
					s.0.file_cache
						.borrow_mut()
						.insert(path.clone(), FileData::new_bytes(data.as_slice().into()));
				}
				if parse {
					queue.push(Job::ParseFile(path));
				}
			}
			Job::ParseFile(path) => {
				if let Some(file) = s.0.file_cache.borrow_mut().get_mut(&path)
					&& file.parsed.is_none()
				{
					let Some(code) = file.get_string() else {
						continue;
					};
					let source = Source::new(path.clone(), code.clone());
					// If failed - then skip import
					file.parsed = crate::parse_jsonnet(&code, source).map(Rc::new).ok();
					if let Some(parsed) = &file.parsed {
						let mut imports = FoundImports(vec![]);
						imports.visit_expr(parsed);
						for import in imports.0 {
							queue.push(Job::ResolveImport {
								from: path.clone(),
								import,
							});
						}
					}
				}
			}
			Job::ResolveImport { from, import } => {
				{
					let mut resolved_map = resolved.resolved.borrow_mut();
					if let Some((resolved, expression)) =
						resolved_map.get_mut(&(from.clone(), import.path.clone()))
					{
						if import.expression && !*expression {
							*expression = true;
							queue.push(Job::ParseFile(resolved.clone()));
						}
						continue;
					}
				}
				let resolved_path = handler.resolve_from(&from, &import.path).await?;
				resolved.resolved.borrow_mut().insert(
					(from.clone(), import.path.clone()),
					(resolved_path.clone(), import.expression),
				);
				queue.push(Job::LoadFile {
					path: resolved_path,
					parse: import.expression,
				});
			}
		}
	}
	Ok(entry)
}
