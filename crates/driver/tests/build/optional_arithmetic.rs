//! Arithmetic on arrays with missing values (roadmap 3.13.6, decision 0045), built and run:
//! `+`, `-`, `*`, `/`, `//`, `%` and unary `-` give `None` wherever a side is `None`, and
//! what pandas's nullable arrays give everywhere else.

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

/// Floats with gaps combine with each other, with a plain array and with a number, on either
/// side. The result is `None` where any array in the expression is, an expression of several
/// operators is one result, and a call's result can be an operand. Dividing a float by zero
/// gives `inf` or `nan`, never a panic. The result is an array with missing values like any
/// other: it reduces, fills, reads by position and becomes a column.
#[test]
fn floats_with_gaps_combine() -> io::Result<()> {
    let program = r#"
def bids() -> Array[f64 | None, 1]:
    return Array([1.5, None, 2.5, 4.0])


def halved(values: Array[f64 | None, 1]) -> Array[f64 | None, 1]:
    return values / 2.0


def main():
    a = bids()
    b: Array[f64 | None, 1] = Array([None, 1.0, 0.0, 2.0])
    c = Array([10.0, 20.0, 30.0, 40.0])
    print(a + b)
    print(a - c)
    print(c * a)
    print(a / b)
    print(a // b)
    print(a % b)
    print(a * 2)
    print(1.0 - a)
    print(-a)
    print(+a)
    print((a + b) * c - a / 2.0)
    print(bids() + bids())
    print(halved(a + a))
    print((a + b).sum(), (a + b)[0], (a + b)[3])
    print((a - a).fillna(0.5))
    for step in range(3):
        a = a + a
    print(a)
    frame = DataFrame({"x": bids(), "y": b.copy()})
    print(frame.assign(z=frame.x + frame.y, w=frame.x * 2.0))
"#;
    let (code, output, report) = run("optional_arithmetic_floats", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[None None 2.5 6.0]\n\
         [-8.5 None -27.5 -36.0]\n\
         [15.0 None 75.0 160.0]\n\
         [None None inf 2.0]\n\
         [None None inf 2.0]\n\
         [None None nan 0.0]\n\
         [3.0 None 5.0 8.0]\n\
         [-0.5 None -1.5 -3.0]\n\
         [-1.5 None -2.5 -4.0]\n\
         [1.5 None 2.5 4.0]\n\
         [None None 73.75 238.0]\n\
         [3.0 None 5.0 8.0]\n\
         [1.5 None 2.5 4.0]\n\
         8.5 None 6.0\n\
         [0.  0.5 0.  0. ]\n\
         [12.0 None 20.0 32.0]\n\
         \x20     x     y     z     w\n\
         0   1.5  <NA>  <NA>   3.0\n\
         1  <NA>   1.0  <NA>  <NA>\n\
         2   2.5   0.0   2.5   5.0\n\
         3   4.0   2.0   6.0   8.0\n"
    );
    Ok(())
}

/// Integers with gaps combine without a panic the program did not earn: the largest `i64`
/// beside a `None` does not overflow, and a zero divisor beside a `None`, or under one, does
/// not divide. `/` of two integers gives floats, `inf` for a zero divisor that is there.
/// Signed and unsigned integers and an empty array work, and integers mix with floats as
/// plain arrays do.
#[test]
fn integers_with_gaps_combine() -> io::Result<()> {
    let program = r"
def main():
    i: Array[i64 | None, 1] = Array([9223372036854775807, None, 7, -3])
    j: Array[i64 | None, 1] = Array([None, 0, 2, 2])
    k = Array([1, 2, 3, 4])
    a: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0])
    print(i + j)
    print(i // j)
    print(i % j)
    print(i / j)
    print(j / 4)
    print(k / j)
    print(j * k)
    print(-j)
    print(i - 1)
    print(i * a)
    print((i + j) // (j - 2 + k))
    empty: Array[i64 | None, 1] = Array([])
    print(empty + empty)
    small: Array[u8 | None, 1] = Array([u8(0), None, u8(3)])
    print(small - u8(0))
    print(small * small)
";
    let (code, output, report) = run("optional_arithmetic_integers", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[None None 9 -1]\n\
         [None None 3 -2]\n\
         [None None 1 1]\n\
         [None None 3.5 -1.5]\n\
         [None 0.0 0.5 0.5]\n\
         [None inf 1.5 2.0]\n\
         [None 0 6 8]\n\
         [None 0 -2 -2]\n\
         [9223372036854775806 None 6 -4]\n\
         [1.3835058055282164e+19 None 17.5 -12.0]\n\
         [None None 3 -1]\n\
         []\n\
         [0 None 3]\n\
         [0 None 9]\n"
    );
    Ok(())
}

/// Two integers that are both there still overflow with the panic any array gives, naming
/// the operator.
#[test]
fn integers_that_are_there_still_overflow() -> io::Result<()> {
    let program = r"
def main():
    i: Array[i64 | None, 1] = Array([9223372036854775807, None])
    print(i + 1)
";
    let (code, output, report) = run("optional_arithmetic_overflow", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: integer overflow in `+`"),
        "{report}"
    );
    Ok(())
}

/// Arrays of different lengths do not combine: the program panics and names both lengths.
/// An array with missing values does not broadcast.
#[test]
fn lengths_must_match() -> io::Result<()> {
    let program = r"
def main():
    a: Array[f64 | None, 1] = Array([1.5, None])
    c = Array([1.0, 2.0, 3.0])
    print(a + c)
";
    let (code, output, report) = run("optional_arithmetic_lengths", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 2 and 3"),
        "{report}"
    );
    Ok(())
}
