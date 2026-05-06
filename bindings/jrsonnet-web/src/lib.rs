#![allow(clippy::future_not_send, reason = "we work with js promises anyway")]

use std::{cell::RefCell, result::Result};

use jrsonnet_evaluator::{
	IStr, NumValue, ObjValue, Result as JrResult, SourcePath, SourceUrl, State, StateBuilder, Val,
	async_import::{ResolvedImportResolver, async_import},
	error,
	function::builtin::{NativeCallback, NativeCallbackHandler},
	manifest::{JsonFormat, ManifestFormat, StringFormat, ToStringFormat, YamlStreamFormat},
	tla::{TlaArg, apply_tla},
	trace::PathResolver,
	val::ArrValue,
	with_state,
};
use jrsonnet_formatter::FormatOptions;
use jrsonnet_gcmodule::Trace;
use jrsonnet_stdlib::{IniFormat, TomlFormat, XmlJsonmlFormat, YamlFormat};
use jrsonnet_types::ValType;
use js_sys::Reflect::get;
use rustc_hash::FxHashMap;
use wasm_bindgen::{convert::RefFromWasmAbi, prelude::*};

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
	BigInt,
}

thread_local! {
	static ERR_FACTORY: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}
#[wasm_bindgen(js_name = setErrorFactory)]
pub fn set_error_factory(f: js_sys::Function) {
	ERR_FACTORY.with(|c| *c.borrow_mut() = Some(f));
}
fn make_jrsonnet_error(message: &str, frames: js_sys::Array, cause: &JsValue) -> JsValue {
	ERR_FACTORY.with(|c| {
		c.borrow().as_ref().map_or_else(
			|| js_sys::Error::new(message).into(),
			|f| {
				let args = js_sys::Array::new();
				args.push(&JsValue::from_str(message));
				args.push(&frames);
				args.push(cause);
				f.apply(&JsValue::NULL, &args)
					.unwrap_or_else(|e| js_sys::Error::new(&format!("{e:?}")).into())
			},
		)
	})
}

fn js_error_message(e: &JsValue) -> String {
	e.dyn_ref::<js_sys::Error>().map_or_else(
		|| e.as_string().unwrap_or_else(|| format!("{e:?}")),
		|err| String::from(err.message()),
	)
}

fn unwrap_val_ref(value: &JsValue) -> Result<<WasmVal as RefFromWasmAbi>::Anchor, JsValue> {
	let ptr = get(value, &JsValue::from_str("__wbg_ptr"))
		.ok()
		.and_then(|v| v.as_f64())
		.ok_or_else(|| JsValue::from_str("expected a Val instance"))? as u32;
	if ptr == 0 {
		return Err(JsValue::from_str("Val has been freed"));
	}
	Ok(unsafe { <WasmVal as RefFromWasmAbi>::ref_from_abi(ptr) })
}

fn js_resolver_error(prefix: &str, e: JsValue) -> JsValue {
	let msg = format!("{prefix}: {}", js_error_message(&e));
	let frames = js_sys::Array::new();
	let frame = js_sys::Object::new();
	let _ = js_sys::Reflect::set(
		&frame,
		&JsValue::from_str("desc"),
		&JsValue::from_str(prefix),
	);
	frames.push(&frame);
	make_jrsonnet_error(&msg, frames, &e)
}

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
	make_jrsonnet_error(&msg, frames, &JsValue::UNDEFINED)
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
			#[cfg(feature = "exp-bigint")]
			ValType::BigInt => Self::BigInt,
		}
	}
}

#[wasm_bindgen(js_name = Val)]
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

