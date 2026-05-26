use std::{
	collections::{BTreeMap, btree_map::Entry},
	process::exit,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Parser, ValueEnum};
use jrsonnet_cli::MiscOpts;
use jrsonnet_evaluator::{FileImportResolver, ImportResolver};
use jrsonnet_ir::{IStr, Source, SourcePath, visit::Visitor};
use jrsonnet_ir_parser::ParserSettings;
use jrsonnet_pack::{bundle_onefile, bundle_playground};

#[derive(Parser)]
struct Opts {
	/// Path to the file to start dependency search from
	input: String,
	/// Bundle the entry and its imports.
	#[clap(long, value_enum, num_args = 0..=1, default_missing_value = "json")]
	bundle: Option<BundleMode>,
	/// Output format for the bundled playground (e.g. json, yaml, toml, string)
	#[clap(long)]
	bundle_format: Option<String>,
	#[clap(flatten)]
	misc: MiscOpts,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum BundleMode {
	/// Plain playground bundle JSON.
	Json,
	/// Playground bundle as a shareable URL.
	Url,
	/// Single self-contained jsonnet expression.
	Onefile,
}

struct FoundImports(Vec<(IStr, bool)>);
impl Visitor for FoundImports {
	fn visit_import(&mut self, expression: bool, value: IStr) {
		self.0.push((value, expression));
	}
}

fn collect_deps(
	resolver: &FileImportResolver,
	source: &SourcePath,
	deps: &mut BTreeMap<String, SourcePath>,
) -> Result<(), String> {
	let contents = resolver
		.load_file_contents(source)
		.map_err(|e| format!("{e}"))?;
	let code = std::str::from_utf8(&contents).map_err(|e| format!("{source}: {e}"))?;
	let code: IStr = code.into();
	let parsed = jrsonnet_ir_parser::parse(
		&code,
		&ParserSettings {
			source: Source::new(source.clone(), code.clone()),
		},
	)
	.map_err(|e| format!("{source}: {e}"))?;

	let mut imports = FoundImports(vec![]);
	imports.visit_expr(&parsed);

	for (path, expression) in imports.0 {
		let resolved = resolver
			.resolve_from(source, &&*path)
			.map_err(|e| format!("{e}"))?;
		let key = format!("{resolved}");
		if let Entry::Vacant(e) = deps.entry(key) {
			e.insert(resolved.clone());
			if expression {
				collect_deps(resolver, &resolved, deps)?;
			}
		}
	}

	Ok(())
}

fn main() {
	let opts = Opts::parse();
	let resolver = opts.misc.import_resolver();

	let source = resolver
		.resolve_from_default(&opts.input.as_str())
		.unwrap_or_else(|e| {
			eprintln!("{e}");
			exit(1);
		});

	if let Some(mode) = opts.bundle {
		match mode {
			BundleMode::Json => {
				let b =
					bundle_playground(&resolver, &source, opts.bundle_format).unwrap_or_else(|e| {
						eprintln!("{e}");
						exit(1);
					});
				let json = serde_json::to_string_pretty(&b).expect("bundle serialization");
				println!("{json}");
			}
			BundleMode::Url => {
				let b =
					bundle_playground(&resolver, &source, opts.bundle_format).unwrap_or_else(|e| {
						eprintln!("{e}");
						exit(1);
					});
				let json = serde_json::to_string(&b).expect("bundle serialization");
				let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
				println!("https://delta.rocks/jrsonnet/playground?state={encoded}");
			}
			BundleMode::Onefile => {
				let src = bundle_onefile(&resolver, &source).unwrap_or_else(|e| {
					eprintln!("{e}");
					exit(1);
				});
				print!("{src}");
			}
		}
	} else {
		let mut deps = BTreeMap::new();
		if let Err(e) = collect_deps(&resolver, &source, &mut deps) {
			eprintln!("{e}");
			exit(1);
		}
		for dep in deps.keys() {
			println!("{dep}");
		}
	}
}
