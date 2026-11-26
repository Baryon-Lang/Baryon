//! `solve_ivp` with `dense_output=True` (roadmap 3.19.8.2 and 3.19.8.3), built and run against
//! `SciPy` 1.18.1: `sol(t)` read inside the run, at its ends and outside it, for RK45 and
//! DOP853, forward and backward, with `t_eval`, and over an empty `t_span`.
//!
//! The steps are `SciPy`'s, so the counts match exactly: calls of `fun` (DOP853 adds three a
//! step to build each interpolant), times reported and the times `sol` holds. The states are
//! compared to a relative 1e-8, DOP853's agreement (see [`super::solve_ivp_dop853`]). The
//! expected lines were printed by:
//!
//! ```text
//! import numpy as np
//! from scipy.integrate import solve_ivp
//! def lorenz(t, y): return np.array([10*(y[1]-y[0]), y[0]*(28-y[2])-y[1], y[0]*y[1]-8/3*y[2]])
//! f = lambda t, y: -y
//! def probe(r, times):
//!     print(r.nfev, len(r.t), len(r.sol.ts))
//!     for t in times: print(t, *[float(v) for v in r.sol(t)])
//! y0 = np.array([1., 1, 1])
//! probe(solve_ivp(lorenz, (0, 2), y0, dense_output=True), [0.0, 0.3, 1.234, 2.0, 2.5, -0.1])
//! probe(solve_ivp(lorenz, (0, 2), y0, 'DOP853', dense_output=True), [0.3, 1.234, 2.0])
//! probe(solve_ivp(f, (1, 0), [2.0], t_eval=[0.5], dense_output=True),
//!       [0.5, 0.9, 1.0, 0.0, -1.0, 3.0])
//! probe(solve_ivp(f, (1, 0), [2.0], 'DOP853', t_eval=[0.5], dense_output=True), [0.5, 0.9])
//! probe(solve_ivp(f, (1, 1), [2.0], dense_output=True), [1.0, 5.0])
//! ```

use super::{baryon_build, package, stderr};
use std::process::Command;

/// `SciPy`'s lines: for each run the calls of `fun`, the times reported and the times `sol`
/// holds, then a line for each time read, the time and the state there.
const SCIPY: &str = "\
158 23 23
0.0 1.0 1.0 1.0
0.3 16.700022731001297 27.2347911713027 26.18301050001915
1.234 -7.140587064673824 -7.137781960924997 25.40136747046069
2.0 -8.167936673944695 -9.517535065334059 24.6738540968915
2.5 32.011577607537134 64.45445994909859 13.169441767838812
-0.1 2.868453570282282 -2.7891868594467066 1.3772363233113216
248 15 15
0.3 16.684717480209603 27.184760980735568 26.205780764339657
1.234 -7.106485698383332 -7.116194383569244 25.33061364773012
2.0 -8.173944643666031 -9.564727886824468 24.617708987972527
14 1 3
0.5 3.297442555301937
0.9 2.2103418366669665
1.0 2.0
0.0 5.436654001176825
-1.0 14.57147585491602
3.0 0.7511587783041647
32 1 3
0.5 3.2974426337224783
0.9 2.2103418361637366
1 2 2
1.0 2.0
5.0 2.0";

/// Every number of `output` matches `SciPy`'s in `expected` to a relative 1e-8, the counts
/// exactly, line for line.
fn agree(expected: &str, output: &str) {
    assert_eq!(expected.lines().count(), output.lines().count(), "{output}");
    for (expected, found) in expected.lines().zip(output.lines()) {
        let expected: Vec<f64> = expected
            .split(' ')
            .map(|v| v.parse().unwrap_or(f64::NAN))
            .collect();
        let found: Vec<f64> = found
            .split(' ')
            .map(|v| v.parse().unwrap_or(f64::NAN))
            .collect();
        assert_eq!(expected.len(), found.len(), "{output}");
        for (want, got) in expected.into_iter().zip(found) {
            assert!(
                (want - got).abs() <= 1e-8 * want.abs(),
                "{want} {got}\n{output}"
            );
        }
    }
}

/// `sol(t)` gives `SciPy`'s state at each time, from the step that covers it, extrapolating
/// outside the run; without `dense_output`, `sol` is `None`.
#[test]
fn sol_gives_scipys_states() {
    let program = "\
from integrate import OdeResult, solve_ivp

Vec3 = Fixed[f64, (3,)]


def lorenz(t: f64, y: Vec3) -> Vec3:
    return Vec3(10.0 * (y[1] - y[0]), y[0] * (28.0 - y[2]) - y[1], y[0] * y[1] - 8.0 / 3.0 * y[2])


def decay(t: f64, y: f64) -> f64:
    return -y


def probe(r: Owned[OdeResult], times: list[f64]):
    counts = f\"{r.nfev} {len(r.t)}\"
    sol = r.sol
    if sol is not None:
        print(counts, len(sol.ts))
        for t in times:
            y = sol(t)
            line = f\"{t}\"
            for i in range(len(y)):
                line = line + f\" {y[i]}\"
            print(line)


def main():
    start = Vec3(1.0, 1.0, 1.0)
    times = [0.0, 0.3, 1.234, 2.0, 2.5, -0.1]
    probe(solve_ivp(lorenz, (0.0, 2.0), start, dense_output=True), times)
    probe(solve_ivp(lorenz, (0.0, 2.0), start, \"DOP853\", dense_output=True), [0.3, 1.234, 2.0])
    back = solve_ivp(decay, (1.0, 0.0), 2.0, t_eval=Array([0.5]), dense_output=True)
    probe(back, [0.5, 0.9, 1.0, 0.0, -1.0, 3.0])
    back = solve_ivp(decay, (1.0, 0.0), 2.0, \"DOP853\", t_eval=Array([0.5]), dense_output=True)
    probe(back, [0.5, 0.9])
    probe(solve_ivp(decay, (1.0, 1.0), 2.0, dense_output=True), [1.0, 5.0])
    plain = solve_ivp(decay, (0.0, 1.0), 2.0)
    print(plain.sol is None)
";
    let directory = package("solve-ivp-dense", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    let output = String::from_utf8_lossy(&ran.stdout);
    let (numbers, last) = output.trim_end().rsplit_once('\n').unwrap();
    agree(SCIPY, numbers);
    assert_eq!(last, "True");
}