#[wasm_bindgen(js_class = Val)]
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
	pub fn bigint(value: js_sys::BigInt) -> Result<Self, JsError> {
		#[cfg(feature = "exp-bigint")]
		{
			let s: String = value
				.to_string(10)
				.map_err(|_| JsError::new("invalid bigint"))?
				.into();
			let bi = s
				.parse::<num_bigint::BigInt>()
				.map_err(|e| JsError::new(&format!("failed to parse bigint: {e}")))?;
			Ok(Self::new(Val::BigInt(Box::new(bi))))
		}
		#[cfg(not(feature = "exp-bigint"))]
		{
			let _ = value;
			Err(JsError::new(
				"bigint support is not enabled in this build (exp-bigint feature)",
			))
		}
	}
	pub fn arr(items: Vec<WasmVal>) -> Self {
		Self::new(Val::arr(
			items.into_iter().map(|v| v.val).collect::<Vec<_>>(),
		))
	}
	pub fn func(
		params: Vec<String>,

		#[wasm_bindgen(unchecked_param_type = "(...args: Val[]) => Val")]
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
	#[wasm_bindgen(js_name = asBool)]
	pub fn as_bool(&self) -> Option<bool> {
		self.val.as_bool()
	}
	#[wasm_bindgen(js_name = asNum)]
	pub fn as_num(&self) -> Option<f64> {
		self.val.as_num()
	}
	#[wasm_bindgen(js_name = asBigint)]
	pub fn as_bigint(&self) -> Result<Option<js_sys::BigInt>, JsError> {
		#[cfg(feature = "exp-bigint")]
		{
			let Some(bi) = self.val.as_bigint() else {
				return Ok(None);
			};
			let big = js_sys::BigInt::new(&JsValue::from_str(&bi.to_string()))
				.map_err(|e| JsError::new(&format!("{e:?}")))?;
			Ok(Some(big))
		}
		#[cfg(not(feature = "exp-bigint"))]
		{
			Err(JsError::new(
				"bigint support is not enabled in this build (exp-bigint feature)",
			))
		}
	}
	#[wasm_bindgen(js_name = asString)]
	pub fn as_string(&self) -> Option<String> {
		self.val.as_str().map(|s| s.to_string())
	}
	#[wasm_bindgen(js_name = asArr)]
	pub fn as_arr(&self) -> Option<WasmArrValue> {
		self.val.as_arr().map(|arr| WasmArrValue {
			arr,
			state: self.state.clone(),
		})
	}
	#[wasm_bindgen(js_name = asObj)]
	pub fn as_obj(&self) -> Option<WasmObjValue> {
		self.val.as_obj().map(|obj| WasmObjValue {
			obj,
			state: self.state.clone(),
		})
	}

	#[wasm_bindgen(js_name = applyTla)]
	pub fn apply_tla(
		&self,
		#[wasm_bindgen(unchecked_param_type = "Record<string, Val>")] args: &js_sys::Object,
	) -> Result<WasmVal, JsValue> {
		let mut map: FxHashMap<IStr, TlaArg> = FxHashMap::default();
		for entry in js_sys::Object::entries(args).iter() {
			let pair: js_sys::Array = entry
				.dyn_into()
				.map_err(|_| JsValue::from_str("expected [key, value] entry"))?;
			let key = pair
				.get(0)
				.as_string()
				.ok_or_else(|| JsValue::from_str("TLA arg key must be a string"))?;
			let value = unwrap_val_ref(&pair.get(1))?;
			map.insert(key.into(), TlaArg::Val(value.val.clone()));
		}
		let val = self.val.clone();
		self.run(|_| apply_tla(&map, val))
			.map(|v| WasmVal {
				val: v,
				state: self.state.clone(),
			})
			.map_err(|e| jrsonnet_js_error(&e))
	}

	#[wasm_bindgen(js_name = manifestJson)]
	pub fn manifest_json(&self, indent: u32) -> Result<String, JsValue> {
		self.manifest_with(JsonFormat::cli(
			indent as usize,
			#[cfg(feature = "exp-preserve-order")]
			false,
		))
	}
	#[wasm_bindgen(js_name = manifestToString)]
	pub fn manifest_to_string(&self) -> Result<String, JsValue> {
		self.manifest_with(ToStringFormat)
	}
	#[wasm_bindgen(js_name = manifestString)]
	pub fn manifest_string(&self) -> Result<String, JsValue> {
		self.manifest_with(StringFormat)
	}
	#[wasm_bindgen(js_name = manifestYaml)]
	pub fn manifest_yaml(&self, indent: u32, quote_keys: bool) -> Result<String, JsValue> {
		self.manifest_with(YamlFormat::std_to_yaml(
			indent != 0,
			quote_keys,
			#[cfg(feature = "exp-preserve-order")]
			false,
		))
	}
	#[wasm_bindgen(js_name = manifestYamlStream)]
	pub fn manifest_yaml_stream(
		&self,
		indent: u32,
		quote_keys: bool,
		c_document_end: bool,
	) -> Result<String, JsValue> {
		self.manifest_with(YamlStreamFormat::std_yaml_stream(
			YamlFormat::std_to_yaml(
				indent != 0,
				quote_keys,
				#[cfg(feature = "exp-preserve-order")]
				false,
			),
			c_document_end,
		))
	}
	#[wasm_bindgen(js_name = manifestXmlJsonml)]
	pub fn manifest_xml_jsonml(&self) -> Result<String, JsValue> {
		self.manifest_with(XmlJsonmlFormat::std_to_xml())
	}
	#[wasm_bindgen(js_name = manifestToml)]
	pub fn manifest_toml(&self, indent: u32) -> Result<String, JsValue> {
		self.manifest_with(TomlFormat::std_to_toml(
			" ".repeat(indent as usize),
			#[cfg(feature = "exp-preserve-order")]
			false,
		))
	}
	#[wasm_bindgen(js_name = manifestIni)]
	pub fn manifest_ini(&self) -> Result<String, JsValue> {
		self.manifest_with(IniFormat::std(
			#[cfg(feature = "exp-preserve-order")]
			false,
		))
	}
}

