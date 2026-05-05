#![allow(clippy::box_default)]

pub mod interop;

pub mod import;
pub mod native;
pub mod val_extract;
pub mod val_make;
pub mod val_modify;
pub mod vars_tlas;

use std::{
	alloc::Layout,
	any::Any,
	borrow::Cow,
	cell::RefCell,
	ffi::{CStr, CString, OsStr},
	os::raw::{c_char, c_double, c_int, c_uint},
	path::{Path, PathBuf},
	rc::Rc,
};

use jrsonnet_evaluator::{
	AsPathLike, FileImportResolver, IStr, ImportResolver, Result, State, Val, apply_tla, bail,
	gc::WithCapacityExt as _,
	manifest::{JsonFormat, ManifestFormat, ToStringFormat},
	rustc_hash::FxHashMap,
	stack::set_stack_depth_limit,
	tla::TlaArg,
	trace::{CompactFormat, PathResolver, TraceFormat},
};
use jrsonnet_gcmodule::Acyclic;
use jrsonnet_ir::SourcePath;
use jrsonnet_stdlib::ContextInitializer;

/// WASM stub
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn _start() {}

/// Return the version string of the Jsonnet interpreter.
///
/// Conforms to [semantic versioning](http://semver.org/).
/// If this does not match `LIB_JSONNET_VERSION`
/// then there is a mismatch between header and compiled library.
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_version() -> &'static [u8; 8] {
	b"v0.22.0\0"
}

unsafe fn parse_path(input: &CStr) -> Cow<'_, Path> {
	#[cfg(target_family = "unix")]
	{
		use std::os::unix::ffi::OsStrExt;
		let str = OsStr::from_bytes(input.to_bytes());
		Cow::Borrowed(Path::new(str))
	}
	#[cfg(not(target_family = "unix"))]
	{
		let string = input.to_str().expect("bad utf-8");
		Cow::Borrowed(string.as_ref())
	}
}

unsafe fn unparse_path(input: &Path) -> CString {
	#[cfg(target_family = "unix")]
	{
		use std::os::unix::ffi::OsStrExt;
		let str = CString::new(input.as_os_str().as_bytes()).expect("input has zero byte in it");
		str
	}
	#[cfg(not(target_family = "unix"))]
	{
		let str = input.as_os_str().to_str().expect("bad utf-8");
		let cstr = CString::new(str).expect("input has NUL inside");
		cstr
	}
}

#[derive(Acyclic)]
struct VMImportResolver {
	inner: RefCell<Rc<dyn ImportResolver>>,
}
impl VMImportResolver {
	fn new(value: impl ImportResolver) -> Self {
		Self {
			inner: RefCell::new(Rc::new(value)),
		}
	}
}
impl ImportResolver for VMImportResolver {
	fn load_file_contents(&self, resolved: &SourcePath) -> Result<Vec<u8>> {
		self.inner.borrow().load_file_contents(resolved)
	}

	fn resolve_from(&self, from: &SourcePath, path: &dyn AsPathLike) -> Result<SourcePath> {
		self.inner.borrow().resolve_from(from, path)
	}

	fn resolve_from_default(&self, path: &dyn AsPathLike) -> Result<SourcePath> {
		self.inner.borrow().resolve_from_default(path)
	}
}

pub struct VM {
	state: State,
	manifest_format: Box<dyn ManifestFormat>,
	trailing_newline: bool,
	trace_format: Box<dyn TraceFormat>,
	tla_args: FxHashMap<IStr, TlaArg>,
}
impl VM {
	fn replace_import_resolver(&self, resolver: impl ImportResolver) {
		*(self.state.import_resolver() as &dyn Any)
			.downcast_ref::<VMImportResolver>()
			.expect("valid resolver ty")
			.inner
			.borrow_mut() = Rc::new(resolver);
	}
	fn add_jpath(&self, path: PathBuf) {
		let ir = self.state.import_resolver();
		let vmi = (ir as &dyn Any)
			.downcast_ref::<VMImportResolver>()
			.expect("valid resolver ty");
		let vmi = &mut *vmi.inner.borrow_mut();
		(vmi as &mut dyn Any)
			.downcast_mut::<FileImportResolver>()
			.expect("jpaths are not compatible with callback imports!")
			.add_jpath(path);
	}
}

/// Creates a new Jsonnet virtual machine.
#[unsafe(no_mangle)]
#[allow(clippy::box_default)]
pub extern "C" fn jsonnet_make() -> *mut VM {
	let mut state = State::builder();
	state
		.import_resolver(VMImportResolver::new(FileImportResolver::default()))
		.context_initializer(ContextInitializer::new(PathResolver::new_cwd_fallback()));
	let state = state.build();
	Box::into_raw(Box::new(VM {
		state,
		manifest_format: Box::new(JsonFormat::default()),
		trace_format: Box::new(CompactFormat::default()),
		trailing_newline: true,
		tla_args: FxHashMap::new(),
	}))
}

/// Complement of [`jsonnet_vm_make`].
#[unsafe(no_mangle)]
#[allow(clippy::boxed_local)]
pub extern "C" fn jsonnet_destroy(vm: Box<VM>) {
	drop(vm);
}

