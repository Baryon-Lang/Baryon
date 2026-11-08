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

/// Arrays listed in `accumulate=` (decisions 0013 and 0024): a histogram whose iterations add
/// into any bin, and a pair loop that applies each pair's force to both atoms, `Fixed`
/// elements and single lanes included. The results match the serial loops and give the same
/// bits on every run.
#[test]
fn accumulated_arrays_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def histogram(bins: InOut[Array[i64, 1]], n: i64):
    for i in prange(n, accumulate=bins):
        bins[(i * 7919) % 10] += 1
        bins[0] -= 1


def pairs(positions: Array[Vec3, 1], forces: InOut[Array[Vec3, 1]], n: i64):
    for i in prange(n, accumulate=forces):
        for j in range(i + 1, n):
            force = (positions[i] - positions[j]) / float(j - i)
            forces[i] += force
            forces[j] -= force
            forces[j][2] += 0.5


def main():
    bins: Array[i64, 1] = Array.zeros(10)
    histogram(bins, 100000)
    print(bins[0], bins[1], bins[9])
    n = 2000
    positions = Array.zeros(n, dtype=Vec3)
    for k in range(n):
        positions[k][0] = float(k)
        positions[k][1] = 1.0 / float(k + 1)
    forces = Array.zeros(n, dtype=Vec3)
    pairs(positions, forces, n)
    print(forces[0][0], forces[0][1], forces[n - 1][1], forces[n - 1][2])
";
    let directory = package("accumulate", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let first = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&first), "");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        "-90000 10000 10000\n-1999.0 7.178368103610292 -0.004088934051805141 999.5\n"
    );
    for _ in 0..5 {
        let again = Command::new(&program).output().unwrap();
        assert_eq!(again.stdout, first.stdout);
    }
}
