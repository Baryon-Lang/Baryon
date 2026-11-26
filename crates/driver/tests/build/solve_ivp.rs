//! `solve_ivp` with RK45 (roadmap 3.19.7.2), built and run against `SciPy` 1.18.1: a `Fixed`
//! system and `f64` equations, forward and backward, with `args=`, tolerances, `first_step=` and
//! `max_step=`, an empty interval, and a run that fails near a singularity.
//!
//! Each run takes `SciPy`'s steps, so the step counts and the number of calls of `fun` are
//! `SciPy`'s exactly. `SciPy` forms its sums with BLAS, whose rounding depends on the processor,
//! so a step's size differs from `SciPy`'s in its last bits and the difference grows over a run.
//! The numbers are compared to a relative 1e-10, and in the failing run, whose state grows as
//! `1 / (1 - t)`, only the times are. The expected lines were printed by:
//!
//! ```text
//! import numpy as np
//! from scipy.integrate import solve_ivp
//! def lorenz(t, y): return np.array([10*(y[1]-y[0]), y[0]*(28-y[2])-y[1], y[0]*y[1]-8/3*y[2]])
//! f = lambda t, y, k: -k * y
//! for r in [solve_ivp(lorenz, (0, 2), np.array([1., 1., 1.])),
//!           solve_ivp(f, (0, 5), [2.0], args=(0.5,), rtol=1e-8, atol=1e-10),
//!           solve_ivp(f, (5, 0), [2.0], args=(0.5,), first_step=0.1, max_step=0.5),
//!           solve_ivp(f, (1, 1), [2.0], args=(0.5,)),
//!           solve_ivp(lambda t, y: y * y, (0, 2), [1.0])]:
//!     print(r.status, r.nfev, len(r.t), r.y.shape[0], float(r.t.sum()), float(r.y.sum()),
//!           float(r.t[-1]), float(r.y[0, -1]))
//! ```

use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// `SciPy`'s line for each run: status, calls of `fun`, steps kept, state length, then the sum
/// of `t`, the sum of `y`, the last time and the first element of the last state.
const SCIPY: &str = "\
0 158 23 3 19.180155390411677 467.17867364382965 2.0 -8.167936673944697
0 176 30 1 74.04214666054877 23.510770642129973 5.0 0.1641699981823389
0 67 12 1 31.5 109.14516608928459 0.0 24.364997671260696
0 1 2 1 2.0 4.0 1.0 2.0
-1 632 67 1 63.883757720511255 1165736867277266.5 0.9999286400563746 480848617754195.6";

/// Each run takes `SciPy`'s steps: the counts match exactly and the numbers to 1e-10.
#[test]
fn rk45_takes_scipys_steps() {
    let program = "\
from integrate import OdeResult, solve_ivp

Vec3 = Fixed[f64, (3,)]


def lorenz(t: f64, y: Vec3) -> Vec3:
    return Vec3(10.0 * (y[1] - y[0]), y[0] * (28.0 - y[2]) - y[1], y[0] * y[1] - 8.0 / 3.0 * y[2])


def decay(t: f64, y: f64, k: f64) -> f64:
    return -k * y


def blowup(t: f64, y: f64) -> f64:
    return y * y


def show(r: OdeResult):
    steps = len(r.t)
    last = r.y[0, steps - 1]
    print(r.status, r.nfev, steps, r.y.shape[0], r.t.sum(), r.y.sum(), r.t[steps - 1], last)


def main():
    show(solve_ivp(lorenz, (0.0, 2.0), Vec3(1.0, 1.0, 1.0)))
    show(solve_ivp(decay, (0.0, 5.0), 2.0, args=(0.5,), rtol=1e-8, atol=1e-10))
    show(solve_ivp(decay, (5.0, 0.0), 2.0, args=(0.5,), first_step=0.1, max_step=0.5))
    show(solve_ivp(decay, (1.0, 1.0), 2.0, args=(0.5,)))
    failed = solve_ivp(blowup, (0.0, 2.0), 1.0)
    show(failed)
    print(failed.success, failed.message, failed.njev, failed.nlu, failed.sol is None)
    print(failed.t_events is None, failed.y_events is None)
";
    let directory = package("solve-ivp-rk45", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    let output = String::from_utf8_lossy(&ran.stdout);
    let mut lines = output.lines();
    for (expected, found) in SCIPY.lines().zip(&mut lines) {
        let expected: Vec<&str> = expected.split(' ').collect();
        let found: Vec<&str> = found.split(' ').collect();
        assert_eq!(expected[..4], found[..4], "{output}");
        // The failing run's state is near a singularity, so only its times are compared.
        let compared: &[usize] = if expected[0] == "-1" {
            &[4, 6]
        } else {
            &[4, 5, 6, 7]
        };
        for &column in compared {
            let (want, got) = (expected[column], found[column]);
            let (want, got): (f64, f64) = (want.parse().unwrap(), got.parse().unwrap());
            assert!(
                (want - got).abs() <= 1e-10 * want.abs(),
                "{want} {got}\n{output}"
            );
        }
    }
    assert_eq!(
        lines.collect::<Vec<_>>(),
        [
            "False Required step size is less than spacing between numbers. 0 0 True",
            "True True"
        ]
    );
}

/// A `max_step` that is not positive is a mistake in the program and panics with `SciPy`'s
/// message, where `SciPy` raises `ValueError` (decision 0065 point 3).
#[test]
fn a_step_limit_that_is_not_positive_panics() {
    let program = "\
from integrate import solve_ivp


def decay(t: f64, y: f64) -> f64:
    return -y


def main():
    result = solve_ivp(decay, (0.0, 1.0), 1.0, max_step=0.0)
    print(result.status)
";
    let directory = package("solve-ivp-max-step", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101), "{ran:?}");
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: max_step > 0.0, \"`max_step` must be positive.\"\n  \
         --> <std>/integrate_ivp.byn:232:5\n"
    );
}
