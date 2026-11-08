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
