//! `simd` loops built and run (decision 0011): the loop runs as the `range` loop inside it,
//! with the request to vectorize passed to LLVM, and computes what the plain loop computes.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// `axpy` writes each element at the loop variable, and `total` sums with a reduction; the
/// values are whole numbers, so the sum is exact in any order.
#[test]
fn simd_loops_build_and_run() {
    let program = "\
def axpy(a: f64, x: Array[f64, 1], y: InOut[Array[f64, 1]], n: i64):
    for i in simd(range(n)):
        y[i] = a * x[i] + y[i]


def total(x: Array[f64, 1], n: i64) -> f64:
    s = 0.0
    for i in simd(range(n)):
        s += x[i]
    return s


def main():
    n = 1001
    x: Array[f64, 1] = Array.ones(n)
    y: Array[f64, 1] = Array.zeros(n)
    axpy(2.0, x, y, n)
    axpy(1.0, x, y, n - 1)
    print(total(y, n), y[0], y[n - 1])
";
    let directory = package("simd", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "3002.0 3.0 2.0\n");
}

/// A `simd` float sum or product adds iteration `i` into lane `i % 8` and combines the lanes in
/// the fixed pairwise tree, then the value from before the loop (roadmap 1.9.14). The expected
/// numbers are what Python gives for the same lanes and tree; the plain loop's `harmonic(1001)`
/// is `7.486469861549346`, one bit away, so the test tells the two orders apart. It covers an
/// empty loop, fewer than 8 iterations, a start and a negative step, `continue`, and `*=`.
#[test]
fn simd_float_sums_add_into_eight_lanes() {
    let program = "\
def harmonic(n: i64) -> f64:
    s = 0.0
    for i in simd(range(n)):
        s += 1.0 / (i + 1)
    return s


def stepped() -> f64:
    s = 1.5
    for i in simd(range(40, 3, -3)):
        if i % 2 == 0:
            continue
        s += 1.0 / i
    return s


def growth(n: i64) -> f64:
    p = 1.0
    for i in simd(range(n)):
        p *= 1.0 + 1.0 / (i + 7)
    return p


def main():
    print(harmonic(0), harmonic(5), harmonic(1001), stepped(), growth(20))
";
    let directory = package("simd-lanes", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "0.0 2.283333333333333 7.486469861549345 1.8716968902707443 3.857142857142856\n"
    );
}
