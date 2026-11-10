//! `sorted` and `round(x, digits)` built and run (roadmap 1.13.13).

use super::{baryon_build, package, stderr};
use std::process::Command;

/// `sorted` of numbers and `str`s, both ways, leaving the list it copies alone, and
/// `round(x, digits)` of floats and integers, rounding a float's exact value with ties to even.
/// Each line prints what CPython prints for the same program, with `f32(x)` and `i32(n)`
/// read as `x` and `n`.
#[test]
fn sorted_and_round_with_digits_build_and_run() {
    let program = r#"
def main():
    counts = [3, 1, 2, 1]
    print(sorted(counts), sorted(counts, reverse=True), counts)
    names = ["neon", "argon", "Ångström", "ar"]
    print(sorted(names), sorted(names, reverse=True))
    print(round(2.675, 2), round(0.125, 2), round(1250.0, -2), round(1250.5, -2), round(-0.5, 0))
    print(round(1234, -2), round(1250, -2), round(1350, -2), round(7, 2))
    print(round(f32(0.125), 2), round(i32(1250), -2))
    empty: list[f64] = []
    print(sorted(empty))
"#;
    let directory = package("builtins-sorting", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[1, 1, 2, 3] [3, 2, 1, 1] [3, 1, 2, 1]\n['ar', 'argon', 'neon', 'Ångström'] ['Ångström', 'neon', 'argon', 'ar']\n2.67 0.12 1200.0 1300.0 -0.0\n1200 1200 1400 7\n0.12 1200\n[]\n"
    );
}

/// `sorted` puts every NaN last, either way, as `numpy.sort` does (Collins, 2026-11-10);
/// Python's order would depend on where the NaN starts.
#[test]
fn sorted_puts_nan_last() {
    let program = r"
import math


def main():
    print(sorted([1.0, math.nan, 0.5]), sorted([math.nan, 1.0, 0.5], reverse=True))
";
    let directory = package("builtins-sorting-nan", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[0.5, 1.0, nan] [1.0, 0.5, nan]\n"
    );
}
