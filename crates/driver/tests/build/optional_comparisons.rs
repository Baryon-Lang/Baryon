//! Comparisons on arrays with missing values (roadmap 3.13.6, decision 0045), built and run:
//! `==`, `!=`, `<`, `<=`, `>` and `>=` give an array of `bool`s that is `None` wherever a
//! side is `None`, as pandas's nullable arrays do. Such masks combine with `&`, `|`, `^` and
//! `~` by pandas's rules, and choose the rows of a frame and the elements of an array where
//! they are `True`.

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

/// An array with missing values compares with another, with a plain array and with a number,
/// on either side, and integers compare with floats as plain arrays do. The result is `None`
/// where a side is, a side can be an expression or a call, and the result is an array with
/// missing values like any other: it fills, counts and reads by position.
#[test]
fn arrays_with_gaps_compare() -> io::Result<()> {
    let program = r"
def bids() -> Array[f64 | None, 1]:
    return Array([1.5, None, 2.5, 4.0])


def main():
    a = bids()
    b: Array[f64 | None, 1] = Array([None, 1.0, 2.5, 2.0])
    c = Array([1.0, 2.0, 3.0, 5.0])
    i: Array[i64 | None, 1] = Array([3, None, 2, 4])
    print(a > 2.0)
    print(a == b)
    print(a != b)
    print(a < c)
    print(c >= a)
    print(2 <= a)
    print(i > a)
    print(i == 2)
    print(i > 2.5)
    print((a + b) > 4.5)
    print(bids() > bids())
    mask = a > 2.0
    print(mask.isna(), mask.fillna(False), mask.count())
    print(mask[0], mask[1])
    print(c[mask.fillna(False)])
    empty: Array[f64 | None, 1] = Array([])
    print(empty > 1.0)
";
    let (code, output, report) = run("optional_comparisons_arrays", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[False None True True]\n\
         [None None True False]\n\
         [None None False True]\n\
         [False None True True]\n\
         [False None True True]\n\
         [False None True True]\n\
         [True None False False]\n\
         [False None True False]\n\
         [True None False True]\n\
         [None None True True]\n\
         [False None False False]\n\
         [False  True False False] [False False  True  True] 3\n\
         False None\n\
         [3. 5.]\n\
         []\n"
    );
    Ok(())
}

/// A mask with missing values chooses the rows of a frame where it is `True`: a row whose
/// mask is `None` is left out, as in pandas. The mask can be a variable or an expression of
/// columns, can choose no row, and can be a column of its own.
#[test]
fn masks_with_gaps_choose_rows() -> io::Result<()> {
    let program = r#"
def main():
    quotes = DataFrame({
        "sym": Array(["a", "b", "c", "d"]),
        "bid": Array([1.5, None, 2.5, 4.0]),
        "size": Array([10, 20, None, 40]),
    })
    print(quotes[quotes.bid > 2.0])
    print(quotes[quotes.size == 20])
    print(quotes[quotes.bid * 2.0 >= quotes.size / 10.0])
    low = quotes.bid < 3.0
    print(quotes[low])
    print(len(quotes[quotes.bid > 100.0]))
    print(quotes.assign(big=quotes.bid > 2.0))
"#;
    let (code, output, report) = run("optional_comparisons_masks", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "\x20 sym  bid  size\n\
         0   c  2.5  <NA>\n\
         1   d  4.0    40\n\
         \x20 sym   bid  size\n\
         0   b  <NA>    20\n\
         \x20 sym  bid  size\n\
         0   a  1.5    10\n\
         1   d  4.0    40\n\
         \x20 sym  bid  size\n\
         0   a  1.5    10\n\
         1   c  2.5  <NA>\n\
         0\n\
         \x20 sym   bid  size    big\n\
         0   a   1.5    10  False\n\
         1   b  <NA>    20   <NA>\n\
         2   c   2.5  <NA>   True\n\
         3   d   4.0    40   True\n"
    );
    Ok(())
}

