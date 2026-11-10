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

/// The arithmetic decision 0030 first left unchecked (roadmap 1.11.8): `+` and `-x` in a generic
/// function instantiated with an integer, `abs` of the minimum, whole-array arithmetic on a
/// `Fixed` and an `Array` of integers, and integer `a.sum()` and `v.dot(w)`. `main` runs each
/// once without overflow, then the lines after it overflow one by one.
const LEFTOVERS: &str = "\
Pair = Fixed[i64, (2,)]


def double[T: Number](x: T) -> T:
    return x + x


def flip[T: Number](x: T) -> T:
    return -x


def main():
    big = 9223372036854775807
    small = -big - 1
    v = Pair(big, 1)
    w = Pair(1, 1)
    items: list[i64] = [big, 1]
    print(double(i32(5)), flip(big), abs(-big), (v - w)[0], (Array(items) * 1)[0])
    print(w.sum(), Pair(3, 4).dot(Pair(5, 6)))
    print(double(big))
    print(double(i32(1073741824)))
    print(flip(small))
    print(abs(small))
    print((v + w)[0])
    print((Array(items) * 2)[0])
    print((-Array([small]))[0])
    print(Array(items).sum())
    print(v.dot(w))
";

/// The first output line of [`LEFTOVERS`], which every build prints before the first overflow.
const LEFTOVERS_FIRST: &str =
    "10 -9223372036854775807 9223372036854775807 9223372036854775806 9223372036854775807\n2 39\n";

/// Each line after the first two of `main` in [`LEFTOVERS`] panics in a debug build with its
/// operator and location: the generic `+` at both `i64` and `i32`, the generic `-x`, `abs`,
/// whole-array `+`, `*` and `-x`, and the `+` inside `sum` and `dot`.
#[test]
fn generic_array_and_method_arithmetic_is_checked() {
    let cases = [
        ("`+`", "main.byn:5:12"),
        ("`+`", "main.byn:5:12"),
        ("`-`", "main.byn:9:12"),
        ("`-`", "main.byn:20:11"),
        ("`+`", "main.byn:20:12"),
        ("`*`", "main.byn:20:12"),
        ("`-`", "main.byn:20:12"),
        ("`+`", "main.byn:20:11"),
        ("`+`", "main.byn:20:11"),
    ];
    let (head, tail) = LEFTOVERS.split_at(LEFTOVERS.find("    print(double(big))").unwrap());
    let lines: Vec<&str> = tail.lines().collect();
    for (index, (operator, location)) in cases.iter().enumerate() {
        let program = format!("{head}{}\n", lines[index]);
        let name = format!("overflow-leftover-{index}");
        let directory = package(&name, &[("main.byn", &program)]).unwrap();
        let ran = baryon(&["run"], &directory).unwrap();
        assert_eq!(String::from_utf8_lossy(&ran.stdout), LEFTOVERS_FIRST);
        assert_eq!(ran.status.code(), Some(101), "{index}: {}", stderr(&ran));
        assert_eq!(
            panic_report(&ran),
            format!("panic: integer overflow in {operator}\n  --> {location}\n"),
            "{index}"
        );
    }
}

/// `--release` wraps every one of them around, as it wraps `+` on two integers.
#[test]
fn release_builds_wrap_generic_array_and_method_arithmetic() {
    let directory = package("overflow-leftovers-release", &[("main.byn", LEFTOVERS)]).unwrap();
    let ran = baryon(&["run", "--release"], &directory).unwrap();
    assert_eq!(ran.status.code(), Some(0), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        format!(
            "{LEFTOVERS_FIRST}-2\n-2147483648\n-9223372036854775808\n-9223372036854775808\n\
             -9223372036854775808\n-2\n-9223372036854775808\n-9223372036854775808\n\
             -9223372036854775808\n"
        )
    );
}
