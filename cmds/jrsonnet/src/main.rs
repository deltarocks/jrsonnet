use std::{
	fs::{File, create_dir_all},
	io::{Read, Write},
};

use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use jrsonnet_cli::{GcOpts, ManifestOpts, MiscOpts, OutputOpts, StdOpts, TlaOpts, TraceOpts};
use jrsonnet_evaluator::{
	ResultExt, State, Val, apply_tla, bail,
	error::{Error as JrError, ErrorKind},
};
use jrsonnet_ir::{SourceDefaultIgnoreJpath, SourcePath};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimallocator::Mimalloc = mimallocator::Mimalloc;

#[derive(Parser)]
enum SubOpts {
	/// Generate completions for specified shell
	Generate {
		/// Target shell name
		shell: Shell,
	},
}

#[derive(Parser)]
#[clap(next_help_heading = "DEBUG")]
struct DebugOpts {
	/// Required OS stack size.
	/// This shouldn't be changed unless jrsonnet is failing with stack overflow error.
	#[clap(long, name = "size")]
	pub os_stack: Option<usize>,
}

#[derive(Parser)]
#[clap(next_help_heading = "INPUT")]
struct InputOpts {
	/// Treat input as code, evaluate it instead of reading file.
	#[clap(long, short = 'e')]
	pub exec: bool,

	/// Path to the file to be compiled if `--exec` is unset, otherwise code itself.
	pub input: Option<String>,

	/// After executing input, apply specified code.
	/// Output of the initial input will be accessible using `_`.
	#[cfg(feature = "exp-apply")]
	#[clap(long)]
	pub exp_apply: Vec<String>,
}

/// Jsonnet commandline interpreter (Rust implementation)
#[derive(Parser)]
#[clap(
	args_conflicts_with_subcommands = true,
	disable_version_flag = true,
	version,
	author
)]
struct Opts {
	#[clap(subcommand)]
	sub: Option<SubOpts>,
	/// Print version
	#[clap(long)]
	version: bool,

	#[clap(flatten)]
	input: InputOpts,
	#[clap(flatten)]
	misc: MiscOpts,
	#[clap(flatten)]
	tla: TlaOpts,
	#[clap(flatten)]
	std: StdOpts,
	#[clap(flatten)]
	gc: GcOpts,

	#[clap(flatten)]
	trace: TraceOpts,
	#[clap(flatten)]
	manifest: ManifestOpts,
	#[clap(flatten)]
	output: OutputOpts,
	#[clap(flatten)]
	debug: DebugOpts,
}

// TODO: Add unix_sigpipe = "sig_dfl"
fn main() {
	let opts: Opts = Opts::parse();

	if opts.version {
		print!("{}", Opts::command().render_version());
		std::process::exit(0)
	}

	if let Some(sub) = opts.sub {
		match sub {
			SubOpts::Generate { shell } => {
				use clap_complete::generate;
				let app = &mut Opts::command();
				let buf = &mut std::io::stdout();
				generate(shell, app, "jrsonnet", buf);
				std::process::exit(0)
			}
		}
	}

	let success = if let Some(size) = opts.debug.os_stack {
		std::thread::Builder::new()
			.stack_size(size * 1024 * 1024)
			.spawn(|| main_catch(opts))
			.expect("new thread spawned")
			.join()
			.expect("thread finished successfully")
	} else {
		main_catch(opts)
	};
	if !success {
		std::process::exit(1);
	}
}

#[derive(thiserror::Error, Debug)]
enum Error {
	// Handled differently
	#[error("evaluation error")]
	Evaluation(JrError),
	#[error("io error")]
	Io(#[from] std::io::Error),
	#[error("input is not utf8 encoded")]
	Utf8(#[from] std::str::Utf8Error),
	#[error("missing input argument")]
	MissingInputArgument,
}
impl From<JrError> for Error {
	fn from(e: JrError) -> Self {
		Self::Evaluation(e)
	}
}
impl From<ErrorKind> for Error {
	fn from(e: ErrorKind) -> Self {
		Self::from(JrError::from(e))
	}
}

fn main_catch(opts: Opts) -> bool {
	let trace = opts.trace.trace_format();
	if let Err(e) = main_real(opts) {
		if let Error::Evaluation(e) = e {
			let mut out = String::new();
			trace.write_trace(&mut out, &e).expect("format error");
			eprintln!("{out}");
		} else {
			eprintln!("{e}");
		}
		return false;
	}
	true
}

fn main_real(opts: Opts) -> Result<(), Error> {
	let _gc_leak_guard = opts.gc.leak_on_exit();
	let _gc_print_stats = opts.gc.stats_printer();
	let _stack_depth_override = opts.misc.stack_size_override();

	let import_resolver = opts.misc.import_resolver();
	let std = opts.std.context_initializer()?;

	let mut s = State::builder();
	s.import_resolver(import_resolver).context_initializer(std);
	let s = s.build();
	let _s = s.enter();

	let input = opts.input.input.ok_or(Error::MissingInputArgument)?;
	let val = if opts.input.exec {
		s.evaluate_snippet("<cmdline>".to_owned(), &input as &str)?
	} else if input == "-" {
		let mut input = Vec::new();
		std::io::stdin().read_to_end(&mut input)?;
		let input_str = std::str::from_utf8(&input)?;
		s.evaluate_snippet("<stdin>".to_owned(), input_str)?
	} else {
		s.import_from(&SourcePath::new(SourceDefaultIgnoreJpath), input.as_str())?
	};

	let tla = opts.tla.tla_opts()?;
	#[allow(
		// It is not redundant/unused in exp-apply
		unused_mut,
		clippy::redundant_clone,
	)]
	let mut val = apply_tla(&tla, val)?;

	#[cfg(feature = "exp-apply")]
	for apply in opts.input.exp_apply {
		use jrsonnet_evaluator::{InitialUnderscore, Thunk};
		val = s.evaluate_snippet_with(
			"<exp_apply>".to_owned(),
			&apply,
			&InitialUnderscore(Thunk::evaluated(val)),
		)?;
	}

	let manifest_format = opts.manifest.manifest_format();
	if let Some(multi) = opts.output.multi {
		if opts.output.create_output_dirs {
			let mut dir = multi.clone();
			dir.pop();
			create_dir_all(dir)?;
		}
		let Val::Obj(obj) = val else {
			bail!(
				"value should be object for --multi manifest, got {}",
				val.value_type()
			)
		};
		for (field, data) in obj.iter(
			#[cfg(feature = "exp-preserve-order")]
			opts.manifest.preserve_order,
		) {
			let data = data.with_description(|| format!("getting field {field} for manifest"))?;

			let mut path = multi.clone();
			path.push(&field as &str);
			if opts.output.create_output_dirs {
				let mut dir = path.clone();
				dir.pop();
				create_dir_all(dir)?;
			}
			println!("{}", path.to_str().expect("path"));
			let mut file = File::create(path)?;
			write!(
				file,
				"{}",
				data.manifest(&manifest_format)
					.with_description(|| format!("manifesting {field}"))?,
			)?;
			if !opts.manifest.no_trailing_newline {
				writeln!(file)?;
			}
			file.flush()?;
		}
	} else if let Some(path) = opts.output.output_file {
		if opts.output.create_output_dirs {
			let mut dir = path.clone();
			dir.pop();
			create_dir_all(dir)?;
		}
		let mut file = File::create(path)?;
		writeln!(file, "{}", val.manifest(manifest_format)?)?;
	} else {
		let output = val.manifest(manifest_format)?;
		if !output.is_empty() {
			println!("{output}");
		}
	}

	Ok(())
}
