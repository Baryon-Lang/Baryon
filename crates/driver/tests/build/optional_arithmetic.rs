//! Arithmetic on arrays with missing values (roadmap 3.13.6, decision 0045), built and run:
//! `+`, `-`, `*`, `/`, `//`, `%`, `**` and unary `-`, and `&`, `|`, `^` and `~` of whole
//! numbers, give `None` wherever a side is `None`, and what pandas's nullable arrays give
//! everywhere else.

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

/// `+=` and the other in-place operators give a variable the array the operator would,
/// with a number, a plain array, another array with missing values or an expression that
/// reads the variable, through an `InOut` parameter, and for integers, where a zero beside
/// a `None` does not divide and one that is there stops the program.
#[test]
fn in_place_operators_rebind() -> io::Result<()> {
    let program = r"
def bump(values: InOut[Array[f64 | None, 1]]):
    values *= 2.0


def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    asks: Array[f64 | None, 1] = Array([None, 1.0, 0.5])
    bids += 1.0
    print(bids)
    bids -= asks
    print(bids)
    bids /= Array([2.0, 2.0, 2.0])
    print(bids)
    bids += bids * 2.0
    print(bids)
    bump(bids)
    print(bids)
    counts: Array[i64 | None, 1] = Array([7, None, 9])
    counts //= 2
    counts %= 3
    counts *= counts
    print(counts)
    zero: Array[i64 | None, 1] = Array([0, None, 3])
    gap: Array[i64 | None, 1] = Array([None, 0, 3])
    zero //= gap
    print(zero)
    zero //= Array([1, 1, 0])
";
    let (code, output, report) = run("optional_arithmetic_in_place", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(
        output,
        "[2.5 None 3.5]\n\
         [None None 3.0]\n\
         [None None 1.5]\n\
         [None None 4.5]\n\
         [None None 9.0]\n\
         [0 None 1]\n\
         [None None 1]\n"
    );
    assert!(
        report.contains("panic: integer division by zero"),
        "{report}"
    );
    Ok(())
}

/// `**` and `**=` (roadmap 3.13.14) raise each element to a power, between arrays with
/// missing values, a plain array and a number, for floats and for integers, in an
/// expression of several operators and on the columns of a frame. The result is `None`
/// wherever a side is, with no exception: `None ** 0.0` and `1.0 ** None` are `None`, where
/// pandas gives 1. An integer power is computed only where both sides are there, so a huge
/// exponent beside a `None` does not overflow, and a negative exponent that is there stops
/// the program as it does for plain arrays.
#[test]
fn powers_with_gaps() -> io::Result<()> {
    let program = r"
class Quote(Struct):
    bid: f64 | None
    size: i64 | None


def main():
    a: Array[f64 | None, 1] = Array([1.5, None, 3.0, None])
    b: Array[f64 | None, 1] = Array([2.0, 2.0, None, 0.0])
    plain = Array([2.0, 0.5, 1.0, 0.0])
    print(a ** b)
    print(a ** 2.0)
    print(2.0 ** a)
    print(a ** plain)
    print(plain ** a)
    print(a ** 2)
    print((a + 1.0) ** 2.0 - a ** b)
    print(1.0 ** a)
    print(a ** 0.0)
    n: Array[i64 | None, 1] = Array([2, None, 3037000499, 10])
    m: Array[i64 | None, 1] = Array([10, 3, None, -1])
    print(n ** Array([2, 70, 2, 3]))
    print(Array([3, 2, 2, -2]) ** (m + 1))
    print(n ** a)
    c = a ** b
    c **= 2.0
    c **= b
    print(c)
    k: Array[i64 | None, 1] = Array([2, None, 3])
    k **= 2
    k **= k
    print(k)
    quotes = DataFrame([Quote(1.5, 2), Quote(None, 3), Quote(2.0, None)])
    print(quotes.bid ** quotes.size)
    print(quotes.size ** 2)
    print(n ** m)
";
    let (code, output, report) = run("optional_arithmetic_power", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(
        output,
        "[2.25 None None None]\n\
         [2.25 None 9.0 None]\n\
         [2.8284271247461903 None 8.0 None]\n\
         [2.25 None 3.0 None]\n\
         [2.8284271247461903 None 1.0 None]\n\
         [2.25 None 9.0 None]\n\
         [4.0 None None None]\n\
         [1.0 None 1.0 None]\n\
         [1.0 None 1.0 None]\n\
         [4 None 9223372030926249001 1000]\n\
         [177147 16 None 1]\n\
         [2.8284271247461903 None 2.8011385460385662e+28 None]\n\
         [25.62890625 None None None]\n\
         [256 None 387420489]\n\
         [2.25 None None]\n\
         [4 9 None]\n"
    );
    assert!(
        report.contains("panic: negative exponent -1 in integer `**`"),
        "{report}"
    );
    Ok(())
}

/// `&`, `|`, `^` and `~` on whole numbers with missing values (roadmap 3.24.1) work bit by
/// bit and give `None` wherever a side is `None`, as pandas's nullable integers do: between
/// two such arrays, with a plain array or a number on either side, in one expression with
/// arithmetic, in place, on unsigned numbers and on a column of a frame. Masks of `bool`s
/// keep their own rules.
#[test]
fn bits_with_gaps() -> io::Result<()> {
    let program = r#"
def main():
    k: Array[i64 | None, 1] = Array([6, None, 3])
    j: Array[i64 | None, 1] = Array([3, 1, None])
    print(k & j)
    print(k | 3)
    print(5 ^ k)
    print(~k)
    print(k & Array([1, 1, 1]))
    print((k + 1) & ~j | 8)
    k &= 2
    k |= j
    k ^= k
    print(k)
    small: Array[u8 | None, 1] = Array([u8(255), None, u8(3)])
    print(~small, small & u8(15))
    flags: Array[bool | None, 1] = Array([True, None, False])
    print(~flags, flags & flags)
    frame = DataFrame({"k": j})
    print(frame.k & 1)
"#;
    let (code, output, report) = run("optional_arithmetic_bits", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[2 None None]\n\
         [7 None 3]\n\
         [3 None 6]\n\
         [-7 None -4]\n\
         [0 None 1]\n\
         [12 None None]\n\
         [0 None None]\n\
         [0 None 252] [15 None 3]\n\
         [False None True] [True None False]\n\
         [1 1 None]\n"
    );
    Ok(())
}

/// An in-place operator on a column of a frame that may hold `None` (roadmap 3.24.2) gives
/// the column the array the operator would give, by attribute or by name, with a number, a
/// plain column, an array with missing values or the column itself, for floats, integers
/// and masks, and through an `InOut` frame. A mask keeps the masks' rule, `False & None`
/// is `False`, in a column and in a variable. A value of another length stops the program
/// and leaves the frame's columns one length.
#[test]
fn in_place_operators_replace_a_column() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    bid: f64 | None
    size: i64 | None
    ok: bool | None


def bump(quotes: InOut[DataFrame[Quote]], by: f64):
    quotes.bid += by
    quotes["size"] -= 1


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "bid": Array([1.5, None, 3.0]),
        "size": Array([10, 20, None]),
        "ok": Array([True, None, False]),
    })
    bump(quotes, 2.0)
    quotes.size //= 3
    quotes["size"] **= 2
    quotes.size |= 1
    other: Array[bool | None, 1] = Array([True, True, None])
    quotes.ok &= other
    print(quotes)
    other |= quotes.ok
    print(other)
    quotes.bid *= Array([2.0, 2.0, 2.0])
    quotes.bid -= quotes.bid / 2.0
    extra: Array[f64 | None, 1] = Array([None, 1.0, 1.0])
    quotes["bid"] += extra
    print(quotes.bid)
    quotes.bid += Array([1.0, 2.0])
"#;
    let (code, output, report) = run("optional_arithmetic_column_in_place", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(
        output,
        "    bid  size     ok\n\
         0   3.5     9   True\n\
         1  <NA>    37   <NA>\n\
         2   5.0  <NA>  False\n\
         [True True None]\n\
         [None None 6.0]\n"
    );
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 3 and 2"),
        "{report}"
    );
    Ok(())
}
