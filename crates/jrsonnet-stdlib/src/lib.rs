#![allow(clippy::similar_names)]

use std::{
	cell::{Ref, RefCell, RefMut},
	collections::HashMap,
	f64,
	rc::Rc,
};

pub use arrays::*;
pub use compat::*;
pub use encoding::*;
pub use hash::*;
use jrsonnet_evaluator::{
	IStr, InitialContextBuilder, NumValue, ObjValue, ObjValueBuilder, Source, Thunk, Val, error::Result, function::{CallLocation, FuncVal, builtin_id}, tla::TlaArg, trace::PathResolver, typed::SerializeTypedObj as _
};
use jrsonnet_gcmodule::{Acyclic, Cc, Trace};
use jrsonnet_macros::{IntoUntyped, Typed};
pub use manifest::*;
pub use math::*;
pub use misc::*;
pub use objects::*;
pub use operator::*;
pub use parse::*;
pub use sets::*;
pub use sort::*;
pub use strings::*;
pub use types::*;

#[cfg(feature = "exp-regex")]
pub use crate::regex::*;

mod arrays;
mod compat;
mod encoding;
mod hash;
mod keyf;
mod manifest;
mod math;
mod misc;
mod objects;
mod operator;
mod parse;
#[cfg(feature = "exp-regex")]
mod regex;
mod sets;
mod sort;
mod strings;
mod types;

