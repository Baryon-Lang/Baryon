//! `sum`, `min` and `max` of a sequence built and run (roadmap 1.13.13).

use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// A list sums as CPython 3.12 sums it, bit for bit: integers exactly, floats with Neumaier's
/// compensation, so `sum([1e16, 1.0, -1e16])` is `1.0`, the first float added plainly to an
/// integer start, and an infinite sum kept infinite. `min` and `max` of a list scan it, and
/// of an array give what `a.min()` gives; `sum` of an array is `a.sum()`. Each line prints
/// what CPython prints for the same program, with `Array(...)` read as `list(...)`.
#[test]
fn sum_min_and_max_of_sequences_build_and_run() {
    let program = r"
import math


def main():
    tenths = [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1]
    print(sum(tenths), sum([0.1, 0.2, 0.3]), sum([1e16, 1.0, -1e16]))
    print(sum([1, 2, 3]), sum([1, 2, 3], 10), sum([1, 2], 0.5), sum([0.1, 0.2], start=1))
    print(sum([0.1, 0.2], 1.5), sum([1e100, 1.0, -1e100, 1e-100]), sum([3.0, 1e16, -1e16], 10**17))
    print(sum([-0.0]), sum([1e308, 1e308, -1e308]), sum([math.inf, 1.0]), sum([math.inf, -math.inf]))
    print(min([3, 1, 2]), max([3, 1, 2]), min([2.5, 1.5]), max([1.0, 7.25, 3.0]))
    a = Array([1.0, 2.0, 4.0])
    print(sum(a), min(a), max(a))
";
    let directory = package("builtins-reductions", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "1.0 0.6 1.0\n6 16 3.5 1.3\n1.8 1.0 1e+17\n0.0 inf inf nan\n1 3 1.5 7.25\n7.0 1.0 4.0\n"
    );
}

/// A NaN anywhere in a list makes its `min` and `max` NaN, as for an array, wherever the NaN
/// is (Collins, 2026-11-10); Python's answer would depend on the order. An empty list panics.
#[test]
fn min_of_a_list_with_nan_is_nan_and_of_an_empty_list_panics() {
    let program = r"
import math


def main():
    print(min([1.0, math.nan, 0.5]), max([math.nan, 1.0]), min([0.5, 2.0, math.nan]))
    empty: list[i64] = []
    print(max(empty))
";
    let directory = package("builtins-reductions-panic", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "nan nan nan\n");
    assert_eq!(
        panic_report(&ran),
        "panic: `max(empty)` of an empty list\n  --> main.byn:8:11\n"
    );
}
