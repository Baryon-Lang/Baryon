//! `prange` loops built and run (decision 0018): the iterations spread over the runtime's
//! thread pool, arrays written at each iteration's own elements, and reductions combined in a
//! fixed order, so the result is the same on every run.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// A `prange` over 100,000 iterations writes `ys[i]` and sums into `total`, and one with a
/// start, a step and `continue` multiplies into `product`. The float sum gives the same bits on
/// every run, whatever the thread count.
#[test]
fn prange_loops_build_and_run() {
    let program = "\
def scale(xs: Array[f64, 1], ys: InOut[Array[f64, 1]], factor: f64) -> f64:
    total = 0.0
    for i in prange(len(xs)):
        y = xs[i] * factor
        ys[i] = y
        total += y
    return total


def odd(n: i64) -> i64:
    product = 1
    for i in prange(1, n, 2):
        if i == 5:
            continue
        product *= i
    return product


def main():
    n = 100000
    xs: Array[f64, 1] = Array.zeros(n)
    for i in range(n):
        xs[i] = 1.0 / (i + 1)
    ys: Array[f64, 1] = Array.zeros(n)
    print(scale(xs, ys, 2.0), ys[0], ys[n - 1])
    print(odd(12), odd(0))
";
    let directory = package("prange", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let first = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&first), "");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        "24.180292259726855 2.0 2e-05\n2079 1\n"
    );
    for _ in 0..5 {
        let again = Command::new(&program).output().unwrap();
        assert_eq!(again.stdout, first.stdout);
    }
}