#[derive(Typed, IntoUntyped, Default)]
#[allow(non_snake_case)]
struct Builtins {
	#[typed(method)]
	id: builtin_id,
	// Types
	#[typed(method, rename = "type")]
	r#type: builtin_type,
	#[typed(method)]
	isString: builtin_is_string,
	#[typed(method)]
	isNumber: builtin_is_number,
	#[typed(method)]
	isBoolean: builtin_is_boolean,
	#[typed(method)]
	isObject: builtin_is_object,
	#[typed(method)]
	isArray: builtin_is_array,
	#[typed(method)]
	isFunction: builtin_is_function,
	#[typed(method)]
	isNull: builtin_is_null,
	// Arrays
	#[typed(method)]
	makeArray: builtin_make_array,
	#[typed(method)]
	repeat: builtin_repeat,
	#[typed(method)]
	slice: builtin_slice,
	#[typed(method)]
	map: builtin_map,
	#[typed(method)]
	mapWithIndex: builtin_map_with_index,
	#[typed(method)]
	mapWithKey: builtin_map_with_key,
	#[typed(method)]
	flatMap: builtin_flatmap,
	#[typed(method)]
	filter: builtin_filter,
	#[typed(method)]
	foldl: builtin_foldl,
	#[typed(method)]
	foldr: builtin_foldr,
	#[typed(method)]
	range: builtin_range,
	#[typed(method)]
	join: builtin_join,
	#[typed(method)]
	lines: builtin_lines,
	#[typed(method)]
	resolvePath: builtin_resolve_path,
	#[typed(method)]
	deepJoin: builtin_deep_join,
	#[typed(method)]
	reverse: builtin_reverse,
	#[typed(method)]
	any: builtin_any,
	#[typed(method)]
	all: builtin_all,
	#[typed(method)]
	member: builtin_member,
	#[typed(method)]
	find: builtin_find,
	#[typed(method)]
	contains: builtin_contains,
	#[typed(method)]
	count: builtin_count,
	#[typed(method)]
	avg: builtin_avg,
	#[typed(method)]
	removeAt: builtin_remove_at,
	#[typed(method)]
	remove: builtin_remove,
	#[typed(method)]
	flattenArrays: builtin_flatten_arrays,
	#[typed(method)]
	flattenDeepArray: builtin_flatten_deep_array,
	#[typed(method)]
	prune: builtin_prune,
	#[typed(method)]
	filterMap: builtin_filter_map,
	// Math
	#[typed(method)]
	abs: builtin_abs,
	#[typed(method)]
	sign: builtin_sign,
	#[typed(method)]
	max: builtin_max,
	#[typed(method)]
	min: builtin_min,
	#[typed(method)]
	clamp: builtin_clamp,
	#[typed(method)]
	sum: builtin_sum,
	#[typed(method)]
	modulo: builtin_modulo,
	#[typed(method)]
	floor: builtin_floor,
	#[typed(method)]
	ceil: builtin_ceil,
	#[typed(method)]
	log: builtin_log,
	#[typed(method)]
	log2: builtin_log2,
	#[typed(method)]
	log10: builtin_log10,
	#[typed(method)]
	pow: builtin_pow,
	#[typed(method)]
	sqrt: builtin_sqrt,
	#[typed(method)]
	sin: builtin_sin,
	#[typed(method)]
	cos: builtin_cos,
	#[typed(method)]
	tan: builtin_tan,
	#[typed(method)]
	asin: builtin_asin,
	#[typed(method)]
	acos: builtin_acos,
	#[typed(method)]
	atan: builtin_atan,
	#[typed(method)]
	atan2: builtin_atan2,
	#[typed(method)]
	exp: builtin_exp,
	#[typed(method)]
	mantissa: builtin_mantissa,
	#[typed(method)]
	exponent: builtin_exponent,
	#[typed(method)]
	round: builtin_round,
	#[typed(method)]
	isEven: builtin_is_even,
	#[typed(method)]
	isOdd: builtin_is_odd,
	#[typed(method)]
	isInteger: builtin_is_integer,
	#[typed(method)]
	isDecimal: builtin_is_decimal,
	#[typed(method)]
	deg2rad: builtin_deg2rad,
	#[typed(method)]
	rad2deg: builtin_rad2deg,
	#[typed(method)]
	hypot: builtin_hypot,
	// Operator
	#[typed(rename = "mod", method)]
	r#mod: builtin_mod,
	#[typed(method)]
	primitiveEquals: builtin_primitive_equals,
	#[typed(method)]
	equals: builtin_equals,
	#[typed(method)]
	xor: builtin_xor,
	#[typed(method)]
	xnor: builtin_xnor,
	#[typed(method)]
	format: builtin_format,
	// Sort
	#[typed(method)]
	sort: builtin_sort,
	#[typed(method)]
	uniq: builtin_uniq,
	#[typed(method)]
	set: builtin_set,
	#[typed(method)]
	minArray: builtin_min_array,
	#[typed(method)]
	maxArray: builtin_max_array,
	// Hash
	#[typed(method)]
	md5: builtin_md5,
	#[typed(method)]
	sha1: builtin_sha1,
	#[typed(method)]
	sha256: builtin_sha256,
	#[typed(method)]
	sha512: builtin_sha512,
	#[typed(method)]
	sha3: builtin_sha3,
	// Encoding
	#[typed(method)]
	encodeUTF8: builtin_encode_utf8,
	#[typed(method)]
	decodeUTF8: builtin_decode_utf8,
	#[typed(method)]
	base64: builtin_base64,
	#[typed(method)]
	base64Decode: builtin_base64_decode,
	#[typed(method)]
	base64DecodeBytes: builtin_base64_decode_bytes,
	// Objects
	#[typed(method)]
	objectFieldsEx: builtin_object_fields_ex,
	#[typed(method)]
	objectFields: builtin_object_fields,
	#[typed(method)]
	objectFieldsAll: builtin_object_fields_all,
	#[typed(method)]
	objectValues: builtin_object_values,
	#[typed(method)]
	objectValuesAll: builtin_object_values_all,
	#[typed(method)]
	objectKeysValues: builtin_object_keys_values,
	#[typed(method)]
	objectKeysValuesAll: builtin_object_keys_values_all,
	#[typed(method)]
	objectHasEx: builtin_object_has_ex,
	#[typed(method)]
	objectHas: builtin_object_has,
	#[typed(method)]
	objectHasAll: builtin_object_has_all,
	#[typed(method)]
	objectRemoveKey: builtin_object_remove_key,
	// Manifest
	#[typed(method)]
	escapeStringJson: builtin_escape_string_json,
	#[typed(method)]
	escapeStringPython: builtin_escape_string_python,
	#[typed(method)]
	escapeStringXML: builtin_escape_string_xml,
	#[typed(method)]
	manifestJsonEx: builtin_manifest_json_ex,
	#[typed(method)]
	manifestJson: builtin_manifest_json,
	#[typed(method)]
	manifestJsonMinified: builtin_manifest_json_minified,
	#[typed(method)]
	manifestYamlDoc: builtin_manifest_yaml_doc,
	#[typed(method)]
	manifestYamlStream: builtin_manifest_yaml_stream,
	#[typed(method)]
	manifestTomlEx: builtin_manifest_toml_ex,
	#[typed(method)]
	manifestToml: builtin_manifest_toml,
	#[typed(method)]
	toString: builtin_to_string,
	#[typed(method)]
	manifestPython: builtin_manifest_python,
	#[typed(method)]
	manifestPythonVars: builtin_manifest_python_vars,
	#[typed(method)]
	manifestXmlJsonml: builtin_manifest_xml_jsonml,
	#[typed(method)]
	manifestIni: builtin_manifest_ini,
	// Parse
	#[typed(method)]
	parseJson: builtin_parse_json,
	#[typed(method)]
	parseYaml: builtin_parse_yaml,
	// Strings
	#[typed(method)]
	codepoint: builtin_codepoint,
	#[typed(method)]
	substr: builtin_substr,
	#[typed(method)]
	char: builtin_char,
	#[typed(method)]
	strReplace: builtin_str_replace,
	#[typed(method)]
	escapeStringBash: builtin_escape_string_bash,
	#[typed(method)]
	escapeStringDollars: builtin_escape_string_dollars,
	#[typed(method)]
	isEmpty: builtin_is_empty,
	#[typed(method)]
	equalsIgnoreCase: builtin_equals_ignore_case,
	#[typed(method)]
	splitLimit: builtin_splitlimit,
	#[typed(method)]
	splitLimitR: builtin_splitlimitr,
	#[typed(method)]
	split: builtin_split,
	#[typed(method)]
	asciiUpper: builtin_ascii_upper,
	#[typed(method)]
	asciiLower: builtin_ascii_lower,
	#[typed(method)]
	findSubstr: builtin_find_substr,
	#[typed(method)]
	parseInt: builtin_parse_int,
	#[cfg(feature = "exp-bigint")]
	#[typed(method)]
	bigint: builtin_bigint,
	#[typed(method)]
	parseOctal: builtin_parse_octal,
	#[typed(method)]
	parseHex: builtin_parse_hex,
	#[typed(method)]
	stringChars: builtin_string_chars,
	#[typed(method)]
	lstripChars: builtin_lstrip_chars,
	#[typed(method)]
	rstripChars: builtin_rstrip_chars,
	#[typed(method)]
	stripChars: builtin_strip_chars,
	#[typed(method)]
	trim: builtin_trim,
	// Misc
	#[typed(method)]
	length: builtin_length,
	#[typed(method)]
	get: builtin_get,
	#[typed(method)]
	startsWith: builtin_starts_with,
	#[typed(method)]
	endsWith: builtin_ends_with,
	#[typed(method)]
	assertEqual: builtin_assert_equal,
	#[typed(method)]
	mergePatch: builtin_merge_patch,
	// Sets
	#[typed(method)]
	setMember: builtin_set_member,
	#[typed(method)]
	setInter: builtin_set_inter,
	#[typed(method)]
	setDiff: builtin_set_diff,
	#[typed(method)]
	setUnion: builtin_set_union,
	// Regex
	#[cfg(feature = "exp-regex")]
	#[typed(method)]
	regexQuoteMeta: builtin_regex_quote_meta,
	// Compat
	#[typed(method)]
	__compare: builtin___compare,
	#[typed(method)]
	__compare_array: builtin___compare_array,
	#[typed(method)]
	__array_less: builtin___array_less,
	#[typed(method)]
	__array_greater: builtin___array_greater,
	#[typed(method)]
	__array_less_or_equal: builtin___array_less_or_equal,
	#[typed(method)]
	__array_greater_or_equal: builtin___array_greater_or_equal,
}

