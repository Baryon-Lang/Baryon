//! `interpolate`'s `CubicSpline` and `PPoly` (roadmap 3.21.2), built and run: every value,
//! derivative, antiderivative and integral is `SciPy` 1.18's, to the last bit, for each
//! `bc_type`, for two and three points, inside and outside the breakpoints, with and without
//! extrapolation. The last line, three points with "not-a-knot", is compared to 12 decimals,
//! since `SciPy` solves that case with `OpenBLAS`'s fused multiply-adds. The expected lines were
//! printed by this script with `SciPy` 1.18.1 and `NumPy` 2:
//!
//! ```text
//! import numpy as np
//! from scipy.interpolate import CubicSpline, PPoly
//! x = np.array([0.0, 0.7, 1.5, 2.25, 3.0, 4.2])
//! y = np.array([1.0, -0.4, 2.2, 0.9, -1.3, 1.0])
//! for bc in ["not-a-knot", "clamped", "natural", "periodic"]:
//!     cs = CubicSpline(x, y, bc_type=bc)
//!     vals = [cs(t, nu) for t in (-0.6, 0.0, 1.1, 4.2, 5.3) for nu in (0, 1, 2, 3)]
//!     d = cs.derivative(); a = cs.antiderivative(2)
//!     line = " ".join(repr(float(v)) for v in vals)
//!     print(bc, line)
//!     print(bc, repr(float(cs.integrate(-1.0, 9.5))), repr(float(cs.integrate(3.0, 0.5))), repr(float(cs.integrate(-1.0, 9.5, extrapolate=False))),
//!           repr(float(d(2.0))), repr(float(a(2.0))), repr(float(a(6.0))), repr(float(d.integrate(0.2, 7.0))), repr(float(cs(5.3, 0, False))))
//! for xs, ys in [([0.0, 2.0], [1.0, 3.0]), ([1.0, 2.0, 4.0], [3.0, 1.0, 3.0])]:
//!     for bc in ["not-a-knot", "clamped", "natural", "periodic"]:
//!         if bc == "periodic" and ys[0] != ys[-1]: continue
//!         cs = CubicSpline(np.array(xs), np.array(ys), bc_type=bc)
//!         print(len(xs), bc, " ".join(repr(float(cs(t))) for t in (0.5, 1.5, 3.7)))
//! p = PPoly(np.array([[1.0, -2.0, 0.5], [0.0, 3.0, -1.0], [2.0, 1.0, 4.0]]), np.array([0.0, 1.0, 2.5, 3.0]), extrapolate=False)
//! print(" ".join(repr(float(v)) for v in (p(0.5), p(2.5), p(3.0), p(-0.1), p(1.7, 1), p(1.7, 2), p(1.7, 3), p(-0.1, 0, True))))
//! a = p.antiderivative(); d = p.derivative(2)
//! print(" ".join(repr(float(v)) for v in (a(2.9), a(1.0), d(2.0), p.integrate(0.25, 2.75), p.integrate(2.75, 0.25), p.integrate(0.0, 3.5), p.integrate(0.0, 3.5, True))))
//! print(a.c.shape[0], d.c.shape[0], p.derivative(5).c.shape[0], repr(float(p.derivative(5)(1.0))))
//! cs = CubicSpline(np.array([0.0, 1.0, 3.0]), np.array([0.0, 2.0, 1.0]))
//! print(" ".join(f"{float(v):.12f}" for v in (cs(0.5), cs(2.0), cs(2.0, 1), cs.integrate(0.0, 3.0))))
//! ```

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// The program the expected lines come from, the script's steps in Baryon.
const SPLINES: &str = "\
from interpolate import CubicSpline, PPoly


