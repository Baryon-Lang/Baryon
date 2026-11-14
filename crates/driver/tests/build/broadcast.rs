//! Broadcasting (roadmap 3.9), built and run: whole-array arithmetic on arrays of different
//! ranks and of length 1 prints what `NumPy` prints for the same program, and shapes that do not
//! broadcast panic with the dimension and both lengths.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    Ok((
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
    ))
}

/// A matrix minus a row, a column plus a row, a fused temporary plus a column, a vector times a
/// length-1 array, `+=` and a slice assignment of a row into every row, and an update that grows
/// its array and one that keeps it: every line is what `NumPy` 2.5 prints for the same steps.
#[test]
fn broadcasting_prints_what_numpy_prints() {
    let program = "\
def update(y: Owned[Array[f64, 1]], b: Array[f64, 1]) -> Array[f64, 1]:
    y = y * 2.0 + b
    return y


def main():
    grid = Array.zeros((3, 4))
    for i in range(3):
        for j in range(4):
            grid[i, j] = 10.0 * i + j
    row = Array([0.5, 1.5, 2.5, 3.5])
    column = Array.zeros((3, 1))
    for i in range(3):
        column[i, 0] = 100.0 * (i + 1)
    print(grid - row)
    print(column + row)
    t = grid * 0.5
    print(t + column)
    one = Array([2.0])
    print(row * one)
    grid += row
    print(grid)
    grid[1:3, :] = row
    print(grid)
    print(update(Array([1.0]), row))
    print(update(Array([1.0, 1.0, 1.0, 1.0]), one))
";
    let (code, stdout, report) = run("broadcast-numpy", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
[[-0.5 -0.5 -0.5 -0.5]
 [ 9.5  9.5  9.5  9.5]
 [19.5 19.5 19.5 19.5]]
[[100.5 101.5 102.5 103.5]
 [200.5 201.5 202.5 203.5]
 [300.5 301.5 302.5 303.5]]
[[100.  100.5 101.  101.5]
 [205.  205.5 206.  206.5]
 [310.  310.5 311.  311.5]]
[1. 3. 5. 7.]
[[ 0.5  2.5  4.5  6.5]
 [10.5 12.5 14.5 16.5]
 [20.5 22.5 24.5 26.5]]
[[0.5 2.5 4.5 6.5]
 [0.5 1.5 2.5 3.5]
 [0.5 1.5 2.5 3.5]]
[2.5 3.5 4.5 5.5]
[4. 4. 4. 4.]
"
    );
}

/// Lengths that neither match nor are 1 panic, naming the result's dimension, the length the
/// result has there, and the length that does not fit.
#[test]
fn shapes_that_do_not_broadcast_panic() {
    let program = "\
def main():
    a = Array.zeros((2, 3))
    b = Array.zeros((4,))
    print(a + b)
";
    let (code, _, report) = run("broadcast-mismatch", program).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 1: 3 and 4\n  --> main.byn:4:11\n"
    );
}

/// `x += b` keeps `x`'s shape, as `NumPy`'s `+=` does: a `b` that would broadcast `x` larger
/// panics, where `x = x + b` gives a new, larger array.
#[test]
fn an_update_in_place_never_grows_its_target() {
    let program = "\
def main():
    x = Array.zeros((1,))
    b = Array.zeros((3,))
    y = x + b
    print(y)
    x += b
";
    let (code, stdout, report) = run("broadcast-in-place", program).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(stdout, "[0. 0. 0.]\n");
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 1 and 3\n  --> main.byn:6:5\n"
    );
}

/// `None` in a subscript adds an axis of length 1, so `x[:, None] - x[None, :]` is the table of
/// every difference, and a vector times a row of another broadcasts to a matrix: what `NumPy`
/// 2.5 prints for the same lines.
#[test]
fn new_axes_make_outer_operations() {
    let program = "\
def main():
    x = Array([0.0, 1.0, 3.0])
    print(x[:, None] - x[None, :])
    y = Array([1.0, 2.0])
    print(x[:, None] * y)
    print(x[None, :])
    print(x[None, :, None] + y)
";
    let (code, stdout, report) = run("broadcast-new-axes", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
[[ 0. -1. -3.]
 [ 1.  0. -2.]
 [ 3.  2.  0.]]
[[0. 0.]
 [1. 2.]
 [3. 6.]]
[[0. 1. 3.]]
[[[1. 2.]
  [2. 3.]
  [4. 5.]]]
"
    );
}

/// An array meets a scalar or array of a wider number type in that type, as `NumPy` promotes:
/// a float array plus a complex number adds in `complex128`, and an integer array plus a float
/// in `f64`. Each used to reach the backend unconverted and fail to compile.
#[test]
fn mixed_element_types_promote_as_numpy_does() {
    let program = "\
def main():
    a = Array([1.0, 2.0])
    print(a + 0j)
    print(a * 2j - a)
    n = Array([1, 2])
    print(n + 1.5)
    print(n * 0.5 + n)
";
    let (code, stdout, report) = run("broadcast-promote", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "[1.+0.j 2.+0.j]\n[-1.+2.j -2.+4.j]\n[2.5 3.5]\n[1.5 3. ]\n"
    );
}

/// Integer `//` and `%` of arrays check each divisor, as the same operators on two integers do,
/// so a zero divisor panics at the expression rather than stopping the program with `SIGFPE`
/// (roadmap 1.9.37). `NumPy` gives 0 with a warning there instead; Baryon keeps one rule for
/// integers, scalar or array.
#[test]
fn integer_array_division_by_zero_panics() {
    let program = "\
def main():
    a = Array([7, -7, 4])
    print(a // Array([2, 2, 3]), a % Array([2, 2, 3]), a // 2)
    print(a % Array([1, 0, 2]))
";
    let (code, stdout, report) = run("broadcast-divide-by-zero", program).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(stdout, "[ 3 -4  1] [1 1 1] [ 3 -4  2]\n");
    assert_eq!(
        report,
        "panic: integer division by zero\n  --> main.byn:4:11\n"
    );
}

/// `/` of integer arrays gives a float array, as in `NumPy`, whatever side the integers are on
/// (roadmap 1.9.38); before, the backend could not divide two integers.
#[test]
fn integer_arrays_divide_to_floats() {
    let program = "\
def main():
    c = Array([1, 2, 3])
    d = Array([1, 4, 2])
    print(c / d, c / 2, 3 / d, d / Array([0, 0, 0]))
";
    let (code, stdout, report) = run("broadcast-true-divide", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "[1.  0.5 1.5] [0.5 1.  1.5] [3.   0.75 1.5 ] [inf inf inf]\n"
    );
}

/// Float `//` by zero gives `a / b`, as `NumPy` does, for a scalar and an array alike: an
/// infinity with the quotient's sign, or NaN for `0.0 // 0.0`; `%` by zero gives NaN (roadmap
/// 1.9.39). Python raises `ZeroDivisionError` for both.
#[test]
fn float_floor_division_by_zero_follows_numpy() {
    let program = "\
def main():
    a = 2.0
    zero = 0.0
    print(a // zero, -a // zero, zero // zero, a % zero, -7.0 // 2.0, -0.0 // 3.0)
    print(Array([2.0, -2.0, 0.0]) // Array([0.0, 0.0, 0.0]), Array([2.0, -2.0]) // -0.0)
    print(Array([2.0, -2.0]) % Array([0.0, 0.0]))
";
    let (code, stdout, report) = run("broadcast-float-floor-by-zero", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "inf -inf nan nan -4.0 -0.0\n[ inf -inf  nan] [-inf  inf]\n[nan nan]\n"
    );
}
