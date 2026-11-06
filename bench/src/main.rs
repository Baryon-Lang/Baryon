//! `cargo kernels`: the Phase 0 baseline benchmark suite.
//!
//! NPL must run numeric code within 5% of `gfortran -O3`. This tool measures what "within 5%"
//! means: it builds eight physics kernels written in C++ and in Fortran, runs them, checks that
//! both languages computed the same answer, and prints the times. With `--record` it saves them as
//! the machine's baseline in `bench/baselines/`, the numbers the NPL compiler must later match.
//!
//! ```text
//! cargo kernels                    # reference size, 5 runs each, print a table
//! cargo kernels --size smoke       # tiny problems: does everything build and agree?
//! cargo kernels --record           # also write bench/baselines/<os>-<arch>.json
//! cargo kernels --only nbody,fft   # a subset
//! ```
//!
//! The kernels and their sizes are listed in [`suite::KERNELS`]; `bench/README.md` explains each
//! kernel, the output protocol and the baseline format.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool that reports to the terminal"
)]

mod baseline;
mod measure;
mod suite;
mod toolchain;

use baseline::{Context, KernelResult};
use measure::relative_difference;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use suite::{KERNELS, Kernel, Size};
use toolchain::{Compiler, Language};

/// Help text for `--help` and for an unknown option.
const USAGE: &str = "\
Usage: cargo kernels [options]

Builds the C++ and Fortran physics kernels, runs them, checks that both languages agree,
and prints the times.

Options:
  --size <smoke|reference>   Problem size (default: reference)
  --repetitions <n>          Runs per kernel and language (default: 5, or 1 for smoke)
  --only <name,name,...>     Run only these kernels
  --record                   Write the results to bench/baselines/<os>-<arch>.json
  --help                     Show this message

Environment:
  CXX, CXXFLAGS              C++ compiler and extra flags (default: clang++)
  FC, FFLAGS                 Fortran compiler and extra flags (default: gfortran)";

/// What the command line asked for.
#[derive(Debug, PartialEq)]
struct Options {
    /// Problem size.
    size: Size,
    /// Runs per kernel and language.
    repetitions: usize,
    /// Kernel names to run; empty means all.
    only: Vec<String>,
    /// Whether to write a baseline file.
    record: bool,
}

/// What the command line asked the tool to do.
#[derive(Debug, PartialEq)]
enum Request {
    /// Run the suite.
    Run(Options),
    /// Print the usage text.
    Help,
}

/// Parses the arguments after the program name.
fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<Request, String> {
    let mut size = Size::Reference;
    let mut repetitions = None;
    let mut only = Vec::new();
    let mut record = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let mut value = |option: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("`{option}` needs a value"))
        };
        match argument.as_str() {
            "--size" => size = value("--size")?.parse()?,
            "--repetitions" => {
                let text = value("--repetitions")?;
                let count: usize =
                    text.parse()
                        .ok()
                        .filter(|count| *count > 0)
                        .ok_or_else(|| {
                            format!("`--repetitions` must be a positive integer, not `{text}`")
                        })?;
                repetitions = Some(count);
            }
            "--only" => {
                only = value("--only")?
                    .split(',')
                    .map(|name| name.trim().to_owned())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            "--record" => record = true,
            "--help" | "-h" => return Ok(Request::Help),
            unknown => return Err(format!("unknown option `{unknown}`")),
        }
    }
    let repetitions = repetitions.unwrap_or(match size {
        Size::Smoke => 1,
        Size::Reference => 5,
    });
    Ok(Request::Run(Options {
        size,
        repetitions,
        only,
        record,
    }))
}

/// The kernels `only` selects, in suite order; all of them when `only` is empty.
fn select(only: &[String]) -> Result<Vec<&'static Kernel>, String> {
    if let Some(unknown) = only
        .iter()
        .find(|name| !KERNELS.iter().any(|kernel| kernel.name == name.as_str()))
    {
        let known: Vec<&str> = KERNELS.iter().map(|kernel| kernel.name).collect();
        return Err(format!(
            "unknown kernel `{unknown}`; expected one of: {}",
            known.join(", ")
        ));
    }
    Ok(KERNELS
        .iter()
        .filter(|kernel| only.is_empty() || only.iter().any(|name| name == kernel.name))
        .collect())
}

