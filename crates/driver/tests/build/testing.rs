//! `baryon test` built and run (roadmap 1.11.6): each `@test` function in its own process, each
//! result reported, and a failed test's output after the list.

use super::{BACKTRACE, BARYON, build_runtime, package, stderr};
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Runs `baryon test` on the package in `directory`.
fn baryon_test(directory: &Path) -> io::Result<Output> {
    build_runtime();
    Command::new(BARYON)
        .arg("test")
        .arg(directory.join("package"))
        .output()
}

/// The text of standard output, each backtrace cut out: the line that starts it and the frame
/// lines after it, which name the scratch directory.
fn report(output: &Output) -> String {
    let text = String::from_utf8_lossy(&output.stdout);
    let mut kept = String::new();
    let mut in_backtrace = false;
    for line in text.split_inclusive('\n') {
        if line == BACKTRACE {
            in_backtrace = true;
        } else if !(in_backtrace && line.starts_with("  ")) {
            in_backtrace = false;
            kept.push_str(line);
        }
    }
    kept
}

/// Tests across two modules, in file and source order: one passes, one fails a check after
/// printing, which panics and ends only its own run, and one raises an error. The package has
/// no `main`, and a function without `@test` is not run.
#[test]
fn each_test_runs_and_is_reported() {
    let directory = package(
        "tests",
        &[
            (
                "heat.byn",
                "class Unstable(Exception):\n\
                 \x20   step: i64\n\
                 \n\
                 \n\
                 def flux(a: f64, b: f64) -> f64:\n\
                 \x20   return a - b\n\
                 \n\
                 \n\
                 @test\n\
                 def test_flux():\n\
                 \x20   assert flux(3.0, 1.0) == 2.0\n\
                 \n\
                 \n\
                 @test\n\
                 def test_wrong():\n\
                 \x20   print(\"before the check\")\n\
                 \x20   assert flux(1.0, 1.0) == 1.0\n\
                 \n\
                 \n\
                 @test\n\
                 def test_raises() -> Raises[None, Unstable]:\n\
                 \x20   raise Unstable(step=4)\n",
            ),
            (
                "wave.byn",
                "def helper():\n\
                 \x20   assert False\n\
                 \n\
                 \n\
                 @test\n\
                 def test_speed():\n\
                 \x20   assert 3 * 4 == 12\n",
            ),
        ],
    )
    .unwrap();
    let output = baryon_test(&directory).unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        report(&output),
        "running 4 tests\n\
         test heat.test_flux ... ok\n\
         test heat.test_wrong ... FAILED\n\
         test heat.test_raises ... FAILED\n\
         test wave.test_speed ... ok\n\
         \n\
         failures:\n\
         \n\
         ---- heat.test_wrong ----\n\
         before the check\n\
         panic: assertion failed: flux(1.0, 1.0) == 1.0\n  \
         --> heat.byn:17:5\n\
         \n\
         ---- heat.test_raises ----\n\
         error: Unstable(step=4)\n\
         \n\
         test result: FAILED. 2 passed; 2 failed\n"
    );
}

/// Every test passing exits with status 0; a package with no tests passes with none run.
#[test]
fn passing_tests_exit_with_success() {
    let directory = package(
        "tests-pass",
        &[(
            "heat.byn",
            "@test\ndef test_flux():\n    assert 2 + 2 == 4\n",
        )],
    )
    .unwrap();
    let output = baryon_test(&directory).unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        report(&output),
        "running 1 test\n\
         test heat.test_flux ... ok\n\
         \n\
         test result: ok. 1 passed; 0 failed\n"
    );

    let empty = package(
        "tests-none",
        &[("heat.byn", "def f() -> i64:\n    return 1\n")],
    )
    .unwrap();
    let output = baryon_test(&empty).unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        report(&output),
        "running 0 tests\n\ntest result: ok. 0 passed; 0 failed\n"
    );
}

/// `-k` runs only the tests whose full name contains its text, and the summary counts the rest
/// as filtered out; a module name with its dot picks that module's tests.
#[test]
fn a_filter_runs_only_the_tests_it_names() {
    let directory = package(
        "tests-filter",
        &[
            (
                "heat.byn",
                "@test\ndef test_flux():\n    assert 2 + 2 == 4\n\n\n\
                 @test\ndef test_wrong():\n    assert 2 + 2 == 5\n",
            ),
            (
                "wave.byn",
                "@test\ndef test_flux_speed():\n    assert 3 * 4 == 12\n",
            ),
        ],
    )
    .unwrap();
    let filtered = |filter: &str| {
        build_runtime();
        Command::new(BARYON)
            .args(["test", "-k", filter])
            .arg(directory.join("package"))
            .output()
            .unwrap()
    };
    let output = filtered("flux");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        report(&output),
        "running 2 tests\n\
         test heat.test_flux ... ok\n\
         test wave.test_flux_speed ... ok\n\
         \n\
         test result: ok. 2 passed; 0 failed; 1 filtered out\n"
    );
    let output = filtered("wave.");
    assert_eq!(
        report(&output),
        "running 1 test\n\
         test wave.test_flux_speed ... ok\n\
         \n\
         test result: ok. 1 passed; 0 failed; 2 filtered out\n"
    );
}

/// A test that takes parameters, or a method decorated `@test`, is an error, and nothing runs.
#[test]
fn tests_take_no_parameters_and_are_top_level() {
    let directory = package(
        "tests-wrong",
        &[(
            "heat.byn",
            "@test\n\
             def test_flux(n: i64):\n\
             \x20   assert n == n\n\
             \n\
             \n\
             class Grid(Struct):\n\
             \x20   n: i64\n\
             \n\
             \x20   @test\n\
             \x20   def test_size(self):\n\
             \x20       assert self.n == 0\n",
        )],
    )
    .unwrap();
    let output = baryon_test(&directory).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report(&output), "");
    assert_eq!(
        stderr(&output),
        "error: a test must take no parameters and return `None`\n \
         --> heat.byn:2:5\n  \
         |\n\
         2 | def test_flux(n: i64):\n  \
         |     ^^^^^^^^^ a test must take no parameters and return `None`\n\n\
         error: `@test` goes on a top-level function\n  \
         --> heat.byn:10:9\n   \
         |\n\
         10 |     def test_size(self):\n   \
         |         ^^^^^^^^^ `@test` goes on a top-level function\n\n"
    );
}
