//! Reductions along one axis (roadmap 3.9), built and run: `sum`, `min` and `max` with an axis
//! print what `NumPy` 2.5 prints for the same program, bit for bit, and a `min` along an empty
//! axis panics as `NumPy`'s raises.

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

/// Float sums along the last axis, pairwise per row, along the first, one element at a time,
/// and through a strided view; sums, minimums and maximums of a rank-3 array and of integers;
/// a NaN that wins its row and column; and a vector reduced to one number. Every line is what
/// `NumPy` prints, and each float sum is printed in full so a different order would show.
#[test]
fn axis_reductions_print_what_numpy_prints() {
    let program = "\
import math


def show(v: Array[f64, 1]):
    for i in range(v.shape[0]):
        print(v[i])


def main():
    a = Array.zeros((3, 20))
    for i in range(3):
        for j in range(20):
            a[i, j] = 1.0 / (i + j + 1.0) + 0.1 * j
    show(a.sum(axis=1))
    show(a.sum(axis=0))
    show(a.sum(-1))
    show(a[:, 1:19:2].sum(axis=1))
    c = Array.zeros((4, 3, 5))
    for i in range(4):
        for j in range(3):
            for k in range(5):
                c[i, j, k] = 0.1 * i + 0.3 * j - 0.7 * k
    s = c.sum(axis=1)
    print(s.shape)
    print(s)
    print(c.min(axis=0))
    print(c.max(axis=2))
    n = Array.zeros((2, 3), dtype=i64)
    for i in range(2):
        for j in range(3):
            n[i, j] = i * 10 + j
    print(n.sum(axis=0))
    print(n.max(axis=1))
    print(n.min(axis=-2))
    a[1, 4] = math.nan
    print(a.max(axis=1))
    print(a.min(axis=0)[4])
    v = Array([3.0, -1.0, 2.0])
    print(v.sum(axis=0))
    print(v.min(axis=-1))
";
    let (code, stdout, report) = run("axes-numpy", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
22.59773965714368
21.645358704762735
21.190813250217275
1.8333333333333333
1.3833333333333333
1.3833333333333333
1.5166666666666666
1.7095238095238097
1.9345238095238093
2.178968253968254
2.436111111111111
2.7020202020202024
2.9742424242424246
3.2511655011655005
3.5316849816849816
3.8150183150183157
4.100595238095238
4.387990196078432
4.67687908496732
4.967010663914689
5.258187134502925
5.550250626566417
5.843073593073593
22.59773965714368
21.645358704762735
21.190813250217275
9.514484126984128
9.233255530159557
9.064484126984128
(4, 5)
[[ 0.9 -1.2 -3.3 -5.4 -7.5]
 [ 1.2 -0.9 -3.  -5.1 -7.2]
 [ 1.5 -0.6 -2.7 -4.8 -6.9]
 [ 1.8 -0.3 -2.4 -4.5 -6.6]]
[[ 0.  -0.7 -1.4 -2.1 -2.8]
 [ 0.3 -0.4 -1.1 -1.8 -2.5]
 [ 0.6 -0.1 -0.8 -1.5 -2.2]]
[[0.  0.3 0.6]
 [0.1 0.4 0.7]
 [0.2 0.5 0.8]
 [0.3 0.6 0.9]]
[10 12 14]
[ 2 12]
[0 1 2]
[1.95              nan 1.94545455]
nan
4.0
-1.0
"
    );
}

/// `Fixed` arrays reduce to `Fixed` arrays, and a view that walks both axes backwards sums each
/// row in `NumPy`'s order.
#[test]
fn fixed_arrays_and_reversed_views_reduce_as_numpy_does() {
    let program = "\
Mat = Fixed[f64, (2, 3)]


def main():
    m = Mat((1.5, -2.0, 0.25), (4.0, 0.5, -1.0))
    print(m.sum(axis=0))
    print(m.sum(axis=1))
    print(m.max(axis=-1))
    a = Array.zeros((2, 12))
    for i in range(2):
        for j in range(12):
            a[i, j] = 1.0 / (j + 1.0) + i
    r = a[::-1, ::-1].sum(axis=1)
    print(r[0])
    print(r[1])
";
    let (code, stdout, report) = run("axes-fixed", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
[ 5.5  -1.5  -0.75]
[-0.25  3.5 ]
[1.5 4. ]
15.103210678210678
3.103210678210678
"
    );
}

/// A `min` along an axis of length 0 panics even though the other axis is long, and a `max`
/// along a long axis of an array with no rows gives an empty array, as `NumPy`'s do.
#[test]
fn an_extreme_along_an_empty_axis_panics() {
    let program = "\
def main():
    a = Array.zeros((0, 3))
    print(a.sum(axis=0))
    print(a.max(axis=1))
    print(a.min(axis=0))
";
    let (code, stdout, report) = run("axes-empty", program).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(stdout, "[0. 0. 0.]\n[]\n");
    assert_eq!(
        report,
        "panic: `a.min(axis=0)` of an empty array\n  --> main.byn:5:11\n"
    );
}

/// A sum of `bool` elements counts the `True` ones and gives an `int`, whole or along an axis,
/// as `NumPy` does, so `values.isna().sum()` is the number of missing values (roadmap 1.9.36).
/// Before, the checker typed the sum as a `bool` and the backend could not add two of them.
#[test]
fn sums_of_bools_count_the_true_elements() {
    let program = "\
def main():
    grid = Array.ones((2, 3))
    grid[0, 1] = -1.0
    mask = grid > 0.0
    print(mask.sum(axis=0), mask.sum(axis=1), mask.sum())
    flags = Array([True, False, True])
    count: int = flags.sum()
    print(count, flags.sum(axis=0), Array([False]).sum())
    values: list[f64 | None] = [1.0, None, 3.0, None, None]
    print(Array(values).isna().sum())
";
    let (code, stdout, report) = run("axes-bools", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, "[2 1 2] [2 3] 5\n2 2 0\n3\n");
}