#[allow(clippy::too_many_lines)]
pub fn stdlib_uncached(settings: Cc<RefCell<Settings>>) -> ObjValue {
	let mut builder = ObjValueBuilder::new();

	let builtins = Builtins::default();
	builtins.serialize(&mut builder).expect("no conflicts");

	builder.method(
		"extVar",
		builtin_ext_var {
			settings: settings.clone(),
		},
	);
	builder.method(
		"native",
		builtin_native {
			settings: settings.clone(),
		},
	);
	builder.method("trace", builtin_trace { settings });

	builder.field("pi").hide().value(Val::Num(
		NumValue::new(f64::consts::PI).expect("pi is finite"),
	));

	#[cfg(feature = "exp-regex")]
	{
		// Regex
		let regex_cache = RegexCache::default();
		builder.method(
			"regexFullMatch",
			builtin_regex_full_match {
				cache: regex_cache.clone(),
			},
		);
		builder.method(
			"regexPartialMatch",
			builtin_regex_partial_match {
				cache: regex_cache.clone(),
			},
		);
		builder.method(
			"regexReplace",
			builtin_regex_replace {
				cache: regex_cache.clone(),
			},
		);
		builder.method(
			"regexGlobalReplace",
			builtin_regex_global_replace { cache: regex_cache },
		);
	};

	builder.build()
}

