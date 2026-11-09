//! Integer overflow in debug and release builds (decision 0030): a debug build panics with the
//! operator and the source location, and `--release` wraps around.

use super::{BARYON, build_runtime, package, panic_report, stderr};
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// A program whose every integer operation overflows on its second call: `+`, `*` on an `i32`,
/// `-x`, and `+=` in a loop. `step` picks which one `main` runs to the overflow, so each panic
/// can be tested on its own.
const PROGRAM: &str = "\
def add(a: i64, b: i64) -> i64:
    return a + b


def times(a: i32, b: i32) -> i32:
    return a * b


def negate(a: i64) -> i64:
    return -a


def total(start: i64, count: i64) -> i64:
    sum = start
    for i in range(count):
        sum += start
    return sum


def main():
    big = 9223372036854775807
    print(add(big, -1), times(i32(46341), i32(46340)), negate(-big), total(1, 3))
    print(add(big, 1))
    print(times(i32(46341), i32(46341)))
    print(negate(-big - 1))
    print(total(big, 1))
";

/// Runs `baryon` with `arguments`, then the package in `directory`'s `package` folder.
fn baryon(arguments: &[&str], directory: &Path) -> io::Result<Output> {
    build_runtime();
    Command::new(BARYON)
        .args(arguments)
        .arg(directory.join("package"))
        .output()
}

/// A debug build, the default, stops at the first overflow with the operator and the line and
/// column of the expression, and exits with status 101 before printing the wrapped value.
#[test]
fn debug_builds_panic_on_overflow() {
    let directory = package("overflow-debug", &[("main.byn", PROGRAM)]).unwrap();
    let ran = baryon(&["run"], &directory).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "9223372036854775806 2147441940 9223372036854775807 4\n"
    );
    assert_eq!(ran.status.code(), Some(101), "{}", stderr(&ran));
    assert_eq!(
        panic_report(&ran),
        "panic: integer overflow in `+`\n  --> main.byn:2:12\n"
    );
}

/// Each operator reports itself: `*`, the `-` of a negation, and the `+` of `+=`.
#[test]
fn each_overflowing_operator_is_named() {
    let cases = [
        ("    print(add(big, 1))\n", "`*`", "main.byn:6:12"),
        (
            "    print(times(i32(46341), i32(46341)))\n",
            "`-`",
            "main.byn:10:12",
        ),
        ("    print(negate(-big - 1))\n", "`+`", "main.byn:16:9"),
    ];
    let mut program = PROGRAM.to_owned();
    for (index, (removed, operator, location)) in cases.iter().enumerate() {
        program = program.replacen(removed, "", 1);
        let name = format!("overflow-operator-{index}");
        let directory = package(&name, &[("main.byn", &program)]).unwrap();
        let ran = baryon(&["run"], &directory).unwrap();
        assert_eq!(ran.status.code(), Some(101), "{}", stderr(&ran));
        assert_eq!(
            panic_report(&ran),
            format!("panic: integer overflow in {operator}\n  --> {location}\n")
        );
    }
}

/// `--release` wraps around in two's complement, as C, Fortran and Rust's release builds do, and
/// runs to the end. The flag may come before or after the directory.
#[test]
fn release_builds_wrap() {
    let directory = package("overflow-release", &[("main.byn", PROGRAM)]).unwrap();
    let ran = baryon(&["run", "--release"], &directory).unwrap();
    assert_eq!(ran.status.code(), Some(0), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "9223372036854775806 2147441940 9223372036854775807 4\n\
         -9223372036854775808\n\
         -2147479015\n\
         -9223372036854775808\n\
         -2\n"
    );

    let program = directory.join("program");
    let built = Command::new(BARYON)
        .arg("build")
        .arg(directory.join("package"))
        .arg(&program)
        .arg("--release")
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(0), "{}", stderr(&ran));
}

/// `--release` is a flag of `build`, `run` and `test` only: `check` with it is a usage error.
#[test]
fn release_is_only_for_building_commands() {
    let directory = package("overflow-check", &[("main.byn", PROGRAM)]).unwrap();
    let checked = baryon(&["check", "--release"], &directory).unwrap();
    assert_eq!(checked.status.code(), Some(2));
    assert!(stderr(&checked).starts_with("baryon: usage: baryon check"));
}
