//! `baryon test`'s runner: starts the test program once per test, and reports each result and
//! a summary on standard output, in the shape `cargo test` uses.
//!
//! ```text
//! running 2 tests
//! test heat.test_energy ... ok
//! test heat.test_flux ... FAILED
//!
//! failures:
//!
//! ---- heat.test_flux ----
//! panic: assertion failed
//! ...
//!
//! test result: FAILED. 1 passed; 1 failed
//! ```
//!
//! A test passes when its run exits with status 0. Its output is kept back; a failed test's
//! standard output and standard error are printed after the list, in that order. Tests run one
//! after another, in the order given, so the report is the same on every run.
//!
//! With a filter, as `baryon test -k flux` gives, only the tests whose name contains it run, and
//! the summary counts the rest as filtered out: `test result: ok. 1 passed; 0 failed; 1 filtered
//! out`. It is a plain substring of the full name, `module.test_name`, as pytest's `-k` is at its
//! simplest, so `-k heat.` picks a module's tests and `-k test_flux` one test.

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

/// Runs each of `tests`, the names of the test program `program`'s tests in the order its
/// command line numbers them, whose name contains `filter` when there is one, and writes the
/// report to `out`. Returns success when every test run passed, including when none ran.
///
/// # Errors
///
/// Fails with a message when the program cannot be started or the report cannot be written.
pub(crate) fn run(
    program: &Path,
    tests: &[String],
    filter: Option<&str>,
    out: &mut impl Write,
) -> Result<ExitCode, String> {
    let written = |error: io::Error| error.to_string();
    // Each test keeps its position in `tests`, which is how the program's command line names it.
    let chosen: Vec<(usize, &String)> = tests
        .iter()
        .enumerate()
        .filter(|(_, name)| filter.is_none_or(|filter| name.contains(filter)))
        .collect();
    let plural = if chosen.len() == 1 { "" } else { "s" };
    writeln!(out, "running {} test{plural}", chosen.len()).map_err(written)?;
    let mut failures = Vec::new();
    for &(position, name) in &chosen {
        let finished = Command::new(program)
            .arg(position.to_string())
            .output()
            .map_err(|error| format!("{}: {error}", program.display()))?;
        let passed = finished.status.success();
        let result = if passed { "ok" } else { "FAILED" };
        writeln!(out, "test {name} ... {result}").map_err(written)?;
        if !passed {
            let mut output = finished.stdout;
            output.extend_from_slice(&finished.stderr);
            if finished.status.code().is_none() {
                output.extend_from_slice(b"the test was ended by a signal\n");
            }
            failures.push((name, output));
        }
    }
    if !failures.is_empty() {
        writeln!(out, "\nfailures:").map_err(written)?;
        for (name, output) in &failures {
            writeln!(out, "\n---- {name} ----").map_err(written)?;
            out.write_all(output).map_err(written)?;
        }
    }
    let passed = chosen.len() - failures.len();
    let verdict = if failures.is_empty() { "ok" } else { "FAILED" };
    let filtered = match filter {
        Some(_) => format!("; {} filtered out", tests.len() - chosen.len()),
        None => String::new(),
    };
    writeln!(
        out,
        "\ntest result: {verdict}. {passed} passed; {} failed{filtered}",
        failures.len()
    )
    .map_err(written)?;
    Ok(if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
