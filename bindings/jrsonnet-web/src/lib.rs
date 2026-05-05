use std::result::Result;

use jrsonnet_evaluator::{
	NumValue, Result as JrResult, SourcePath, SourceUrl, State, StateBuilder, Val,
	async_import::{ResolvedImportResolver, async_import},
	error,
	function::builtin::{NativeCallback, NativeCallbackHandler},
	manifest::{JsonFormat, ManifestFormat, StringFormat, ToStringFormat, YamlStreamFormat},
	trace::{JsFormat, PathResolver, TraceFormat},
	with_state,
};
use jrsonnet_formatter::FormatOptions;
use jrsonnet_gcmodule::Trace;
use jrsonnet_stdlib::{IniFormat, TomlFormat, XmlJsonmlFormat, YamlFormat};
use jrsonnet_types::ValType;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum ValKind {
	Null,
	Bool,
	Num,
	Str,
	Arr,
	Obj,
	Func,
}

#[wasm_bindgen(inline_js = r"
export class JrsonnetError extends Error {
	constructor(message, frames) {
		super(message);
		this.name = 'JrsonnetError';
		this.frames = frames;
	}
}
export function makeJrsonnetError(message, frames) {
	return new JrsonnetError(message, frames);
}
")]
extern "C" {
	#[wasm_bindgen(js_name = makeJrsonnetError)]
	fn make_jrsonnet_error(message: &str, frames: js_sys::Array) -> JsValue;
}

#[wasm_bindgen(typescript_custom_section)]
const TS_JRSONNET_ERROR: &'static str = r"
export interface JrsonnetFrame {
	desc: string;
	path?: string;
	line?: number;
	column?: number;
}
export class JrsonnetError extends Error {
	name: 'JrsonnetError';
	frames: JrsonnetFrame[];
}
";

fn jrsonnet_js_error(e: &jrsonnet_evaluator::Error) -> JsValue {
	let msg = e.error().to_string();
	// let msg = format.format(e).unwrap_or_else(|_| e.to_string());
	let frames = js_sys::Array::new();
	for el in &e.trace().0 {
		let frame = js_sys::Object::new();
		let _ = js_sys::Reflect::set(
			&frame,
			&JsValue::from_str("desc"),
			&JsValue::from_str(&el.desc),
		);
		if let Some(loc) = &el.location {
			let path = loc.0.source_path().to_string();
			let _ = js_sys::Reflect::set(
				&frame,
				&JsValue::from_str("path"),
				&JsValue::from_str(&path),
			);
			let mapped = loc.0.map_source_locations(&[loc.1, loc.2]);
			let _ = js_sys::Reflect::set(
				&frame,
				&JsValue::from_str("line"),
				&JsValue::from(mapped[0].line),
			);
			let _ = js_sys::Reflect::set(
				&frame,
				&JsValue::from_str("column"),
				&JsValue::from(mapped[0].column),
			);
		}
		frames.push(&frame);
	}
	make_jrsonnet_error(&msg, frames)
}

impl From<ValType> for ValKind {
	fn from(v: ValType) -> Self {
		match v {
			ValType::Null => Self::Null,
			ValType::Bool => Self::Bool,
			ValType::Num => Self::Num,
			ValType::Str => Self::Str,
			ValType::Arr => Self::Arr,
			ValType::Obj => Self::Obj,
			ValType::Func => Self::Func,
		}
	}
}

#[wasm_bindgen]
pub struct WasmVal {
	val: Val,
	state: Option<State>,
}

impl WasmVal {
	fn new(val: Val) -> Self {
		Self { val, state: None }
	}
	fn with_state(val: Val, state: State) -> Self {
		Self {
			val,
			state: Some(state),
		}
	}
	fn child(&self, val: Val) -> Self {
		Self {
			val,
			state: self.state.clone(),
		}
	}
	fn run<R>(&self, f: impl FnOnce(&Val) -> R) -> R {
		if let Some(state) = &self.state {
			let _guard = state.try_enter();
			f(&self.val)
		} else {
			f(&self.val)
		}
	}
	fn manifest_with(&self, format: impl ManifestFormat) -> Result<String, JsValue> {
		self.run(|v| v.manifest(format))
			.map_err(|e| jrsonnet_js_error(&e))
	}
}