#[wasm_bindgen(js_name = ArrValue)]
pub struct WasmArrValue {
	arr: ArrValue,
	state: Option<State>,
}

#[wasm_bindgen(js_class = ArrValue)]
impl WasmArrValue {
	#[wasm_bindgen(getter)]
	pub fn length(&self) -> u32 {
		self.arr.len()
	}
	pub fn at(&self, index: u32) -> Result<Option<WasmVal>, JsValue> {
		let result = self.state.as_ref().map_or_else(
			|| self.arr.get(index),
			|state| {
				let _guard = state.try_enter();
				self.arr.get(index)
			},
		);
		result
			.map(|opt: Option<Val>| {
				opt.map(|v| WasmVal {
					val: v,
					state: self.state.clone(),
				})
			})
			.map_err(|e| jrsonnet_js_error(&e))
	}
}

#[wasm_bindgen(js_name = ObjValue)]
pub struct WasmObjValue {
	obj: ObjValue,
	state: Option<State>,
}

#[wasm_bindgen(js_class = ObjValue)]
impl WasmObjValue {
	pub fn keys(&self) -> Vec<String> {
		self.obj
			.fields(
				#[cfg(feature = "exp-preserve-order")]
				false,
			)
			.into_iter()
			.map(|s| s.to_string())
			.collect()
	}
	pub fn get(&self, key: String) -> Result<Option<WasmVal>, JsValue> {
		let result = if let Some(state) = &self.state {
			let _guard = state.try_enter();
			self.obj.get(key.into())
		} else {
			self.obj.get(key.into())
		};
		result
			.map(|opt: Option<Val>| {
				opt.map(|v| WasmVal {
					val: v,
					state: self.state.clone(),
				})
			})
			.map_err(|e| jrsonnet_js_error(&e))
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

#[wasm_bindgen(js_name = State)]
pub struct WasmState {
	state: State,
	resolver: Option<JsAsyncResolver>,
}
#[wasm_bindgen(js_class = State)]
impl WasmState {
	#[wasm_bindgen(constructor)]
	pub fn new(resolver: Option<ImportResolverJs>) -> Self {
		console_error_panic_hook::set_once();
		let mut state = StateBuilder::default();
		state.import_resolver(ResolvedImportResolver::new());
		let std = jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute);
		state.context_initializer(std);
		let state = state.build();
		Self {
			state,
			resolver: resolver.map(|js| JsAsyncResolver { js }),
		}
	}

	#[wasm_bindgen(js_name = evaluateSnippet)]
	pub fn evaluate_snippet(&self, name: &str, snippet: &str) -> Result<WasmVal, JsValue> {
		let _guard = self.state.enter();
		self.state
			.evaluate_snippet(name, snippet)
			.map(|v| WasmVal::with_state(v, self.state.clone()))
			.map_err(|e| jrsonnet_js_error(&e))
	}

	#[wasm_bindgen(js_name = evaluateFile)]
	pub async fn evaluate_file(&self, path: String) -> Result<WasmVal, JsValue> {
		self.evaluate_file_from_impl(None, path).await
	}

	#[wasm_bindgen(js_name = evaluateFileFrom)]
	pub async fn evaluate_file_from(&self, from: String, path: String) -> Result<WasmVal, JsValue> {
		self.evaluate_file_from_impl(Some(from), path).await
	}
}

impl WasmState {
	async fn evaluate_file_from_impl(
		&self,
		from: Option<String>,
		path: String,
	) -> Result<WasmVal, JsValue> {
		let resolver = self
			.resolver
			.clone()
			.ok_or_else(|| JsValue::from_str("file evaluation requires an ImportResolver"))?;
		let from = match from {
			Some(s) => {
				let url = url::Url::parse(&s).map_err(|e| JsValue::from_str(&e.to_string()))?;
				SourcePath::new(SourceUrl::new(url))
			}
			None => SourcePath::default(),
		};
		let path = async_import(self.state.clone(), resolver, &from, &path.as_str()).await?;
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
		let promise = self
			.js
			.resolve_from(from_js, &path_str)
			.map_err(|e| js_resolver_error("resolveFrom", e))?;
		let resolved_js = wasm_bindgen_futures::JsFuture::from(promise)
			.await
			.map_err(|e| js_resolver_error("resolveFrom", e))?;
		let resolved_str = resolved_js
			.as_string()
			.ok_or_else(|| JsValue::from_str("resolveFrom must return string"))?;
		let url = url::Url::parse(&resolved_str).map_err(|e| JsValue::from_str(&e.to_string()))?;
		Ok(SourcePath::new(SourceUrl::new(url)))
	}

