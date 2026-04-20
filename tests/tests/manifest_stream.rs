use jrsonnet_evaluator::{
	FileImportResolver, State,
	manifest::{JsonFormat, ManifestFormat, YamlStreamFormat},
	trace::PathResolver,
};
use jrsonnet_stdlib::ContextInitializer;

fn eval(code: &str) -> jrsonnet_evaluator::Val {
	let mut s = State::builder();
	s.context_initializer(ContextInitializer::new(PathResolver::new_cwd_fallback()))
		.import_resolver(FileImportResolver::default());
	let s = s.build();
	let _entered = s.enter();
	s.evaluate_snippet("<test>".to_owned(), code).unwrap()
}

fn yaml_stream_format() -> YamlStreamFormat<JsonFormat<'static>> {
	YamlStreamFormat::cli(JsonFormat::default())
}

#[test]
fn manifest_write_matches_manifest_buf() {
	let cases = &[
		"[{a: 1}, {b: 2}, {c: 3}]",
		"[1, 2, 3]",
		"[{nested: {x: [1,2,3]}}, {other: null}]",
		"['hello', 'world']",
		"[true, false, null]",
		"std.makeArray(100, function(i) {index: i})",
	];

	let format = yaml_stream_format();

	for code in cases {
		let val = eval(code);

		let buffered = format.manifest(val.clone()).unwrap();

		let mut streamed = Vec::new();
		format.manifest_write(val, &mut streamed).unwrap();
		let streamed = String::from_utf8(streamed).unwrap();

		assert_eq!(
			buffered, streamed,
			"manifest_write output differs from manifest_buf for: {code}"
		);
	}
}

#[test]
fn manifest_write_empty_array() {
	let val = eval("[]");
	let format = yaml_stream_format();

	let buffered = format.manifest(val.clone()).unwrap();

	let mut streamed = Vec::new();
	let wrote = format.manifest_write(val, &mut streamed).unwrap();
	let streamed = String::from_utf8(streamed).unwrap();

	assert!(wrote);
	assert_eq!(buffered, streamed);
	assert_eq!(streamed, "\n...");
}

#[test]
fn manifest_write_single_element() {
	let val = eval("[{key: 'value'}]");
	let format = yaml_stream_format();

	let mut streamed = Vec::new();
	format.manifest_write(val, &mut streamed).unwrap();
	let output = String::from_utf8(streamed).unwrap();

	assert!(output.starts_with("---\n"));
	assert!(output.ends_with("\n..."));
	assert!(output.contains("\"key\": \"value\""));
}

#[test]
fn manifest_write_streams_incrementally() {
	use std::io::Write;
	use std::sync::{Arc, Mutex};

	#[derive(Clone)]
	struct ChunkRecorder {
		chunks: Arc<Mutex<Vec<String>>>,
	}
	impl ChunkRecorder {
		fn new() -> Self {
			Self {
				chunks: Arc::new(Mutex::new(Vec::new())),
			}
		}
		fn chunks(&self) -> Vec<String> {
			self.chunks.lock().unwrap().clone()
		}
	}
	impl Write for ChunkRecorder {
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			self.chunks
				.lock()
				.unwrap()
				.push(String::from_utf8_lossy(buf).to_string());
			Ok(buf.len())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}

	let val = eval("[{a: 1}, {b: 2}, {c: 3}]");
	let format = yaml_stream_format();

	let mut recorder = ChunkRecorder::new();
	format.manifest_write(val, &mut recorder).unwrap();

	let chunks = recorder.chunks();
	// Should have at least 4 writes: 3 elements + document end marker
	assert!(
		chunks.len() >= 4,
		"expected at least 4 write calls for 3 elements + end marker, got {}",
		chunks.len()
	);
	// First chunk should start with the first document
	assert!(chunks[0].starts_with("---\n"));
	// Last chunk should be the document end marker
	assert_eq!(chunks.last().unwrap(), "\n...");
}

#[test]
fn manifest_write_rejects_non_array() {
	let val = eval("{a: 1}");
	let format = yaml_stream_format();

	let mut out = Vec::new();
	let result = format.manifest_write(val, &mut out);
	assert!(result.is_err());
}

#[test]
fn manifest_write_with_std_yaml_stream_format() {
	let val = eval("[{a: 1, b: [2, 3]}, {c: 'hello'}]");
	let format = YamlStreamFormat::std_yaml_stream(JsonFormat::default(), true);

	let buffered = format.manifest(val.clone()).unwrap();

	let mut streamed = Vec::new();
	format.manifest_write(val, &mut streamed).unwrap();
	let streamed = String::from_utf8(streamed).unwrap();

	assert_eq!(buffered, streamed);
}

#[test]
fn default_manifest_write_empty_output() {
	use jrsonnet_evaluator::manifest::StringFormat;

	let val = eval("''");

	let mut out = Vec::new();
	let wrote = StringFormat.manifest_write(val, &mut out).unwrap();

	assert!(!wrote);
	assert!(out.is_empty());
}