/// The `bench/` directory, which holds this crate, the kernels and the baselines.
fn bench_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Builds, runs and verifies every selected kernel, prints the table and, if asked, records it.
fn run(options: &Options) -> Result<(), String> {
    let bench = bench_directory();
    let repository = bench.parent().unwrap_or(&bench).to_path_buf();
    let kernels_directory = bench.join("kernels");
    let build_directory = repository.join("target").join("bench");
    let kernels = select(&options.only)?;

    let mut compilers = Vec::new();
    for language in Language::ALL {
        let compiler = Compiler::from_environment(language);
        let version = compiler.version()?;
        println!(
            "{:<8} {}: {version}",
            language.display_name(),
            compiler.command
        );
        compilers.push((compiler, version));
    }
    println!(
        "size {}, {} run(s) each; times are the fastest run, in seconds\n",
        options.size, options.repetitions
    );
    println!(
        "{:<20} {:>12} {:>12} {:>9}  checksum",
        "kernel", "C++", "Fortran", "C++/F"
    );

    let mut results = Vec::new();
    let mut disagreements = Vec::new();
    for kernel in kernels {
        let parameters = kernel.parameters(options.size);
        let mut measurements = Vec::new();
        for (compiler, _) in &compilers {
            let executable = compiler.build(&kernels_directory, &build_directory, kernel.name)?;
            let measurement = measure::measure(&executable, parameters, options.repetitions)?;
            measurements.push((compiler.language, measurement));
        }
        let result = KernelResult {
            kernel,
            measurements,
        };
        let verdict = verdict(&result);
        print_row(&result, &verdict);
        if let Err(problem) = verdict {
            disagreements.push(problem);
        }
        results.push(result);
    }

    if !disagreements.is_empty() {
        return Err(format!(
            "the languages disagree:\n  {}",
            disagreements.join("\n  ")
        ));
    }
    if options.record {
        let context = Context {
            size: options.size,
            repetitions: options.repetitions,
            compilers: &compilers,
        };
        let path = baseline::path(&bench.join("baselines"));
        write_baseline(&path, &baseline::document(&context, &results, &repository))?;
        println!("\nrecorded {}", path.display());
    }
    Ok(())
}

/// Whether both languages' checksums agree within the kernel's tolerance, with the relative
/// difference on success and a description of the problem on failure.
fn verdict(result: &KernelResult<'_>) -> Result<f64, String> {
    let checksums: Vec<f64> = result
        .measurements
        .iter()
        .map(|(_, measurement)| measurement.checksum)
        .collect();
    let difference = match checksums.as_slice() {
        [first, rest @ ..] => rest
            .iter()
            .map(|checksum| relative_difference(*first, *checksum))
            .fold(0.0, f64::max),
        [] => 0.0,
    };
    if difference <= result.kernel.tolerance {
        Ok(difference)
    } else {
        Err(format!(
            "{}: checksums {checksums:?} differ by {difference:.2e}, more than the tolerance {:.0e}",
            result.kernel.name, result.kernel.tolerance
        ))
    }
}

/// Prints one table row: the fastest time per language, their ratio, and the checksum verdict.
fn print_row(result: &KernelResult<'_>, verdict: &Result<f64, String>) {
    let times: Vec<f64> = result
        .measurements
        .iter()
        .map(|(_, measurement)| measurement.minimum())
        .collect();
    let [cpp, fortran] = times.as_slice() else {
        return;
    };
    let status = match verdict {
        Ok(difference) if *difference == 0.0 => String::from("identical"),
        Ok(difference) => format!("agree ({difference:.1e})"),
        Err(_) => String::from("DISAGREE"),
    };
    println!(
        "{:<20} {cpp:>12.4} {fortran:>12.4} {:>9.2}  {status}",
        result.kernel.name,
        cpp / fortran
    );
}

/// Writes `document` to `path` as pretty-printed JSON with a trailing newline.
fn write_baseline(path: &Path, document: &serde_json::Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(document).map_err(|error| error.to_string())?;
    text.push('\n');
    std::fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Parses the command line, runs the suite and exits non-zero on any failure.
fn main() -> ExitCode {
    let outcome = match parse_arguments(std::env::args().skip(1)) {
        Ok(Request::Help) => {
            println!("{USAGE}");
            Ok(())
        }
        Ok(Request::Run(options)) => run(&options),
        Err(error) => Err(format!("{error}\n\n{USAGE}")),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Request, String> {
        parse_arguments(arguments.iter().map(|argument| String::from(*argument)))
    }

    #[test]
    fn defaults_to_five_reference_runs() {
        assert_eq!(
            parse(&[]),
            Ok(Request::Run(Options {
                size: Size::Reference,
                repetitions: 5,
                only: Vec::new(),
                record: false,
            }))
        );
    }

    #[test]
    fn smoke_defaults_to_one_run() {
        let Ok(Request::Run(options)) = parse(&["--size", "smoke"]) else {
            panic!("expected options");
        };
        assert_eq!(options.repetitions, 1);
    }

    #[test]
    fn reads_every_option() {
        assert_eq!(
            parse(&[
                "--size",
                "smoke",
                "--repetitions",
                "3",
                "--only",
                "nbody, fft",
                "--record"
            ]),
            Ok(Request::Run(Options {
                size: Size::Smoke,
                repetitions: 3,
                only: vec![String::from("nbody"), String::from("fft")],
                record: true,
            }))
        );
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse(&["--repetitions", "0"]).is_err());
        assert!(parse(&["--size"]).is_err());
        assert!(parse(&["--fast"]).is_err());
        assert_eq!(parse(&["--help"]), Ok(Request::Help));
    }

    #[test]
    fn selects_kernels_by_name_in_suite_order() {
        let selected = select(&[String::from("fft"), String::from("nbody")]);
        let names: Option<Vec<&str>> = selected
            .ok()
            .map(|kernels| kernels.iter().map(|kernel| kernel.name).collect());
        assert_eq!(names, Some(vec!["nbody", "fft"]));
        assert!(select(&[String::from("lattice_qcd")]).is_err());
        assert_eq!(select(&[]).map(|kernels| kernels.len()), Ok(KERNELS.len()));
    }
}
