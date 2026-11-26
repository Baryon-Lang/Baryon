//! `integrate` validated against the true values (roadmap 3.19.10): `quad` on seven integrals
//! that test its rules, endpoints singular, oscillating and infinite, and `solve_ivp` on the van
//! der Pol oscillator, each against a value computed by `mpmath` 1.4.1 at 40 digits. Other tests
//! check that Baryon matches `SciPy`'s steps; these check that the answers are right.
//!
//! The true values were printed by:
//!
//! ```text
//! import mpmath as mp
//! mp.mp.dps = 40
//! mp.quad(lambda x: mp.exp(-x) * mp.sin(x)**2, [0, 10])  # and the six below
//! f = lambda t, y: [y[1], 2 * (1 - y[0]**2) * y[1] - y[0]]
//! sol = mp.odefun(f, 0, [mp.mpf(1), mp.mpf(0)])
//! [sol(t) for t in (1, 5, 10)]
//! ```

use super::{baryon_build, package, stderr};
use std::process::Command;

/// The true value of each integral the program computes, in its order, from `mpmath`.
const INTEGRALS: [f64; 7] = [
    0.399_970_863_194_761_1,     // exp(-x) sin(x)**2 on [0, 10]
    -0.444_444_444_444_444_4,    // sqrt(x) log(x) on [0, 1], -4/9
    std::f64::consts::FRAC_PI_2, // 1 / (1 + x**2) on [0, inf), pi/2
    1.772_453_850_905_516,       // exp(-x**2) on (-inf, inf), sqrt(pi)
    2.0,                         // x**-0.5 on [0, 1]
    -0.005_063_656_411_097_588,  // cos(100 x) on [0, 1], sin(100)/100
    -0.822_467_033_424_113_2,    // log(x) / (1 + x) on [0, 1], -pi**2/12
];

/// The van der Pol state `(x, dx/dt)` with mu = 2 from (1, 0) at t = 1, 5 and 10, from `mpmath`.
const VAN_DER_POL: [f64; 6] = [
    0.444_490_859_017_714_07,
    -1.328_471_476_419_86,
    0.196_335_072_398_210_63,
    2.995_784_241_011_366,
    -1.946_825_068_090_138,
    0.300_788_299_663_925_06,
];

/// Each `quad` value is within its own error estimate of the true value, and within `SciPy`'s
/// default `epsrel`, 1.49e-8. Each `solve_ivp` state is within 1e-5 of the true state with RK45
/// at `rtol=1e-6`, and within 1e-9 with DOP853 at `rtol=1e-10`, calling the right-hand side as
/// many times as `SciPy` does: 956 and 2027.
#[test]
fn answers_are_right() {
    let program = "\
from math import cos, exp, inf, log, sqrt, sin
from integrate import IntegrationError, quad, solve_ivp

Vec2 = Fixed[f64, (2,)]


def van_der_pol(t: f64, y: Vec2) -> Vec2:
    return Vec2(y[1], 2.0 * (1.0 - y[0] * y[0]) * y[1] - y[0])


def damped(x: f64) -> f64:
    return exp(-x) * sin(x) * sin(x)


def root_log(x: f64) -> f64:
    return sqrt(x) * log(x)


def lorentzian(x: f64) -> f64:
    return 1.0 / (1.0 + x * x)


def gaussian(x: f64) -> f64:
    return exp(-x * x)


def inverse_root(x: f64) -> f64:
    return 1.0 / sqrt(x)


def wave(x: f64) -> f64:
    return cos(100.0 * x)


def log_ratio(x: f64) -> f64:
    return log(x) / (1.0 + x)


def show(value: f64, error: f64):
    print(value, error)


def main() -> Raises[None, IntegrationError]:
    v, e = quad(damped, 0.0, 10.0)
    show(v, e)
    v, e = quad(root_log, 0.0, 1.0)
    show(v, e)
    v, e = quad(lorentzian, 0.0, inf)
    show(v, e)
    v, e = quad(gaussian, -inf, inf)
    show(v, e)
    v, e = quad(inverse_root, 0.0, 1.0)
    show(v, e)
    v, e = quad(wave, 0.0, 1.0)
    show(v, e)
    v, e = quad(log_ratio, 0.0, 1.0)
    show(v, e)
    times = Array([1.0, 5.0, 10.0])
    for method in [\"RK45\", \"DOP853\"]:
        rtol = 1e-6
        atol = 1e-9
        if method == \"DOP853\":
            rtol = 1e-10
            atol = 1e-12
        try:
            start = Vec2(1.0, 0.0)
            r = solve_ivp(van_der_pol, (0.0, 10.0), start, method, times, rtol=rtol, atol=atol)
            print(r.nfev, r.y[0, 0], r.y[1, 0], r.y[0, 1], r.y[1, 1], r.y[0, 2], r.y[1, 2])
        except ValueError as error:
            print(error.message)
";
    let directory = package("integrate-validation", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    let output = String::from_utf8_lossy(&ran.stdout);
    let lines: Vec<Vec<f64>> = output
        .lines()
        .map(|line| {
            line.split(' ')
                .map(|v| v.parse().unwrap_or(f64::NAN))
                .collect()
        })
        .collect();
    assert_eq!(lines.len(), 9, "{output}");
    for (line, exact) in lines.iter().zip(INTEGRALS) {
        let (value, error) = (line[0], line[1]);
        let off = (value - exact).abs();
        assert!(
            off <= error + 1e-15 * exact.abs(),
            "{value} {exact} {error}"
        );
        assert!(off <= 1.49e-8 * exact.abs(), "{value} {exact}");
    }
    for (line, (calls, tolerance)) in lines[7..].iter().zip([(956.0, 1e-5), (2027.0, 1e-9)]) {
        assert!((line[0] - calls).abs() < 0.5, "{output}");
        for (found, exact) in line[1..].iter().zip(VAN_DER_POL) {
            assert!(
                (found - exact).abs() <= tolerance,
                "{found} {exact}\n{output}"
            );
        }
    }
}