/// Set the maximum stack depth.
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_max_stack(_vm: &VM, v: c_uint) {
	set_stack_depth_limit(v as usize);
}

/// Set the number of objects required before a garbage collection cycle is allowed.
///
/// No-op for now
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_gc_min_objects(_vm: &VM, _v: c_uint) {}

/// Run the garbage collector after this amount of growth in the number of objects
///
/// No-op for now
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_gc_growth_trigger(_vm: &VM, _v: c_double) {}

/// Expect a string as output and don't JSON encode it.
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_string_output(vm: &mut VM, v: c_int) {
	vm.manifest_format = match v {
		0 => Box::new(JsonFormat::default()),
		1 => Box::new(ToStringFormat),
		_ => panic!("incorrect output format"),
	};
}

/// Enable/disable trailing newline in manifested/string output.
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_set_trailing_newline(vm: &mut VM, enable: c_int) {
	vm.trailing_newline = enable != 0;
}

/// Allocate, resize, or free a buffer.  This will abort if the memory cannot be allocated. It will
/// only return NULL if sz was zero.
///
/// # Safety
///
/// `buf` should be either previosly allocated by this library, or NULL
///
/// This function is most definitely broken, but it works somehow, see TODO inside
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_realloc(_vm: &VM, buf: *mut u8, sz: usize) -> *mut u8 {
	if buf.is_null() {
		if sz == 0 {
			return std::ptr::null_mut();
		}
		return unsafe {
			std::alloc::alloc(Layout::from_size_align(sz, std::mem::align_of::<u8>()).unwrap())
		};
	}
	// TODO: Somehow store size of allocation, because its real size is probally not 16 :D
	// OR (Alternative way of fixing this TODO)
	// TODO: Standard allocator uses malloc, and it doesn't uses allocation size,
	// TODO: so it should work in normal cases. Maybe force allocator for this library?
	let old_layout = Layout::from_size_align(16, std::mem::align_of::<u8>()).unwrap();
	if sz == 0 {
		unsafe { std::alloc::dealloc(buf, old_layout) };
		return std::ptr::null_mut();
	}
	unsafe { std::alloc::realloc(buf, old_layout, sz) }
}

/// Clean up a JSON subtree.
///
/// This is useful if you want to abort with an error mid-way through building a complex value.
#[unsafe(no_mangle)]
#[allow(clippy::boxed_local)]
pub extern "C" fn jsonnet_json_destroy(_vm: &VM, v: Box<Val>) {
	drop(v);
}

/// Set the number of lines of stack trace to display (0 for all of them).
#[unsafe(no_mangle)]
pub extern "C" fn jsonnet_max_trace(vm: &mut VM, v: c_uint) {
	if let Some(format) = vm.trace_format.as_any_mut().downcast_mut::<CompactFormat>() {
		format.max_trace = v as usize;
	} else {
		panic!("max_trace is not supported by current tracing format")
	}
}

/// Evaluate a file containing Jsonnet code, return a JSON string.
///
/// The returned string should be cleaned up with `jsonnet_realloc`.
///
/// # Safety
///
/// `filename` should be a NUL-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_evaluate_file(
	vm: &VM,
	filename: *const c_char,
	error: &mut c_int,
) -> *const c_char {
	let filename = unsafe { parse_path(CStr::from_ptr(filename)) };
	match vm
		.state
		.import(filename)
		.and_then(|val| apply_tla(&vm.tla_args, val))
		.and_then(|val| val.manifest(&vm.manifest_format))
	{
		Ok(v) => {
			*error = 0;
			CString::new(&*v as &str).unwrap().into_raw()
		}
		Err(e) => {
			*error = 1;
			let mut out = String::new();
			vm.trace_format.write_trace(&mut out, &e).unwrap();
			CString::new(&out as &str).unwrap().into_raw()
		}
	}
}

/// Evaluate a string containing Jsonnet code, return a JSON string.
///
/// The returned string should be cleaned up with `jsonnet_realloc`.
///
/// # Safety
///
/// `filename`, `snippet` should be a NUL-terminated strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_evaluate_snippet(
	vm: &VM,
	filename: *const c_char,
	snippet: *const c_char,
	error: &mut c_int,
) -> *const c_char {
	let filename = unsafe { CStr::from_ptr(filename) };
	let snippet = unsafe { CStr::from_ptr(snippet) };
	match vm
		.state
		.evaluate_snippet(filename.to_str().unwrap(), snippet.to_str().unwrap())
		.and_then(|val| apply_tla(&vm.tla_args, val))
		.and_then(|val| val.manifest(&vm.manifest_format))
	{
		Ok(mut v) => {
			if vm.trailing_newline {
				v.push('\n');
			}
			*error = 0;
			CString::new(&*v as &str).unwrap().into_raw()
		}
		Err(e) => {
			*error = 1;
			let mut out = String::new();
			vm.trace_format.write_trace(&mut out, &e).unwrap();
			CString::new(&out as &str).unwrap().into_raw()
		}
	}
}

