use std::{
	ffi::OsString,
	mem,
	process::{Command, Stdio},
	time::Instant,
};

use anyhow::{bail, Result};
use nix::{libc, sys::wait::WaitStatus, unistd::Pid};

#[derive(Debug, Clone)]
pub struct Stats {
	pub min: f64,
	pub max: f64,
	pub mean: f64,
	pub stddev: f64,
}

impl Stats {
	fn from_samples(samples: &[f64]) -> Self {
		let n = samples.len() as f64;
		let mean = samples.iter().sum::<f64>() / n;
		let var = if samples.len() > 1 {
			samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)
		} else {
			0.0
		};
		Self {
			min: samples.iter().copied().fold(f64::INFINITY, f64::min),
			max: samples.iter().copied().fold(f64::NEG_INFINITY, f64::max),
			mean,
			stddev: var.sqrt(),
		}
	}
}

#[derive(Debug, Clone)]
pub struct BenchResult {
	pub runs: u32,
	/// Wall-clock time per run, seconds.
	pub wall_secs: Stats,
	/// Peak resident set per run, KiB (Linux `ru_maxrss`).
	pub max_rss_kib: Stats,
}

pub struct BenchOpts<'a> {
	pub program: &'a OsString,
	pub args: &'a [OsString],
	pub runs: u32,
	pub warmup: u32,
	pub output: bool,
}

pub fn bench(opts: BenchOpts<'_>) -> Result<BenchResult> {
	if opts.runs == 0 {
		bail!("runs must be >= 1");
	}

	for _ in 0..opts.warmup {
		run_once(opts.program, opts.args, opts.output)?;
	}

	let mut wall = Vec::with_capacity(opts.runs as usize);
	let mut rss = Vec::with_capacity(opts.runs as usize);
	for _ in 0..opts.runs {
		let s = run_once(opts.program, opts.args, opts.output)?;
		wall.push(s.wall_secs);
		rss.push(s.max_rss_kib as f64);
	}

	Ok(BenchResult {
		runs: opts.runs,
		wall_secs: Stats::from_samples(&wall),
		max_rss_kib: Stats::from_samples(&rss),
	})
}

struct Sample {
	wall_secs: f64,
	max_rss_kib: i64,
}

fn run_once(program: &OsString, args: &[OsString], silent: bool) -> Result<Sample> {
	let mut cmd = Command::new(program);
	cmd.args(args);
	if silent {
		cmd.stdout(Stdio::null()).stderr(Stdio::null());
	}

	let start = Instant::now();
	let child = cmd.spawn()?;
	let pid = child.id() as libc::pid_t;
	// We'll reap via wait4 ourselves; don't let std touch this handle again.
	mem::forget(child);

	let mut status: libc::c_int = 0;
	let mut ru: libc::rusage = unsafe { mem::zeroed() };
	let waited = unsafe { libc::wait4(pid, &raw mut status, 0, &raw mut ru) };
	let elapsed = start.elapsed();
	if waited < 0 {
		return Err(std::io::Error::last_os_error().into());
	}

	match WaitStatus::from_raw(Pid::from_raw(pid), status)? {
		WaitStatus::Exited(_, 0) => {}
		WaitStatus::Exited(_, code) => bail!("child exited with code {code}"),
		WaitStatus::Signaled(_, sig, _) => bail!("child killed by signal {sig:?}"),
		other => bail!("unexpected wait status: {other:?}"),
	}

	Ok(Sample {
		wall_secs: elapsed.as_secs_f64(),
		max_rss_kib: ru.ru_maxrss,
	})
}

#[cfg(target_os = "linux")]
pub fn bench_cmd(args: &[String], runs: u32, warmup: u32, output: bool) -> Result<()> {
	let program = std::ffi::OsString::from(&args[0]);
	let rest: Vec<std::ffi::OsString> = args[1..].iter().map(Into::into).collect();
	let r = bench(BenchOpts {
		program: &program,
		args: &rest,
		runs,
		warmup,
		output,
	})?;
	eprintln!(
		"runs: {}  wall: {:.3}s ± {:.3}s  [{:.3}..{:.3}]",
		r.runs, r.wall_secs.mean, r.wall_secs.stddev, r.wall_secs.min, r.wall_secs.max,
	);
	eprintln!(
		"           max_rss: {} ± {} KiB  [{}..{}]",
		r.max_rss_kib.mean as i64,
		r.max_rss_kib.stddev as i64,
		r.max_rss_kib.min as i64,
		r.max_rss_kib.max as i64,
	);
	Ok(())
}
