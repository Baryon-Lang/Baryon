//! Array slices, views of an array's memory (roadmap 1.9.16), built and run: read, passed to
//! read-only and `InOut` parameters, used in whole-array arithmetic, assigned into, and stopped
//! by the range check.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds and runs `main.byn` holding `program` as the package `name`; its standard output and
/// its standard error before any backtrace, and whether it succeeded.
///
/// # Panics
///
/// When the program does not build.
fn run(name: &str, program: &str) -> io::Result<(String, String, bool)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&program).output()?;
    Ok((
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
        run.status.success(),
    ))
}

/// The helpers every program below calls: a sum in a loop, which reads elements through the
/// view's strides, and a length.
const HELPERS: &str = "\
def total(a: Array[f64, 1]) -> f64:
    s = 0.0
    for i in range(len(a)):
        s += a[i]
    return s


def count(a: Array[f64, 1]) -> int:
    return len(a)


def scale(a: InOut[Array[f64, 1]], k: f64):
    for i in range(len(a)):
        a[i] = a[i] * k


def grow(a: InOut[Array[f64, 1]]):
    a = Array.zeros(5)


";

/// Every form of range Python has, read through a borrowed parameter and through `a.sum()`:
/// defaults, steps forwards and backwards, a step known only at run time, negative literals
/// counting from the end, empty ranges, a column and a row of a matrix, and slices of a `Fixed`
/// array.
#[test]
fn slices_read_every_range() {
    let program = format!(
        "{HELPERS}\
Mat23 = Fixed[f64, (2, 3)]


def main():
    a = Array.zeros(10)
    for i in range(10):
        a[i] = f64(i)
    k = -2
    print(total(a[2:5]), total(a[1:9:2]), total(a[::-1]), total(a[::k]))
    print(count(a[8:2:k]), count(a[2:8:-1]), count(a[5:5]), count(a[0:0:k]))
    print(total(a[-3:]), count(a[:-1]))
    print(a[1:9:3].sum(), a[::2].sum(), a[::-1].sum())
    m = Array.zeros((3, 4))
    for i in range(3):
        for j in range(4):
            m[i, j] = f64(10 * i + j)
    print(total(m[:, 1]), total(m[2, :]), m[1:, ::2].sum())
    f = Mat23((1.0, 2.0, 3.0), (4.0, 5.0, 6.0))
    print(total(f[1, :]), total(f[:, 2]), total(f[0, ::-1]))
    b = a[2:5].copy()
    print(b[0], len(b))
"
    );
    let (stdout, stderr, success) = run("slices-read", &program).unwrap();
    assert_eq!(stderr, "");
    assert!(success);
    assert_eq!(
        stdout,
        "\
9.0 16.0 45.0 25.0
3 0 0 0
24.0 9
12.0 20.0 45.0
33.0 86.0 64.0
15.0 9.0 6.0
2.0 3
"
    );
}

/// Writing through views: `=` of a number and of a slice, `+=`, a slice passed `InOut`, a
/// whole-array expression of slices into a new array, and an assignment between overlapping
/// slices of one array, which reads every element before any is written, as `NumPy` does.
#[test]
fn slices_are_written_in_place() {
    let program = format!(
        "{HELPERS}\
def main():
    a = Array.zeros(10)
    for i in range(10):
        a[i] = f64(i)
    scale(a[0:3], 10.0)
    print(a[0], a[1], a[2], a[3])
    a[5:7] = 0.5
    a[7:9] += a[0:2]
    print(a[5], a[6], a[7], a[8])
    c = a[1:4] - a[0:3]
    print(c[0], c[1], c[2])
    a[1:10] = a[0:9]
    print(a[0], a[1], a[2], a[9])
    m = Array.zeros((2, 3))
    m[:, 1] = 7.0
    m[1, :] += 1.0
    print(m[0, 0], m[0, 1], m[1, 0], m[1, 1])
"
    );
    let (stdout, stderr, success) = run("slices-write", &program).unwrap();
    assert_eq!(stderr, "");
    assert!(success);
    assert_eq!(
        stdout,
        "\
0.0 10.0 20.0 3.0
0.5 0.5 7.0 18.0
10.0 10.0 -17.0
0.0 0.0 10.0 18.0
0.0 7.0 1.0 8.0
"
    );
}

/// Each way a slice fails panics with what went wrong: a bound past the end, a negative
/// literal that counts back past the start, a reversed range starting at the length, a step of
/// zero known only at run time, slices of different lengths, and a callee that gives an `InOut`
/// slice a new array of another length.
#[test]
fn slices_out_of_range_panic() {
    let cases = [
        (
            "print(a[2:12].sum())",
            "panic: slice 2:12 is out of range for dimension 0 of length 10\n  --> main.byn:23:13\n",
        ),
        (
            "print(a[-12:].sum())",
            "panic: slice -2:10 is out of range for dimension 0 of length 10\n  --> main.byn:23:13\n",
        ),
        (
            "print(a[10::-1].sum())",
            "panic: slice 10:-1 is out of range for dimension 0 of length 10\n  --> main.byn:23:13\n",
        ),
        (
            "print(a[::len(a) - 10].sum())",
            "panic: slice step cannot be zero\n  --> main.byn:23:13\n",
        ),
        (
            "a[0:3] = a[0:4]",
            "panic: array lengths differ in dimension 0: 3 and 4\n  --> main.byn:23:14\n",
        ),
        (
            "grow(a[0:3])",
            "panic: array lengths differ in dimension 0: 3 and 5\n  --> main.byn:23:5\n",
        ),
    ];
    for (position, (line, report)) in cases.into_iter().enumerate() {
        let program = format!("{HELPERS}def main():\n    a = Array.zeros(10)\n    {line}\n");
        let (stdout, stderr, success) = run(&format!("slices-panic-{position}"), &program).unwrap();
        assert_eq!(
            (stdout.as_str(), stderr.as_str(), success),
            ("", report, false),
            "{line}"
        );
    }
}

/// Adding into a slice of an array a `prange` loop accumulates into (roadmap 1.12.7): a row
/// of a matrix from an array and from an expression, and a range of a vector from a number.
/// Each element is added as `forces[j] += f` adds one, so the result is the same on any number
/// of threads.
#[test]
fn slices_of_accumulated_arrays_are_added_into() {
    let program = "\
def main():
    n = 6
    forces = Array.zeros((n, 3))
    f = Array.ones(3)
    for i in prange(n - 1, accumulate=forces):
        forces[i, :] += f
        forces[i + 1, :] -= 0.5 * f
    print(forces[0, 0], forces[1, 0], forces[5, 2])
    g = Array.zeros(10)
    for i in prange(4, accumulate=g):
        g[i:i + 3] += 1.0
    print(g[0], g[1], g[2], g[3], g[5], g[6])
";
    let (stdout, stderr, success) = run("slices-accumulate", program).unwrap();
    assert_eq!(stderr, "");
    assert!(success);
    assert_eq!(stdout, "1.0 0.5 -0.5\n1.0 2.0 3.0 3.0 1.0 0.0\n");
}
