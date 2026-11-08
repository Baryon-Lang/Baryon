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
