//! `solve_ivp` with `method="DOP853"` (roadmap 3.19.8), built and run against `SciPy` 1.18.1: the
//! runs of [`super::solve_ivp`] again, a `Fixed` system and `f64` equations, forward and
//! backward, an empty interval and a failing run, with and without `t_eval`, and the method
//! chosen at run time.
//!
//! Each run takes `SciPy`'s steps, so the step counts and the number of calls of `fun`, the
//! three extra calls of each step that builds an interpolant for `t_eval` included, are
//! `SciPy`'s exactly. DOP853's error estimate is a sum of thirteen stages that nearly cancel,
//! and `SciPy` adds them with BLAS in an order that depends on the processor, so a step's size
//! can differ from `SciPy`'s in its ninth digit. The numbers are compared to a relative 1e-8,
//! and in the failing run only the times are. The expected lines were printed by:
//!
//! ```text
//! import numpy as np
//! from scipy.integrate import solve_ivp
//! def lorenz(t, y): return np.array([10*(y[1]-y[0]), y[0]*(28-y[2])-y[1], y[0]*y[1]-8/3*y[2]])
//! f = lambda t, y, k: -k * y
//! m = "DOP853"
//! for r in [solve_ivp(lorenz, (0, 2), np.array([1., 1., 1.]), method=m),
//!           solve_ivp(f, (0, 5), [2.0], m, args=(0.5,), rtol=1e-8, atol=1e-10),
//!           solve_ivp(f, (5, 0), [2.0], m, args=(0.5,), first_step=0.1, max_step=0.5),
//!           solve_ivp(f, (1, 1), [2.0], m, args=(0.5,)),
//!           solve_ivp(lambda t, y: y * y, (0, 2), [1.0], m),
//!           solve_ivp(lorenz, (0, 2), np.array([1., 1., 1.]), m, np.linspace(0, 2, 9)),
//!           solve_ivp(f, (5, 0), [2.0], m, [5, 3.3, .1, 0], args=(.5,))]:
//!     print(r.status, r.nfev, len(r.t), r.y.shape[0], float(r.t.sum()), float(r.y.sum()),
//!           float(r.t[-1]), float(r.y[0, -1]))
//! ```

use super::solve_ivp::agree;
use super::{baryon_build, package, stderr};
use std::process::Command;

/// `SciPy`'s line for each run, as [`super::solve_ivp`] prints them: status, calls of `fun`,
/// times reported, state length, the sum of `t`, the sum of `y`, the last time and the first
/// element of the last state.
const SCIPY: &str = "\
0 206 15 3 13.099376184616526 289.54220088843846 2.0 -8.173944643666031
0 74 7 1 14.35305860649105 7.103311387062537 5.0 0.1641699974496102
0 133 12 1 31.5 109.1451336094787 0.0 24.36498792135734
0 1 2 1 2.0 4.0 1.0 2.0
-1 1250 53 1 50.3505662636968 653053648169688.9 1.0000040284303022 309971747867459.2
0 233 9 3 9.0 129.23220971575333 2.0 -8.173944643666031
0 47 4 1 8.4 54.220960931331305 0.0 24.36497654870195";

/// Each run takes `SciPy`'s steps with DOP853: the counts match exactly and the numbers to 1e-8.
/// The Lorenz run is made twice more, with the state in an `Array` and with the method held in a
/// variable, and gives the first line each time.
#[test]
fn dop853_takes_scipys_steps() {
    let program = "\
from integrate import OdeResult, solve_ivp

Vec3 = Fixed[f64, (3,)]


def lorenz(t: f64, y: Vec3) -> Vec3:
    return Vec3(10.0 * (y[1] - y[0]), y[0] * (28.0 - y[2]) - y[1], y[0] * y[1] - 8.0 / 3.0 * y[2])


def lorenz_array(t: f64, y: Array[f64, 1]) -> Array[f64, 1]:
    dx = 10.0 * (y[1] - y[0])
    return Array([dx, y[0] * (28.0 - y[2]) - y[1], y[0] * y[1] - 8.0 / 3.0 * y[2]])


def decay(t: f64, y: f64, k: f64) -> f64:
    return -k * y


def blowup(t: f64, y: f64) -> f64:
    return y * y


def show(r: OdeResult):
    steps = len(r.t)
    last = r.y[0, steps - 1]
    print(r.status, r.nfev, steps, r.y.shape[0], r.t.sum(), r.y.sum(), r.t[steps - 1], last)


def main():
    show(solve_ivp(lorenz, (0.0, 2.0), Vec3(1.0, 1.0, 1.0), method=\"DOP853\"))
    show(solve_ivp(decay, (0.0, 5.0), 2.0, \"DOP853\", args=(0.5,), rtol=1e-8, atol=1e-10))
    show(solve_ivp(decay, (5.0, 0.0), 2.0, \"DOP853\", args=(0.5,), first_step=0.1, max_step=0.5))
    show(solve_ivp(decay, (1.0, 1.0), 2.0, \"DOP853\", args=(0.5,)))
    show(solve_ivp(blowup, (0.0, 2.0), 1.0, \"DOP853\"))
    times: Array[f64, 1] = Array.zeros(9)
    for i in range(9):
        times[i] = f64(i) / 4.0
    show(solve_ivp(lorenz, (0.0, 2.0), Vec3(1.0, 1.0, 1.0), \"DOP853\", times))
    show(solve_ivp(decay, (5.0, 0.0), 2.0, \"DOP853\", Array([5.0, 3.3, 0.1, 0.0]), args=(0.5,)))
    show(solve_ivp(lorenz_array, (0.0, 2.0), Array([1.0, 1.0, 1.0]), \"DOP853\"))
    name = \"DOP853\"
    try:
        show(solve_ivp(lorenz, (0.0, 2.0), Vec3(1.0, 1.0, 1.0), name))
    except ValueError as error:
        print(error.message)
";
    let directory = package("solve-ivp-dop853", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    let output = String::from_utf8_lossy(&ran.stdout);
    let rest = agree(SCIPY, &output, 1e-8);
    let lorenz = SCIPY.lines().next().unwrap();
    assert_eq!(
        agree(&format!("{lorenz}\n{lorenz}"), &rest.join("\n"), 1e-8),
        [] as [&str; 0]
    );
}