fn val_to_multi(val: Val, format: &dyn ManifestFormat) -> Result<Vec<(IStr, IStr)>> {
	let Val::Obj(val) = val else {
		bail!("expected object as multi output")
	};
	let mut out = Vec::new();
	for (k, v) in val.iter(
		#[cfg(feature = "exp-preserve-order")]
		false,
	) {
		out.push((k, v?.manifest(format)?.into()));
	}
	Ok(out)
}

fn multi_to_raw(multi: Vec<(IStr, IStr)>, trailing_newline: bool) -> *const c_char {
	let mut out = Vec::new();
	for (i, (k, v)) in multi.iter().enumerate() {
		if i != 0 {
			out.push(0);
		}
		out.extend_from_slice(k.as_bytes());
		out.push(0);
		out.extend_from_slice(v.as_bytes());
		if trailing_newline {
			out.push(b'\n');
		}
	}
	out.push(0);
	out.push(0);
	let v = out.as_ptr();
	std::mem::forget(out);
	v.cast::<c_char>()
}

/// # Safety
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_evaluate_file_multi(
	vm: &VM,
	filename: *const c_char,
	error: &mut c_int,
) -> *const c_char {
	let filename = unsafe { parse_path(CStr::from_ptr(filename)) };
	match vm
		.state
		.import(filename)
		.and_then(|val| apply_tla(&vm.tla_args, val))
		.and_then(|val| val_to_multi(val, &vm.manifest_format))
	{
		Ok(v) => {
			*error = 0;
			multi_to_raw(v, vm.trailing_newline)
		}
		Err(e) => {
			*error = 1;
			let mut out = String::new();
			vm.trace_format.write_trace(&mut out, &e).unwrap();
			CString::new(&out as &str).unwrap().into_raw()
		}
	}
}

/// # Safety
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_evaluate_snippet_multi(
	vm: &VM,
	filename: *const c_char,
	snippet: *const c_char,
	error: &mut c_int,
) -> *const c_char {
	let filename = unsafe { CStr::from_ptr(filename) };
	let snippet = unsafe { CStr::from_ptr(snippet) };
	match vm
		.state
		.evaluate_snippet(filename.to_str().unwrap(), snippet.to_str().unwrap())
		.and_then(|val| apply_tla(&vm.tla_args, val))
		.and_then(|val| val_to_multi(val, &vm.manifest_format))
	{
		Ok(v) => {
			*error = 0;
			multi_to_raw(v, vm.trailing_newline)
		}
		Err(e) => {
			*error = 1;
			let mut out = String::new();
			vm.trace_format.write_trace(&mut out, &e).unwrap();
			CString::new(&out as &str).unwrap().into_raw()
		}
	}
}

fn val_to_stream(val: Val, format: &dyn ManifestFormat) -> Result<Vec<IStr>> {
	let Val::Arr(val) = val else {
		bail!("expected array as stream output")
	};
	let mut out = Vec::new();
	for item in val.iter() {
		out.push(item?.manifest(format)?.into());
	}
	Ok(out)
}

fn stream_to_raw(multi: Vec<IStr>, trailing_newline: bool) -> *const c_char {
	let mut out = Vec::new();
	for (i, v) in multi.iter().enumerate() {
		if i != 0 {
			out.push(0);
		}
		out.extend_from_slice(v.as_bytes());
		if trailing_newline {
			out.push(b'\n');
		}
	}
	out.push(0);
	out.push(0);
	let v = out.as_ptr();
	std::mem::forget(out);
	v.cast::<c_char>()
}

/// # Safety
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_evaluate_file_stream(
	vm: &VM,
	filename: *const c_char,
	error: &mut c_int,
) -> *const c_char {
	let filename = unsafe { parse_path(CStr::from_ptr(filename)) };
	match vm
		.state
		.import(filename)
		.and_then(|val| apply_tla(&vm.tla_args, val))
		.and_then(|val| val_to_stream(val, &vm.manifest_format))
	{
		Ok(v) => {
			*error = 0;
			stream_to_raw(v, vm.trailing_newline)
		}
		Err(e) => {
			*error = 1;
			let mut out = String::new();
			vm.trace_format.write_trace(&mut out, &e).unwrap();
			CString::new(&out as &str).unwrap().into_raw()
		}
	}
}

/// # Safety
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jsonnet_evaluate_snippet_stream(
	vm: &VM,
	filename: *const c_char,
	snippet: *const c_char,
	error: &mut c_int,
) -> *const c_char {
	let filename = unsafe { CStr::from_ptr(filename) };
	let snippet = unsafe { CStr::from_ptr(snippet) };
	match vm
		.state
		.evaluate_snippet(filename.to_str().unwrap(), snippet.to_str().unwrap())
		.and_then(|val| apply_tla(&vm.tla_args, val))
		.and_then(|val| val_to_stream(val, &vm.manifest_format))
	{
		Ok(v) => {
			*error = 0;
			stream_to_raw(v, vm.trailing_newline)
		}
		Err(e) => {
			*error = 1;
			let mut out = String::new();
			vm.trace_format.write_trace(&mut out, &e).unwrap();
			CString::new(&out as &str).unwrap().into_raw()
		}
	}
}