def main():
    x = Array([0.0, 0.7, 1.5, 2.25, 3.0, 4.2])
    y = Array([1.0, -0.4, 2.2, 0.9, -1.3, 1.0])
    for bc in [\"not-a-knot\", \"clamped\", \"natural\", \"periodic\"]:
        cs = CubicSpline(x, y, bc_type=bc)
        line = bc.copy()
        for t in [-0.6, 0.0, 1.1, 4.2, 5.3]:
            for nu in range(4):
                line += f\" {cs(t, nu)}\"
        print(line)
        d = cs.derivative()
        a = cs.antiderivative(2)
        print(
            f\"{bc} {cs.integrate(-1.0, 9.5)} {cs.integrate(3.0, 0.5)} \"
            + f\"{cs.integrate(-1.0, 9.5, extrapolate=False)} {d(2.0)} {a(2.0)} {a(6.0)} \"
            + f\"{d.integrate(0.2, 7.0)} {cs(5.3, 0, False)}\"
        )
    two = Array([0.0, 2.0])
    for bc in [\"not-a-knot\", \"clamped\", \"natural\"]:
        cs = CubicSpline(two, Array([1.0, 3.0]), bc_type=bc)
        print(f\"2 {bc} {cs(0.5)} {cs(1.5)} {cs(3.7)}\")
    three = Array([1.0, 2.0, 4.0])
    for bc in [\"not-a-knot\", \"clamped\", \"natural\", \"periodic\"]:
        cs = CubicSpline(three, Array([3.0, 1.0, 3.0]), bc_type=bc)
        print(f\"3 {bc} {cs(0.5)} {cs(1.5)} {cs(3.7)}\")
    values = [1.0, -2.0, 0.5, 0.0, 3.0, -1.0, 2.0, 1.0, 4.0]
    c: Array[f64, 2] = Array.zeros((3, 3))
    for i in range(9):
        c[i // 3, i % 3] = values[i]
    p = PPoly(c, Array([0.0, 1.0, 2.5, 3.0]), extrapolate=False)
    print(
        f\"{p(0.5)} {p(2.5)} {p(3.0)} {p(-0.1)} {p(1.7, 1)} {p(1.7, 2)} {p(1.7, 3)} \"
        + f\"{p(-0.1, 0, True)}\"
    )
    a = p.antiderivative()
    d = p.derivative(2)
    print(
        f\"{a(2.9)} {a(1.0)} {d(2.0)} {p.integrate(0.25, 2.75)} {p.integrate(2.75, 0.25)} \"
        + f\"{p.integrate(0.0, 3.5)} {p.integrate(0.0, 3.5, True)}\"
    )
    gone = p.derivative(5)
    print(f\"{a.c.shape[0]} {d.c.shape[0]} {gone.c.shape[0]} {gone(1.0)}\")
    cs = CubicSpline(Array([0.0, 1.0, 3.0]), Array([0.0, 2.0, 1.0]))
    print(f\"{cs(0.5):.12f} {cs(2.0):.12f} {cs(2.0, 1):.12f} {cs.integrate(0.0, 3.0):.12f}\")
";

/// [`SPLINES`] prints what `SciPy` prints.
#[test]
fn cubic_splines_give_scipys_bits() {
    let expected = concat!(
        "not-a-knot 11.528632911392403 -27.865799137571287 39.227755366995865 -24.1708165252469 1.0 -8.679892891918207 24.725265451847726 -24.1708165252469 1.0490106180739098 3.8945551073399174 -1.8626327259238682 -24.170816525246906 1.0 7.287128947002366 11.397792924375207 6.117556142872604 17.268584431976834 23.52582263025301 18.12710468153507 6.117556142872604\n",
        "not-a-knot 602.4685143164971 -1.8399389595987694 nan -2.716390009427985 0.4748824657527191 18.79882022447489 88.739109081261 nan\n",
        "clamped -4.054884897959184 19.94678053375196 -48.730455259026684 51.619403453689166 1.0 0.0 -17.758813186813185 51.619403453689166 0.7246661538461545 4.328873076923077 2.191673076923074 -40.457740384615384 1.0 -1.7763568394002505e-15 -10.25778846153846 -17.65836004273504 -9.123174888710817 -21.966875133547003 -29.681984508547 -17.65836004273504\n",
        "clamped -832.1664957138789 -1.686997641532575 nan -2.9674188034188034 1.0918251058582622 2.6314112397836595 -104.53023365311935 nan\n",
        "natural 2.459331852614863 -0.03838726868244713 -11.96916242837829 19.948604047297152 1.0 -3.629135997195934 8.881784197001252e-16 19.948604047297152 0.8610425116418812 4.148470281908768 0.48696860447649115 -33.69263557157878 0.9999999999999996 3.4501001452105555 0.0 -6.389306160599535 3.3777490764386133 -0.4154300819521559 -7.028236776659487 -6.389306160599535\n",
        "natural -153.19993663940193 -1.7550044841806343 nan -2.860393403768798 0.8339749803186846 10.395607273358873 -13.016832005614475 nan\n",
        "periodic -0.03946407354136705 3.165063334236619 -0.6140884803257389 -20.80661112616587 1.0 -0.948579756668682 -13.098055156025264 43.25998850543908 0.7568011172951459 4.276263384264761 1.7899860338106812 -38.4848769099285 1.0 -0.948579756668682 -13.098055156025264 43.25998850543908 0.756801117295144 4.276263384264759 1.7899860338106972 -38.4848769099285\n",
        "periodic 3.9621477337561 -1.6861415976298182 nan -2.9489336594426963 1.0239753595579615 nan -1.6508686174774019 nan\n",
        "2 not-a-knot 1.5 2.5 4.7\n",
        "2 clamped 1.3125 2.6875 -3.791499999999999\n",
        "2 natural 1.5 2.5 4.7\n",
        "3 not-a-knot 4.75 1.75 2.1900000000000004\n",
        "3 clamped 1.5625 2.1875 2.8211250000000003\n",
        "3 natural 4.1875 1.8125 2.4067500000000006\n",
        "3 periodic 2.875 2.0 3.0570000000000004\n",
        "2.25 4.0 3.625 nan 0.20000000000000018 -4.0 0.0 2.01\n",
        "6.489 2.3333333333333335 -4.0 5.424479166666667 -5.424479166666667 nan 8.625\n",
        "4 1 1 0.0\n",
        "1.208333333333 2.333333333333 -0.500000000000 5.250000000000\n",
    );
    prints("interpolate-splines", SPLINES, expected).unwrap();
}

/// Input that does not fit panics with `SciPy`'s message, where `SciPy` raises `ValueError`:
/// each line is a statement of `main` and the report it stops with.
const BAD_INPUT: [(&str, &str); 8] = [
    (
        "print(CubicSpline(Array([0.0, 1.0]), Array([1.0, 2.0]), bc_type=\"cubic\")(0.5))",
        "panic: assertion failed: known or periodic, f\"bc_type={bc_type} is not allowed.\"\n  \
         --> <std>/interpolate_cubic.byn:57:9\n",
    ),
    (
        "print(CubicSpline(Array([0.0, 1.0, 2.0]), Array([1.0, 2.0, 3.0]), bc_type=\"periodic\")(0.5))",
        "panic: assertion failed: same or not periodic, ( \"The first and last `y` point along axis 0 \
         must be identical (within machine \" \"precision) when bc_type='periodic'.\" )\n  \
         --> <std>/interpolate_cubic.byn:60:9\n",
    ),
    (
        "print(CubicSpline(Array([0.0, 2.0, 1.0]), Array([1.0, 2.0, 3.0]))(0.5))",
        "panic: assertion failed: x[i + 1] - x[i] > 0.0, \"`x` must be strictly increasing sequence.\"\n  \
         --> <std>/interpolate_cubic.byn:86:9\n",
    ),
    (
        "print(CubicSpline(Array([0.0]), Array([1.0]))(0.5))",
        "panic: assertion failed: n >= 2, \"`x` must contain at least 2 elements.\"\n  \
         --> <std>/interpolate_cubic.byn:79:5\n",
    ),
    (
        "print(CubicSpline(Array([0.0, 1.0]), Array([1.0, 2.0, 3.0]))(0.5))",
        "panic: assertion failed: len(y) == n, \"The length of `y` along `axis`=0 doesn't match the \
         length of `x`\"\n  \
         --> <std>/interpolate_cubic.byn:80:5\n",
    ),
    (
        "print(CubicSpline(Array([0.0, 1.0]), Array([1.0, nan]))(0.5))",
        "panic: assertion failed: isfinite(y[i]), \"`y` must contain only finite values.\"\n  \
         --> <std>/interpolate_cubic.byn:84:9\n",
    ),
    (
        "print(PPoly(Array.zeros((4, 3)), Array([0.0, 1.0, 2.0]))(0.5))",
        "panic: assertion failed: c.shape[1] == len(x) - 1, \"number of coefficients != len(x)-1\"\n  \
         --> <std>/interpolate_ppoly.byn:46:9\n",
    ),
    (
        "print(CubicSpline(Array([0.0, 1.0]), Array([1.0, 2.0]))(0.5, -1))",
        "panic: assertion failed: nu >= 0, \"Order of derivative cannot be negative\"\n  \
         --> <std>/interpolate_ppoly.byn:62:9\n",
    ),
];

/// Each line of [`BAD_INPUT`] stops its program with status 101 and its report.
#[test]
fn input_that_does_not_fit_panics() {
    for (index, (statement, report)) in BAD_INPUT.iter().enumerate() {
        let program = format!(
            "from math import nan\nfrom interpolate import CubicSpline, PPoly\n\n\ndef main():\n    \
             {statement}\n"
        );
        let name = format!("interpolate-panic-{index}");
        let directory = package(&name, &[("main.byn", &program)]).unwrap();
        let (output, executable) = baryon_build(&directory).unwrap();
        assert!(output.status.success(), "{}", stderr(&output));
        let run = Command::new(&executable).output().unwrap();
        assert_eq!(run.status.code(), Some(101), "{statement}");
        assert_eq!(panic_report(&run), *report, "{statement}");
    }
}