	async fn load_file_contents(&self, resolved: &SourcePath) -> Result<Vec<u8>, JsValue> {
		let resolved_str = resolved.to_string();
		let promise = self
			.js
			.load_file_contents(&resolved_str)
			.map_err(|e| js_resolver_error("loadFileContents", e))?;
		let bytes_js = wasm_bindgen_futures::JsFuture::from(promise)
			.await
			.map_err(|e| js_resolver_error("loadFileContents", e))?;
		let arr = bytes_js
			.dyn_into::<js_sys::Uint8Array>()
			.map_err(|_| JsValue::from_str("loadFileContents must return Uint8Array"))?;
		Ok(arr.to_vec())
	}
}

#[wasm_bindgen(js_name = FormatOptions)]
pub struct WasmFormatOptions {
	indent: u8,
	use_tabs: bool,
	max_width: u32,
}
#[wasm_bindgen(js_class = FormatOptions)]
impl WasmFormatOptions {
	#[wasm_bindgen(constructor)]
	pub fn new() -> Self {
		Self {
			indent: 4,
			use_tabs: true,
			max_width: 100,
		}
	}

	fn build(&self) -> FormatOptions {
		FormatOptions {
			indent: self.indent,
			use_tabs: self.use_tabs,
			max_width: self.max_width,
		}
	}
}

impl Default for WasmFormatOptions {
	fn default() -> Self {
		Self::new()
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
