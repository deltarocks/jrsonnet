use anyhow::Result;
use clap::Parser;
use xshell::{Shell, cmd};

mod sourcegen;

#[derive(Parser)]
enum Opts {
	/// Generate files for rowan parser
	Sourcegen,
	/// Profile file execution
	Profile {
		#[arg(long, default_value = "true")]
		hyperfine: bool,
		#[arg(long)]
		callgrind: bool,
		#[arg(long)]
		cachegrind: bool,
		#[arg(long, default_value = env!("TARGET_PLATFORM"))]
		target: String,
		args: Vec<String>,
	},
	/// Run all lints enforced by this repo
	Lint {
		/// Also fix found issues when possible.
		#[arg(long)]
		fix: bool,
	},
	/// Build and run test file from `bindings/c`
	TestCBindings {
		#[arg(long, default_value = env!("TARGET_PLATFORM"))]
		target: String,
		/// Which bindings file to build and run
		#[arg(long, default_value = "libjsonnet_test_file")]
		test_file: String,
		args: Vec<String>,
	},
	/// Update C++/Golang golden testsuites from git
	UpdateTestsuites,
}

fn main() -> Result<()> {
	let sh = Shell::new()?;
	match Opts::parse() {
		Opts::Sourcegen => sourcegen::generate_ungrammar(),
		Opts::Profile {
			hyperfine,
			callgrind,
			cachegrind,
			args,
			target,
		} => {
			let out = sh.create_temp_dir()?;

			cmd!(sh, "cargo build --target={target} --profile releasedebug").run()?;
			let built = format!("./target/{target}/releasedebug/jrsonnet");
			let bench_cmd = format!("{built} {}", args.join(" "));
			if hyperfine {
				cmd!(sh, "hyperfine {bench_cmd}").run()?;
			}
			if callgrind {
				let args = args.clone();
				let mut callgrind_out = out.path().to_owned();
				callgrind_out.push("callgrind.out.1");
				cmd!(sh, "valgrind --tool=callgrind --dump-instr=yes --collect-jumps=yes --callgrind-out-file={callgrind_out} {built} {args...}").run()?;
				cmd!(sh, "kcachegrind {callgrind_out}").run()?;
			}
			if cachegrind {
				let mut cachegrind_out = out.path().to_owned();
				cachegrind_out.push("cachegrind.out.1");
				cmd!(
					sh,
					"valgrind --tool=cachegrind --cachegrind-out-file={cachegrind_out} {built} {args...}"
				)
				.run()?;
				cmd!(sh, "kcachegrind {cachegrind_out}").run()?;
			}

			Ok(())
		}
		Opts::Lint { fix } => {
			let fmt_check = if fix { None } else { Some("--check") };
			cmd!(sh, "cargo fmt {fmt_check...}").run()?;
			Ok(())
		}
		Opts::TestCBindings {
			target,
			test_file,
			args,
		} => {
			cmd!(
				sh,
				"cargo build -p libjsonnet --target={target} --release --no-default-features --features=interop-common,interop-threading"
			)
			.run()?;
			let built = format!("./target/{target}/release/libjsonnet.a");
			let c_bindings = "./bindings/c/";
			cmd!(sh, "cp {built} {c_bindings}").run()?;
			sh.change_dir(c_bindings);

			// TODO: Pass target to gcc?
			cmd!(sh, "gcc -c {test_file}.c").run()?;
			cmd!(sh, "gcc -o {test_file} -lc -lm {test_file}.o libjsonnet.a").run()?;
			let sh = Shell::new()?;

			cmd!(sh, "{c_bindings}{test_file} {args...}").run()?;

			Ok(())
		}
		Opts::UpdateTestsuites => {
			let _pushd = sh.push_dir("tests");
			let git_dir = sh.create_temp_dir()?;
			let git_dir_path = git_dir.path();
			cmd!(
				sh,
				"git clone https://github.com/google/jsonnet.git --depth=1 {git_dir_path}/jsonnet"
			)
			.run()?;
			cmd!(
				sh,
				"git clone https://github.com/google/go-jsonnet.git --depth=1 {git_dir_path}/go-jsonnet"
			)
			.run()?;
			sh.remove_path("cpp_test_suite")?;
			sh.remove_path("go_testdata")?;
			sh.remove_path("cpp_perf_tests")?;
			sh.remove_path("cpp_benchmarks")?;
			sh.remove_path("go_builtin_benchmarks")?;
			cmd!(sh, "mv {git_dir_path}/jsonnet/test_suite cpp_test_suite").run()?;
			cmd!(sh, "mv {git_dir_path}/go-jsonnet/testdata go_testdata").run()?;
			cmd!(sh, "mv {git_dir_path}/jsonnet/perf_tests cpp_perf_tests").run()?;
			cmd!(sh, "mv {git_dir_path}/jsonnet/benchmarks cpp_benchmarks").run()?;
			cmd!(
				sh,
				"mv {git_dir_path}/go-jsonnet/builtin-benchmarks go_builtin_benchmarks"
			)
			.run()?;

			Ok(())
		}
	}
}