/// Masks that may hold `None` combine by pandas's rules: `False & None` is `False` and
/// `True | None` is `True`, since the side that is there settles the answer, and every other
/// combination with a `None` is `None`. A plain mask can be either side. All nine pairs of
/// `True`, `False` and `None` are here.
#[test]
fn masks_with_gaps_combine() -> io::Result<()> {
    let program = r"
def main():
    t: Array[bool | None, 1] = Array([True, True, True, False, False, False, None, None, None])
    u: Array[bool | None, 1] = Array([True, False, None, True, False, None, True, False, None])
    p = Array([True, False, True, False, True, False, True, False, True])
    print(t & u)
    print(t | u)
    print(t ^ u)
    print(~t)
    print(t & p)
    print(p | t)
    print(p ^ t)
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0])
    asks = Array([2.0, 3.0, 2.0, 9.0])
    print((bids > 2.0) & (asks < 5.0))
    print((bids > 2.0) | (asks < 2.5))
    print(~(bids > 2.0) & ~(bids > 3.0))
    empty: Array[bool | None, 1] = Array([])
    print(empty & empty, ~empty)
";
    let (code, output, report) = run("optional_comparisons_combined", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[True False None False False False None False None]\n\
         [True True True True False None True None None]\n\
         [False True None True False None None None None]\n\
         [False False False True True True None None None]\n\
         [True False True False False False None False None]\n\
         [True True True False True False True None True]\n\
         [False True False False True False None None None]\n\
         [False None True False]\n\
         [True None True True]\n\
         [True None False False]\n\
         [] []\n"
    );
    Ok(())
}

/// A combined mask chooses rows of a frame, and a mask that may hold `None` chooses and
/// sets elements of a plain array: where it is `None` nothing is chosen and nothing is set.
#[test]
fn masks_with_gaps_choose_elements() -> io::Result<()> {
    let program = r#"
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0])
    asks = Array([2.0, 3.0, 2.0, 9.0])
    quotes = DataFrame({"bid": bids.copy(), "ask": asks.copy()})
    print(quotes[(quotes.bid > 2.0) & (quotes.ask < 5.0)])
    print(quotes[~(quotes.bid > 2.0)])
    print(quotes[(quotes.bid > 2.0) | (quotes.ask < 2.5)])
    print(asks[bids > 2.0])
    kept = asks[(bids > 2.0) | (asks > 8.0)]
    print(kept)
    counts = Array([1, 2, 3, 4])
    print(counts[~(bids > 2.0)])
    asks[bids > 2.0] = 0.0
    print(asks)
"#;
    let (code, output, report) = run("optional_comparisons_chosen", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "\x20  bid  ask\n\
         0  2.5  2.0\n\
         \x20  bid  ask\n\
         0  1.5  2.0\n\
         \x20  bid  ask\n\
         0  1.5  2.0\n\
         1  2.5  2.0\n\
         2  4.0  9.0\n\
         [2. 9.]\n\
         [2. 9.]\n\
         [1]\n\
         [2. 3. 0. 0.]\n"
    );
    Ok(())
}

/// Arrays of different lengths do not compare, a mask of another length than the frame
/// chooses nothing, and masks of different lengths do not combine: the program panics and
/// names both lengths.
#[test]
fn lengths_must_match() -> io::Result<()> {
    let compared = r"
def main():
    a: Array[f64 | None, 1] = Array([1.5, None])
    c = Array([1.0, 2.0, 3.0])
    print(a > c)
";
    let (code, output, report) = run("optional_comparisons_lengths", compared)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 2 and 3"),
        "{report}"
    );
    let masked = r#"
def main():
    frame = DataFrame({"x": Array([1.0, 2.0, 3.0])})
    a: Array[f64 | None, 1] = Array([1.5, None])
    print(frame[a > 1.0])
"#;
    let (code, output, report) = run("optional_comparisons_mask_length", masked)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 3 and 2"),
        "{report}"
    );
    let combined = r"
def main():
    a: Array[f64 | None, 1] = Array([1.5, None])
    c = Array([1.0, 2.0, 3.0])
    print((a > 1.0) & (c > 1.0))
";
    let (code, output, report) = run("optional_comparisons_combined_length", combined)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 2 and 3"),
        "{report}"
    );
    Ok(())
}
