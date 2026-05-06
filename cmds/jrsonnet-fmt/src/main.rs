use std::{
	fs,
	io::{self, Write as _},
	path::PathBuf,
	process,
};

use clap::Parser;
use jrsonnet_formatter::{FormatOptions, format};

#[derive(Parser)]
#[allow(clippy::struct_excessive_bools)]
struct Opts {
	/// Treat input as code, reformat it instead of reading file.
	#[clap(long, short = 'e')]
	exec: bool,
	/// Path to be reformatted if `--exec` if unset, otherwise code itself.
	input: String,
	/// Replace code with formatted in-place, instead of printing it to stdout.
	/// Only applicable if `--exec` is unset.
	#[clap(long, short = 'i')]
	in_place: bool,

	/// Exit with error if formatted does not match input
	#[arg(long)]
	test: bool,
	/// Number of spaces to indent with
	#[arg(long, default_value = "4")]
	indent: u8,
	/// Force hard tab for indentation
	#[arg(long, default_value = "true")]
	use_tabs: bool,
	/// Max formatted source width
	#[arg(long, default_value = "100")]
	max_width: u32,

	/// Debug option: how many times to call reformatting in case of unstable dprint output resolution.
	///
	/// 0 for not retrying to reformat.
	#[arg(long, default_value = "0")]
	conv_limit: usize,
}

#[derive(thiserror::Error, Debug)]
enum Error {
	#[error("--in-place is incompatible with --exec")]
	InPlaceExec,
	#[error("io: {0}")]
	Io(#[from] io::Error),
	#[error("persist: {0}")]
	Persist(#[from] tempfile::PersistError),
	#[error("parsing failed, refusing to reformat corrupted input")]
	Parse,
}

fn main_result() -> Result<(), Error> {
	let opts = Opts::parse();
	let input = if opts.exec {
		if opts.in_place {
			return Err(Error::InPlaceExec);
		}
		opts.input.clone()
	} else {
		fs::read_to_string(&opts.input)?
	};

	let mut iteration = 0;
	let mut formatted = input.clone();
	let mut convergence_tmp;
	// https://github.com/dprint/dprint/pull/423
	loop {
		let reformatted = match format(
			&formatted,
			&FormatOptions {
				indent: opts.indent,
				use_tabs: opts.use_tabs,
				max_width: opts.max_width,
			},
		) {
			Ok(v) => v,
			Err(e) => {
				let snippet = e.build();
				let ansi = hi_doc::source_to_ansi(&snippet);
				eprintln!("{ansi}");
				return Err(Error::Parse);
			}
		};
		convergence_tmp = reformatted.trim().to_owned();
		if formatted == convergence_tmp {
			break;
		}
		formatted = convergence_tmp;
		if opts.conv_limit == 0 {
			break;
		}
		iteration += 1;
		assert!(iteration <= opts.conv_limit, "formatting not converged");
	}
	formatted.push('\n');
	if opts.test && formatted != input {
		process::exit(1);
	}
	if opts.in_place {
		let path = PathBuf::from(opts.input);
		let mut temp = tempfile::NamedTempFile::new_in(path.parent().expect(
			"not failed during read, this path is not a directory, and there is a parent",
		))?;
		temp.write_all(formatted.as_bytes())?;
		temp.flush()?;
		temp.persist(&path)?;
	} else {
		print!("{formatted}");
	}
	Ok(())
}

fn main() {
	if let Err(e) = main_result() {
		eprintln!("{e}");
		process::exit(1);
	}
}
