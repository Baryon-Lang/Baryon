//! `--timings` (roadmap 2.0): `build`, `run` and `test` print how long each stage of the build
//! took, so a compile-speed change can state its gain in numbers.

use super::{BARYON, build_runtime, package, stderr};
use std::process::Command;

/// A two-line program: what it prints shows that `run --timings` still runs it.
const PROGRAM: &str = "\
def main():
    print(42)
";

/// The stage names, in the order the report prints them.
const STAGES: [&str; 5] = ["load", "compile", "machine code", "link", "total"];

/// `build --timings` prints one row per stage after the header with the package's size, and
/// still builds a program that runs. The milliseconds vary from run to run, so the test checks
/// the shape of each row, not its number.
#[test]
fn build_prints_each_stage() {
    build_runtime();
    let directory = package("timings-build", &[("main.byn", PROGRAM)]).unwrap();
    let program = directory.join("program");
    let built = Command::new(BARYON)
        .arg("build")
        .arg("--timings")
        .arg(directory.join("package"))
        .arg(&program)
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    let report = stderr(&built);
    let mut lines = report.lines();
    assert_eq!(lines.next(), Some("timings for 2 line(s) in 1 file(s):"));
    for stage in STAGES {
        let line = lines.next().unwrap_or_default();
        assert!(
            line.trim_start().starts_with(stage) && line.contains(" ms"),
            "{report}"
        );
    }
    assert!(report.contains(" lines/s\nbuilt "), "{report}");
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "42\n");
}

/// `run --timings` prints the report to standard error and leaves standard output to the
/// program.
#[test]
fn run_prints_the_report_to_standard_error() {
    build_runtime();
    let directory = package("timings-run", &[("main.byn", PROGRAM)]).unwrap();
    let ran = Command::new(BARYON)
        .arg("run")
        .arg(directory.join("package"))
        .arg("--timings")
        .output()
        .unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "42\n");
    assert!(stderr(&ran).starts_with("timings for 2 line(s) in 1 file(s):\n"));
}

/// `--timings` is a flag of `build`, `run` and `test` only: `check` with it is a usage error.
#[test]
fn timings_is_only_for_building_commands() {
    let directory = package("timings-check", &[("main.byn", PROGRAM)]).unwrap();
    let checked = Command::new(BARYON)
        .arg("check")
        .arg("--timings")
        .arg(directory.join("package"))
        .output()
        .unwrap();
    assert_eq!(checked.status.code(), Some(2));
    assert!(stderr(&checked).starts_with("baryon: usage: baryon check"));
}