pub trait TracePrinter: Acyclic {
	fn print_trace(&self, loc: CallLocation, value: IStr);
}

#[derive(Acyclic)]
pub struct StdTracePrinter {
	resolver: PathResolver,
}
impl StdTracePrinter {
	pub fn new(resolver: PathResolver) -> Self {
		Self { resolver }
	}
}
impl TracePrinter for StdTracePrinter {
	fn print_trace(&self, loc: CallLocation, value: IStr) {
		eprint!("TRACE:");
		if let Some(loc) = loc.0 {
			let locs = loc.0.map_source_locations(&[loc.1]);
			eprint!(
				" {}:{}",
				loc.0.source_path().path().map_or_else(
					|| loc.0.source_path().to_string(),
					|p| self.resolver.resolve(p)
				),
				locs[0].line
			);
		}
		eprintln!(" {value}");
	}
}

#[derive(Clone, Trace)]
pub struct Settings {
	/// Used for `std.extVar`
	pub ext_vars: HashMap<IStr, TlaArg>,
	/// Used for `std.native`
	pub ext_natives: HashMap<IStr, FuncVal>,
	/// Used for `std.trace`
	pub trace_printer: Rc<dyn TracePrinter>,
	/// Used for `std.thisFile`
	pub path_resolver: PathResolver,
}

#[derive(Trace, Clone)]
pub struct ContextInitializer {
	/// std without applied thisFile overlay
	stdlib_obj: ObjValue,
	settings: Cc<RefCell<Settings>>,
}
impl ContextInitializer {
	pub fn new(resolver: PathResolver) -> Self {
		let settings = Settings {
			ext_vars: HashMap::new(),
			ext_natives: HashMap::new(),
			trace_printer: Rc::new(StdTracePrinter::new(resolver.clone())),
			path_resolver: resolver,
		};
		let settings = Cc::new(RefCell::new(settings));
		let stdlib_obj = stdlib_uncached(settings.clone());
		Self {
			stdlib_obj,
			settings,
		}
	}
	pub fn settings(&self) -> Ref<'_, Settings> {
		self.settings.borrow()
	}
	pub fn settings_mut(&self) -> RefMut<'_, Settings> {
		self.settings.borrow_mut()
	}
	pub fn add_ext_var(&self, name: IStr, value: Val) {
		self.settings_mut()
			.ext_vars
			.insert(name, TlaArg::Val(value));
	}
	pub fn add_ext_str(&self, name: IStr, value: IStr) {
		self.settings_mut()
			.ext_vars
			.insert(name, TlaArg::String(value));
	}
	pub fn add_ext_code(&self, name: &str, code: impl AsRef<str>) -> Result<()> {
		// self.data_mut().volatile_files.insert(source_name, code);
		self.settings_mut()
			.ext_vars
			.insert(name.into(), TlaArg::InlineCode(code.as_ref().to_owned()));
		Ok(())
	}
	pub fn add_native(&self, name: impl Into<IStr>, cb: impl Into<FuncVal>) {
		self.settings_mut()
			.ext_natives
			.insert(name.into(), cb.into());
	}
}
impl jrsonnet_evaluator::ContextInitializer for ContextInitializer {
	fn populate(&self, source: Source, builder: &mut InitialContextBuilder) {
		let mut std = ObjValueBuilder::new();
		std.with_super(self.stdlib_obj.clone());
		std.field("thisFile").hide().value({
			let source_path = source.source_path();
			source_path.path().map_or_else(
				|| source_path.to_string(),
				|p| self.settings().path_resolver.resolve(p),
			)
		});
		let stdlib_with_this_file = std.build();

		builder.bind("std", Thunk::evaluated(Val::Obj(stdlib_with_this_file)));
	}
	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
}
