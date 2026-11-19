//! One-pass sums (decision 0041), built and run: `(a * b).sum()` and its kin never store the
//! expression, yet print what `NumPy` 2.5 prints for the same program, bit for bit, and panic
//! as before when the lengths differ. Integer and `bool` sums, and sums along an axis, are one
//! pass too (decision 0041, amended), and an integer overflow panics where it did.

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

/// Sums of products and of squared differences at every length that takes a different path
/// through the pairwise tree, from none to several leaves; a matrix minus a broadcast row; a
/// strided, reversed view; and `f32` elements. Every line is `NumPy`'s, printed in full.
#[test]
fn one_pass_sums_print_what_numpy_prints() {
    let program = "\
def main():
    for n in [0, 1, 7, 8, 127, 128, 129, 300, 1000, 4099]:
        u = Array.zeros((n,))
        v = Array.zeros((n,))
        for i in range(n):
            u[i] = 1.0 / (i + 1.0)
            v[i] = 0.1 * (i % 7) - 0.25
        print((u * v).sum())
        print(((u - v) * (u - v)).sum())
    g = Array.zeros((37, 41))
    for i in range(37):
        for j in range(41):
            g[i, j] = 1.0 / (i + 2.0 * j + 1.0)
    row = Array.zeros((41,))
    for j in range(41):
        row[j] = 0.01 * j
    print((g - row).sum())
    print((g[1:30:2, ::-1] * 3.0).sum())
    w = Array.zeros((3,), dtype=f32)
    w[0] = 0.1
    w[1] = 0.2
    w[2] = 0.3
    print((w * w).sum())
";
    let (code, stdout, report) = run("one-pass-numpy", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
0.0
0.0
-0.25
1.5625
-0.20749999999999993
2.2242970521541956
-0.23875
2.364922052154195
-0.11702198317671772
7.288634936151649
-0.11819385817671774
7.313539721307899
-0.11858145507594248
7.316875007648864
-0.07462502734350931
14.460856337584644
-0.014714974985155019
44.093364516651874
0.0556547877764031
175.6608805591098
-265.64108585055686
48.0728765595519
0.14000002
"
    );
}

/// Arrays of different lengths panic at the expression with both lengths, as the stored sum
/// did.
#[test]
fn a_one_pass_sum_checks_lengths_first() {
    let program = "\
def main():
    a = Array([1.0, 2.0, 3.0])
    b = Array([1.0, 2.0, 3.0, 4.0])
    print((a * b).sum())
";
    let (code, stdout, report) = run("one-pass-lengths", program).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 3 and 4\n  --> main.byn:4:12\n"
    );
}

/// Integer and `bool` sums of an expression, `any` and `all` of a comparison, and sums of an
/// expression along each axis, of integers and of floats, the last axis long enough for
/// several pairwise leaves per row; then the same sums of stored arrays. Every line is what
/// `NumPy` 2.5 prints, so the fused sums add in the stored sums' order.
#[test]
fn integer_and_axis_sums_print_what_numpy_prints() {
    let program = "\
def main():
    n = Array.zeros((3, 5), dtype=i64)
    g = Array.zeros((4, 300))
    h = Array.zeros((4, 300))
    for i in range(3):
        for j in range(5):
            n[i, j] = 7 * i - 3 * j + 1
    for i in range(4):
        for j in range(300):
            g[i, j] = 1.0 / (i + 2.0 * j + 1.0)
            h[i, j] = 0.01 * j - 0.3 * i
    print((n * n - 2).sum(), (n > 0).sum(), (n * 2 != 4).any(), (n < 100).all())
    print((n * 3).sum(axis=0), (n * 3).sum(axis=1), (n > 0).sum(axis=1))
    s0 = (g - h).sum(axis=0)
    print(s0[0], s0[150], s0[299])
    s1 = ((g - h) * (g - h)).sum(axis=1)
    print(s1[0], s1[1], s1[2], s1[3])
    print((g * 2.0).sum(axis=-1)[2], (g * h).mean(axis=1)[3])
    w = n * n - 2
    d = g - h
    q = d * d
    print(w.sum(), d.sum(axis=0)[150], q.sum(axis=1)[3])
";
    let (code, stdout, report) = run("one-pass-exact", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
790 9 True True
[ 72  45  18  -9 -36] [-75  30 135] [1 3 5]
3.8833333333333333 -4.186776678868428 -10.15333886117261
893.7762036823925 652.7630273736172 466.02530174777274 333.2285115783568
5.6706207506613175 -0.0031051787471347634
790 -4.186776678868428 333.2285115783568
"
    );
}

/// An integer sum of an expression that overflows panics at the sum, as the loop over the
/// stored expression did.
#[test]
fn a_one_pass_integer_sum_panics_on_overflow() {
    let program = "\
def main():
    a = Array([4611686018427387904, 4611686018427387904, 1])
    b = Array([1, 1, 1])
    print((a * b).sum())
";
    let (code, stdout, report) = run("one-pass-overflow", program).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: integer overflow in `+`\n  --> main.byn:4:11\n"
    );
}