#[wasm_bindgen]
impl WasmVal {
	pub fn null() -> Self {
		Self::new(Val::Null)
	}
	pub fn bool(b: bool) -> Self {
		Self::new(Val::Bool(b))
	}
	pub fn num(n: f64) -> Result<Self, JsError> {
		let n = NumValue::new(n)
			.ok_or_else(|| JsError::new("only finite numbers are supported by jsonnet"))?;
		Ok(Self::new(Val::num(n)))
	}
	pub fn string(s: String) -> Self {
		Self::new(Val::string(s))
	}
	pub fn arr(items: Vec<WasmVal>) -> Self {
		Self::new(Val::arr(
			items.into_iter().map(|v| v.val).collect::<Vec<_>>(),
		))
	}
	pub fn func(
		params: Vec<String>,

		#[wasm_bindgen(unchecked_param_type = "(...args: WasmVal[]) => WasmVal")]
		callback: js_sys::Function,
	) -> Self {
		#[allow(deprecated)]
		Self::new(Val::function(NativeCallback::new(
			params,
			JsHandler { func: callback },
		)))
	}

	#[wasm_bindgen(getter)]
	pub fn kind(&self) -> ValKind {
		self.val.value_type().into()
	}
	pub fn as_bool(&self) -> Option<bool> {
		self.val.as_bool()
	}
	pub fn as_num(&self) -> Option<f64> {
		self.val.as_num()
	}
	pub fn as_string(&self) -> Option<String> {
		self.val.as_str().map(|s| s.to_string())
	}
	pub fn arr_len(&self) -> Option<u32> {
		self.val.as_arr().map(|a| a.len())
	}
	pub fn arr_at(&self, index: u32) -> Result<Option<WasmVal>, JsValue> {
		let Some(a) = self.val.as_arr() else {
			return Ok(None);
		};
		self.run(|_| a.get(index))
			.map(|opt| opt.map(|v| self.child(v)))
			.map_err(|e| jrsonnet_js_error(&e))
	}
	pub fn obj_keys(&self) -> Option<Vec<String>> {
		self.val
			.as_obj()
			.map(|o| o.fields().into_iter().map(|s| s.to_string()).collect())
	}
	pub fn obj_get(&self, key: String) -> Result<Option<WasmVal>, JsValue> {
		let Some(o) = self.val.as_obj() else {
			return Ok(None);
		};
		self.run(|_| o.get(key.into()))
			.map(|opt| opt.map(|v| self.child(v)))
			.map_err(|e| jrsonnet_js_error(&e))
	}

	pub fn manifest_json(&self, indent: u32) -> Result<String, JsValue> {
		self.manifest_with(JsonFormat::cli(indent as usize))
	}
	pub fn manifest_to_string(&self) -> Result<String, JsValue> {
		self.manifest_with(ToStringFormat)
	}
	pub fn manifest_string(&self) -> Result<String, JsValue> {
		self.manifest_with(StringFormat)
	}
	pub fn manifest_yaml(&self, indent: u32, quote_keys: bool) -> Result<String, JsValue> {
		self.manifest_with(YamlFormat::std_to_yaml(indent != 0, quote_keys))
	}
	pub fn manifest_yaml_stream(
		&self,
		indent: u32,
		quote_keys: bool,
		c_document_end: bool,
	) -> Result<String, JsValue> {
		self.manifest_with(YamlStreamFormat::std_yaml_stream(
			YamlFormat::std_to_yaml(indent != 0, quote_keys),
			c_document_end,
		))
	}
	pub fn manifest_xml_jsonml(&self) -> Result<String, JsValue> {
		self.manifest_with(XmlJsonmlFormat::std_to_xml())
	}
	pub fn manifest_toml(&self, indent: u32) -> Result<String, JsValue> {
		self.manifest_with(TomlFormat::std_to_toml(" ".repeat(indent as usize)))
	}
	pub fn manifest_ini(&self) -> Result<String, JsValue> {
		self.manifest_with(IniFormat::std())
	}
}

#[derive(Trace)]
struct JsHandler {
	#[trace(skip)]
	func: js_sys::Function,
}

#[wasm_bindgen(inline_js = r"
export function js_invoke_val_callback(cb, args) {
	return cb.apply(null, args);
}
")]
extern "C" {
	#[wasm_bindgen(catch)]
	fn js_invoke_val_callback(
		cb: &js_sys::Function,
		args: &js_sys::Array,
	) -> Result<WasmVal, JsValue>;
}

impl NativeCallbackHandler for JsHandler {
	fn call(&self, args: &[Val]) -> JrResult<Val> {
		let js_args = js_sys::Array::new();
		let state = with_state(|s| s);
		for arg in args {
			js_args.push(&JsValue::from(WasmVal::with_state(
				arg.clone(),
				state.clone(),
			)));
		}
		let result = js_invoke_val_callback(&self.func, &js_args).map_err(|e| {
			let msg = e
				.as_string()
				.or_else(|| {
					e.dyn_ref::<js_sys::Error>()
						.map(|err| String::from(err.message()))
				})
				.unwrap_or_else(|| format!("{e:?}"));
			error!("js callback threw: {msg}")
		})?;
		Ok(result.val)
	}
}

