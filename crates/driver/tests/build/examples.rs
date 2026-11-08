//! The example programs in `docs/examples` built and run, each compared with what it prints, so
//! an example that builds today keeps building. Roadmap item 1.9.12 adds each example here as
//! lowering learns what it needs; `docs/reference/status.md` lists the ones still missing.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Builds the program `text` as the `main.byn` of the package `name`, runs it, and checks that
/// it prints `expected`, nothing else, and exits with status 0.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
fn prints(name: &str, text: &str, expected: &str) -> io::Result<()> {
    let directory = package(name, &[("main.byn", text)])?;
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output()?;
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected);
    assert_eq!(ran.status.code(), Some(0));
    Ok(())
}

/// The 3D heat equation: a stencil in a `prange` loop, `copy()`, a swap by unpacking and
/// `sum()`. `NumPy`, running the same steps, prints the same total.
#[test]
fn heat_equation_builds_and_runs() {
    prints(
        "example-heat-equation",
        include_str!("../../../../docs/examples/heat_equation.byn"),
        "heat remaining after 100 steps: 1000.000000\n",
    )
    .unwrap();
}

/// Unpacking a tuple, a tuple display and a `Fixed` vector; a copy that changes apart from its
/// original; and sums of a 2D array, an integer array, a `Fixed` vector and an empty array.
#[test]
fn unpacking_copy_and_sum_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    a = Array.zeros((2, 3))
    a[0, 0] = 1.0
    a[1, 2] = 4.5
    b = a.copy()
    b[0, 1] = 10.0
    print(a.sum(), b.sum())
    short = Array.ones((2,))
    a_row = Array.zeros((5,))
    short, a_row = a_row, short
    print(len(short), len(a_row))
    x, y = 3, 4.5
    print(x, y)
    p, q, r = Vec3(1.0, 2.0, 3.0)
    print(p, q, r, Vec3(1.0, 2.0, 3.0).sum())
    pair = (Array.fill((4,), 2), 7)
    twos, n = pair
    print(twos.sum(), n, Array.zeros((0,)).sum())
";
    prints(
        "unpacking-copy-sum",
        program,
        "5.5 15.5\n5 2\n3 4.5\n1.0 2.0 3.0 6.0\n8 7 0.0\n",
    )
    .unwrap();
}

/// `**` on floats, `**=`, and `dot` and `round` of a `Fixed` vector, through `llvm.pow` and
/// `llvm.roundeven`: ties round to even, so -1.25 gives -1.0 and 0.75 gives 1.0.
#[test]
fn powers_dot_and_round_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    v = Vec3(1.5, -2.5, 3.0)
    w = (v / 2.0).round()
    x = 2.0
    print(x ** 3, 2.0 ** 0.5, v.dot(v), w[0], w[1], w[2], Vec3(2.5, 3.5, -0.5).round()[0])
    x **= 2.0
    print(x)
";
    prints(
        "powers-dot-round",
        program,
        "8.0 1.4142135623730951 17.5 1.0 -1.0 2.0 2.0\n4.0\n",
    )
    .unwrap();
}

/// Lennard-Jones molecular dynamics: `Fixed` vector arithmetic, `dot`, `round`, a `prange`
/// force loop and `math.sqrt`. An independent `NumPy` run of the same steps prints the same
/// energies, and the total stays constant to four digits, the standard check of the integrator.
#[test]
fn lennard_jones_builds_and_runs() {
    prints(
        "example-lennard-jones",
        include_str!("../../../../docs/examples/lennard_jones.byn"),
        "\
step   50  kinetic    10.8151  potential -1991.7744  total -1980.9594  temperature 0.0141  rms speed 0.2055
step  100  kinetic     3.6545  potential -1984.6125  total -1980.9581  temperature 0.0048  rms speed 0.1195
step  150  kinetic     7.8174  potential -1988.7601  total -1980.9426  temperature 0.0102  rms speed 0.1747
step  200  kinetic     6.0588  potential -1987.0009  total -1980.9420  temperature 0.0079  rms speed 0.1538
",
    )
    .unwrap();
}

/// The `math` functions, called as `math.exp` and imported as `sqrt`, give the bits Python's
/// `math` module gives. An integer argument converts to `f64`, and an `f32` stays an `f32`.
#[test]
fn math_functions_build_and_run() {
    let program = "\
import math
from math import sqrt


def main():
    n = 2
    x: f32 = 2.0
    print(sqrt(n), math.exp(1.0), math.log(10), math.sin(0.5), math.cos(0.5))
    print(math.pow(2, 10), math.pow(x, 0.5), sqrt(x), math.log(0.0), math.sqrt(-1.0))
";
    prints(
        "math-functions",
        program,
        "1.4142135623730951 2.718281828459045 2.302585092994046 0.479425538604203 \
         0.8775825618903728\n1024.0 1.4142135 1.4142135 -inf nan\n",
    )
    .unwrap();
}

/// The built-in `round` gives an `int`, rounding ties to even as Python does, so 2.5 gives 2
/// and -0.5 gives 0. An integer is returned as it is.
#[test]
fn round_builds_and_runs() {
    let program = "\
def main():
    print(round(2.5), round(3.5), round(-0.5), round(-1.5), round(7), round(10.0 / 0.001))
";
    prints("round", program, "2 4 0 -2 7 10000\n").unwrap();
}

/// `a.sum()` of floats adds in `NumPy`'s pairwise tree, so it prints the bits `numpy.sum` gives:
/// the harmonic sum to 1000 is `7.485470860550345` both ways, where adding one element at a time
/// gives `7.485470860550343`. `f32` sums keep `f32` arithmetic, as `NumPy` does.
#[test]
fn float_sums_match_numpy() {
    let program = "\
def main():
    a = Array.zeros((1000,))
    b: Array[f32, 1] = Array.zeros((300,))
    for i in range(1000):
        a[i] = 1.0 / f64(i + 1)
    for i in range(300):
        b[i] = f32(1.0) / f32(i + 1)
    print(a.sum(), b.sum())
";
    prints("float-sums", program, "7.485470860550345 6.282665\n").unwrap();
}
