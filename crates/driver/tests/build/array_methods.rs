//! Array methods and the matrix product built and run (roadmap 1.14.2): `a.min()`, `a.max()`,
//! `a.dot(b)`, `a.round()` and `a.fill(value)` on `Array` and `Fixed` arrays, `@` on matrices
//! and vectors, and the panics for an empty array and for lengths that do not match.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds the program `text` as the `main.byn` of the package `name`, runs it, and returns
/// what it printed to standard output, its exit status, and its panic report, if any.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
fn run(name: &str, text: &str) -> io::Result<(String, Option<i32>, String)> {
    let directory = package(name, &[("main.byn", text)])?;
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output()?;
    Ok((
        String::from_utf8_lossy(&ran.stdout).into_owned(),
        ran.status.code(),
        panic_report(&ran),
    ))
}

/// `min` and `max` of float, integer and `Fixed` arrays; a NaN anywhere makes the result NaN,
/// as `numpy.min` gives. `round` rounds ties to even and leaves the original alone, and `fill`
/// writes every element of an `Array` and of a `Fixed` vector.
#[test]
fn min_max_round_and_fill_build_and_run() {
    let program = "\
from math import nan

Vec3 = Fixed[f64, (3,)]


def main():
    a = Array.zeros((2, 3))
    a[0, 1] = -2.5
    a[1, 2] = 7.25
    print(a.min(), a.max())
    counts = Array.fill((4,), 3)
    counts[2] = -8
    print(counts.min(), counts.max())
    r = Array.zeros((4,))
    r[0] = 2.5
    r[1] = 3.5
    r[2] = -1.5
    r[3] = 0.75
    rounded = r.round()
    print(rounded[0], rounded[1], rounded[2], rounded[3], r[0])
    print(counts.round().sum())
    a.fill(1.5)
    print(a.sum())
    v = Vec3(3.0, -1.0, 2.0)
    print(v.min(), v.max())
    v.fill(2.0)
    print(v.sum())
    a[1, 0] = nan
    print(a.min(), a.max())
";
    let (printed, status, report) = run("array-min-max", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(
        printed,
        "-2.5 7.25\n-8 3\n2.0 4.0 -2.0 1.0 2.5\n1\n9.0\n-1.0 3.0\n6.0\nnan nan\n"
    );
    assert_eq!(status, Some(0));
}

/// `a.dot(b)` of float `Array`s adds the products in `NumPy`'s pairwise order:
/// `numpy.sum(a * b)` of these arrays is `19.315074487579345`, where adding one product at a
/// time gives `19.31507448757933`. `f32` keeps `f32` arithmetic, integers are exact, and a
/// short `Fixed` vector adds in a loop with the same bits.
#[test]
fn dot_products_match_numpy() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    a = Array.zeros((1000,))
    b = Array.zeros((1000,))
    for i in range(1000):
        a[i] = 1.0 / f64(i + 1)
        b[i] = f64(i % 7) + 0.1
    c: Array[f32, 1] = Array.zeros((300,))
    d: Array[f32, 1] = Array.zeros((300,))
    for i in range(300):
        c[i] = f32(1.0) / f32(i + 1)
        d[i] = f32(i % 5) + f32(0.5)
    n = Array.fill((3,), 4)
    v = Vec3(1.0, 2.0, 3.0)
    print(a.dot(b), c.dot(d), n.dot(n), v.dot(v))
";
    let (printed, status, report) = run("array-dot", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(printed, "19.315074487579345 13.50931 48 14.0\n");
    assert_eq!(status, Some(0));
}

/// `@` of a matrix and a matrix, a matrix and a vector, and a vector and a matrix, on
/// `Array`s and on `Fixed` arrays; inside a larger expression; with integer elements; and
/// assigned back to an operand, which reads the old value throughout.
#[test]
fn matrix_products_build_and_run() {
    let program = "\
Mat2 = Fixed[f64, (2, 2)]
Vec2 = Fixed[f64, (2,)]


def main():
    m = Array.zeros((2, 3))
    k = Array.zeros((3, 2))
    for i in range(2):
        for j in range(3):
            m[i, j] = f64(i * 3 + j + 1)
            k[j, i] = f64(j - i)
    p = m @ k
    print(p.shape[0], p.shape[1], p[0, 0], p[0, 1], p[1, 0], p[1, 1])
    x = Array.ones((3,))
    y = m @ x
    print(len(y), y[0], y[1])
    z = y @ m
    print(len(z), z[0], z[1], z[2])
    r = Array.fill((2,), 100.0) - m @ x
    print(r[0], r[1])
    q = Array.fill((2, 2), 2)
    w = Array.fill((2,), 3)
    w = q @ w
    print(w[0], w[1])
    rotate = Mat2((0.0, -1.0), (1.0, 0.0))
    u = Vec2(1.0, 2.0)
    u = rotate @ u
    print(u[0], u[1], (rotate @ rotate)[0, 0], (u @ rotate)[1])
";
    let (printed, status, report) = run("array-matmul", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(
        printed,
        "2 2 8.0 2.0 17.0 2.0\n2 6.0 15.0\n3 66.0 87.0 108.0\n94.0 85.0\n12 12\n-2.0 1.0 -1.0 2.0\n"
    );
    assert_eq!(status, Some(0));
}

/// Each array method or product that cannot give a value panics with what went wrong and
/// where: `min` of an empty array, a dot product of different lengths, and `@` of a matrix
/// whose columns do not match the vector's length.
#[test]
fn empty_arrays_and_mismatched_lengths_panic() {
    let cases = [
        (
            "    print(Array.zeros((0,)).min())\n",
            "panic: `Array.zeros((0,)).min()` of an empty array\n  --> main.byn:2:11\n",
        ),
        (
            "    print(Array.ones((3,)).dot(Array.ones((4,))))\n",
            "panic: array lengths differ in dimension 0: 3 and 4\n  --> main.byn:2:11\n",
        ),
        (
            "    print((Array.ones((2, 3)) @ Array.ones((2,)))[0])\n",
            "panic: array lengths differ in dimension 1: 3 and 2\n  --> main.byn:2:12\n",
        ),
    ];
    for (index, (line, expected)) in cases.into_iter().enumerate() {
        let program = format!("def main():\n{line}");
        let (printed, status, report) = run(&format!("array-panic-{index}"), &program).unwrap();
        assert_eq!(printed, "");
        assert_eq!(status, Some(101), "{report}");
        assert_eq!(report, expected);
    }
}