#[wasm_bindgen]
pub struct WasmState {
	state: State,
	resolver: JsAsyncResolver,
}
#[wasm_bindgen]
impl WasmState {
	#[wasm_bindgen(constructor)]
	pub fn new(resolver: ImportResolverJs) -> Self {
		console_error_panic_hook::set_once();
		let mut state = StateBuilder::default();
		state.import_resolver(ResolvedImportResolver::new());
		let std = jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute);
		state.context_initializer(std);
		let state = state.build();
		Self {
			state,
			resolver: JsAsyncResolver { js: resolver },
		}
	}

	#[wasm_bindgen]
	pub fn evaluate_snippet(&self, name: &str, snippet: &str) -> Result<WasmVal, JsValue> {
		let _guard = self.state.enter();
		self.state
			.evaluate_snippet(name, snippet)
			.map(|v| WasmVal::with_state(v, self.state.clone()))
			.map_err(|e| jrsonnet_js_error(&e))
	}

	pub async fn evaluate_file(&self, path: String) -> Result<WasmVal, JsValue> {
		let path = async_import(self.state.clone(), self.resolver.clone(), &path.as_str()).await?;
		let _guard = self.state.enter();
		self.state
			.import_resolved(path)
			.map(|v| WasmVal::with_state(v, self.state.clone()))
			.map_err(|e| jrsonnet_js_error(&e))
	}
}

#[wasm_bindgen]
extern "C" {
	#[wasm_bindgen(typescript_type = "ImportResolver")]
	#[derive(Clone)]
	pub type ImportResolverJs;

	#[wasm_bindgen(catch, method, structural, js_name = resolveFrom)]
	fn resolve_from(
		this: &ImportResolverJs,
		from: Option<String>,
		path: &str,
	) -> Result<js_sys::Promise, JsValue>;

	#[wasm_bindgen(catch, method, structural, js_name = loadFileContents)]
	fn load_file_contents(
		this: &ImportResolverJs,
		resolved: &str,
	) -> Result<js_sys::Promise, JsValue>;
}

#[wasm_bindgen(typescript_custom_section)]
const TS_IMPORT_RESOLVER: &'static str = r"
export interface ImportResolver {
	resolveFrom(from: string | undefined, path: string): Promise<string>;
	loadFileContents(resolved: string): Promise<Uint8Array>;
}
";

#[derive(Clone)]
struct JsAsyncResolver {
	js: ImportResolverJs,
}

impl jrsonnet_evaluator::async_import::AsyncImportResolver for JsAsyncResolver {
	type Error = JsValue;

	async fn resolve_from(
		&self,
		from: &SourcePath,
		path: &dyn jrsonnet_evaluator::AsPathLike,
	) -> Result<SourcePath, JsValue> {
		let from_js = (!from.is_default()).then(|| from.to_string());
		let path_str = path.as_path().as_ref().to_string_lossy().into_owned();
		let promise = self.js.resolve_from(from_js, &path_str)?;
		let resolved_js = wasm_bindgen_futures::JsFuture::from(promise).await?;
		let resolved_str = resolved_js
			.as_string()
			.ok_or_else(|| JsValue::from_str("resolveFrom must return string"))?;
		let url = url::Url::parse(&resolved_str).map_err(|e| JsValue::from_str(&e.to_string()))?;
		Ok(SourcePath::new(SourceUrl::new(url)))
	}

	async fn load_file_contents(&self, resolved: &SourcePath) -> Result<Vec<u8>, JsValue> {
		let resolved_str = resolved.to_string();
		let promise = self.js.load_file_contents(&resolved_str)?;
		let bytes_js = wasm_bindgen_futures::JsFuture::from(promise).await?;
		let arr = bytes_js
			.dyn_into::<js_sys::Uint8Array>()
			.map_err(|_| JsValue::from_str("loadFileContents must return Uint8Array"))?;
		Ok(arr.to_vec())
	}
}

#[wasm_bindgen]
pub struct WasmFormatOptions {}
#[wasm_bindgen]
impl WasmFormatOptions {
	#[wasm_bindgen(constructor)]
	pub fn new() -> Self {
		Self {}
	}

	fn build(&self) -> FormatOptions {
		FormatOptions { indent: 0 }
	}
}

#[wasm_bindgen]
pub fn format(src: &str, opts: &WasmFormatOptions) -> Result<String, String> {
	match jrsonnet_formatter::format(src, &opts.build()) {
		Ok(v) => Ok(v),
		Err(e) => {
			let e = e.build();
			Err(hi_doc::source_to_ansi(&e))
		}
	}
}
