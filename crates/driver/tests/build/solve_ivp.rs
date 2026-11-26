//! `solve_ivp` with RK45 (roadmap 3.19.7.2 to 3.19.7.4), built and run against `SciPy` 1.18.1: a
//! `Fixed` or `Array` system and `f64` equations, forward and backward, with `args=`, tolerances,
//! `first_step=` and `max_step=`, an empty interval, and a run that fails near a singularity.
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

/// Asserts that each line `output` begins with matches `SciPy`'s line in `expected`: the four
/// counts exactly and the numbers to a relative `tolerance`, only the times for a failed run.
/// Returns the lines after them.
pub(super) fn agree<'a>(expected: &str, output: &'a str, tolerance: f64) -> Vec<&'a str> {
    let mut lines = output.lines();
    for (expected, found) in expected.lines().zip(&mut lines) {
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
            let (want, got): (f64, f64) = (
                expected[column].parse().unwrap_or(f64::NAN),
                found[column].parse().unwrap_or(f64::NAN),
            );
            assert!(
                (want - got).abs() <= tolerance * want.abs(),
                "{want} {got}\n{output}"
            );
        }
    }
    lines.collect()
}

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
    assert_eq!(
        agree(SCIPY, &output, 1e-10),
        [
            "False Required step size is less than spacing between numbers. 0 0 True",
            "True True"
        ]
    );
}

/// `t_eval` reports the state at the times asked for, read from each step's interpolant, forward
/// and backward, and reports none for an empty `t_span`, as `SciPy` does. The expected lines were
/// printed by the script above with these runs, `np.linspace(0, 2, 9)` being `0.0, 0.25, ...`:
///
/// ```text
/// solve_ivp(lorenz, (0, 2), np.array([1., 1., 1.]), t_eval=np.linspace(0, 2, 9))
/// solve_ivp(f, (5, 0), [2.0], t_eval=[5, 3.3, .1, 0], args=(.5,))
/// ```
#[test]
fn t_eval_reports_the_times_asked_for() {
    let program = "\
from integrate import OdeResult, solve_ivp

Vec3 = Fixed[f64, (3,)]


def lorenz(t: f64, y: Vec3) -> Vec3:
    return Vec3(10.0 * (y[1] - y[0]), y[0] * (28.0 - y[2]) - y[1], y[0] * y[1] - 8.0 / 3.0 * y[2])


def decay(t: f64, y: f64, k: f64) -> f64:
    return -k * y


def show(r: OdeResult):
    steps = len(r.t)
    last = r.y[0, steps - 1]
    print(r.status, r.nfev, steps, r.y.shape[0], r.t.sum(), r.y.sum(), r.t[steps - 1], last)


def main():
    times: Array[f64, 1] = Array.zeros(9)
    for i in range(9):
        times[i] = f64(i) / 4.0
    show(solve_ivp(lorenz, (0.0, 2.0), Vec3(1.0, 1.0, 1.0), t_eval=times))
    show(solve_ivp(decay, (5.0, 0.0), 2.0, t_eval=Array([5.0, 3.3, 0.1, 0.0]), args=(0.5,)))
    empty = solve_ivp(decay, (1.0, 1.0), 2.0, t_eval=Array([1.0]), args=(0.5,))
    print(empty.nfev, len(empty.t), empty.y.shape)
";
    let directory = package("solve-ivp-t-eval", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    let output = String::from_utf8_lossy(&ran.stdout);
    let expected = "\
0 158 9 3 9.0 129.40540591863066 2.0 -8.167936673944695
0 26 4 1 8.4 54.22261829822385 0.0 24.365664492751193";
    assert_eq!(agree(expected, &output, 1e-10), ["1 0 (1, 0)"]);
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
         --> <std>/integrate_ivp.byn:320:5\n"
    );
}

/// `method=` is chosen when compiling (decision 0065 point 2): `"RK45"` by position or keyword
/// runs RK45, and a method held in a variable is picked at run time, raising `ValueError` for a
/// name that is not built.
#[test]
fn methods_are_chosen_when_compiling_or_at_run_time() {
    let program = "\
from integrate import solve_ivp


def decay(t: f64, y: f64) -> f64:
    return -y


def main():
    by_position = solve_ivp(decay, (0.0, 1.0), 1.0, \"RK45\")
    by_keyword = solve_ivp(decay, (0.0, 1.0), 1.0, method=\"RK45\")
    print(by_position.nfev)
    print(by_keyword.nfev)
    for name in [\"RK45\", \"LSODA\"]:
        try:
            result = solve_ivp(decay, (0.0, 1.0), 1.0, method=name)
            print(result.nfev)
        except ValueError as error:
            print(error.message)
";
    let directory = package("solve-ivp-methods", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "14\n14\n14\n`method` must be one of {'RK45', 'DOP853'} or OdeSolver class.\n"
    );
}

/// A method written as a literal that is not built is a compile error that lists the methods.
#[test]
fn an_unknown_method_literal_is_a_compile_error() {
    let program = "\
from integrate import solve_ivp


def decay(t: f64, y: f64) -> f64:
    return -y


def main():
    result = solve_ivp(decay, (0.0, 1.0), 1.0, method=\"RK54\")
    print(result.nfev)
";
    let directory = package("solve-ivp-unknown-method", &[("main.byn", program)]).unwrap();
    let (output, _) = baryon_build(&directory).unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr(&output),
        "\
error: `RK54` is not a method of `solve_ivp`
 --> main.byn:9:55
  |
9 |     result = solve_ivp(decay, (0.0, 1.0), 1.0, method=\"RK54\")
  |                                                       ^^^^^^ unknown method
  = note: the methods built are \"RK45\", \"DOP853\"

"
    );
}

/// An `Array[f64, 1]` state takes the same steps as a `Fixed` one, with and without `t_eval`,
/// and frees each state once (roadmap 1.9.75): the Lorenz run of [`rk45_takes_scipys_steps`]
/// and of [`t_eval_reports_the_times_asked_for`], with the state held in an `Array`.
#[test]
fn an_array_state_takes_scipys_steps() {
    let program = "\
from integrate import OdeResult, solve_ivp


def lorenz(t: f64, y: Array[f64, 1]) -> Array[f64, 1]:
    dx = 10.0 * (y[1] - y[0])
    return Array([dx, y[0] * (28.0 - y[2]) - y[1], y[0] * y[1] - 8.0 / 3.0 * y[2]])


def show(r: OdeResult):
    steps = len(r.t)
    last = r.y[0, steps - 1]
    print(r.status, r.nfev, steps, r.y.shape[0], r.t.sum(), r.y.sum(), r.t[steps - 1], last)


def main():
    y0 = Array([1.0, 1.0, 1.0])
    show(solve_ivp(lorenz, (0.0, 2.0), y0))
    times: Array[f64, 1] = Array.zeros(9)
    for i in range(9):
        times[i] = f64(i) / 4.0
    show(solve_ivp(lorenz, (0.0, 2.0), y0, t_eval=times))
    print(y0)
";
    let directory = package("solve-ivp-array", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    let output = String::from_utf8_lossy(&ran.stdout);
    let expected = "\
0 158 23 3 19.180155390411677 467.17867364382965 2.0 -8.167936673944697
0 158 9 3 9.0 129.40540591863066 2.0 -8.167936673944695";
    assert_eq!(agree(expected, &output, 1e-10), ["[1. 1. 1.]"]);
}
