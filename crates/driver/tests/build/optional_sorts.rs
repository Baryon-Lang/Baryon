//! `argsort`, `unique`, `nunique` and text `min` and `max` of an array with missing values
//! (roadmap 3.13.6, decision 0045), built and run: the positions that put the elements in
//! rising order, with those of the `None`s last; the distinct elements, a `None` among
//! them, and how many are there; and the first and last text by `<`.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Floats, integers, `bool`s and text that may be `None` are each put in rising order with
/// the `None`s last, after any NaN, and equal elements and `None`s in the order they stand
/// in. The positions choose elements of the array they came from, and an array with no
/// elements gives none. A column of a frame is such an array.
#[test]
fn positions_put_none_last() -> io::Result<()> {
    let program = r#"
import math


class Quote(Struct):
    price: f64
    bid: f64 | None


def main():
    bids: Array[f64 | None, 1] = Array([3.0, None, 1.0, math.nan, None, 2.0])
    print(bids.argsort())
    print(bids[bids.argsort()])
    names: Array[str | None, 1] = Array(["b", None, "a", "b"])
    order = names.argsort()
    print(order, names[order])
    flags: Array[bool | None, 1] = Array([True, None, False])
    print(flags.argsort())
    ids: Array[i64 | None, 1] = Array([5, 4, None, 4])
    print(ids.argsort())
    empty = bids[0:0]
    print(empty.argsort(), len(empty.argsort()))
    gone: Array[i64 | None, 1] = Array([None, None])
    print(gone.argsort())
    quotes: DataFrame[Quote] = DataFrame(
        {"price": Array([1.0, 2.0, 3.0]), "bid": Array([2.5, None, 1.5])}
    )
    print(quotes.bid.argsort())
"#;
    let directory = package("optional_sorts_positions", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[2 5 0 3 1 4]\n\
         [1.0 2.0 3.0 nan None None]\n\
         [2 0 3 1] ['a' 'b' 'b' None]\n\
         [2 0 1]\n\
         [1 3 0 2]\n\
         [] 0\n\
         [0 1]\n\
         [2 0 1]\n"
    );
    Ok(())
}

/// `unique` keeps the first of each distinct element, one `None` and one NaN among them,
/// and `nunique` counts the distinct values that are there, for floats, integers, `bool`s
/// and text, for an array with no elements and for one that has only `None`s after
/// assignments.
#[test]
fn distinct_elements_keep_one_none() -> io::Result<()> {
    let program = r#"
import math


def main():
    bids: Array[f64 | None, 1] = Array([3.0, None, 1.0, math.nan, 3.0, None, math.nan])
    print(bids.unique(), bids.nunique())
    names: Array[str | None, 1] = Array([None, "b", "a", "b", None])
    print(names.unique(), names.nunique())
    flags: Array[bool | None, 1] = Array([True, None, True])
    print(flags.unique(), flags.nunique())
    ids: Array[i64 | None, 1] = Array([5, 4, 5])
    print(ids.unique(), ids.nunique())
    empty = bids[0:0]
    print(empty.unique(), empty.nunique())
    ids[0] = None
    ids[2] = None
    print(ids.unique(), ids.nunique())
"#;
    let directory = package("optional_sorts_distinct", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[3.0 None 1.0 nan] 2\n\
         [None 'b' 'a'] 2\n\
         [True None] 1\n\
         [5 4] 2\n\
         [] 0\n\
         [None 4] 1\n"
    );
    Ok(())
}

/// `min` and `max` of text that may be `None` give a new `str`, the first and the last by
/// `<` among the texts that are there, and panic when none is, as those of numbers do.
#[test]
fn text_extremes_skip_none() -> io::Result<()> {
    let program = r#"
def main():
    names: Array[str | None, 1] = Array(["delta", None, "", "beta", None, "zeta"])
    print(names.min() == "", names.max())
    one: Array[str | None, 1] = Array([None, "b"])
    low = one.min()
    print(low, one.max(), len(low))
    gone: Array[str | None, 1] = Array([None])
    print(gone.max())
"#;
    let directory = package("optional_sorts_extremes", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(101));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "True zeta\nb b 1\n");
    let report = panic_report(&run);
    assert!(
        report.contains("panic: `gone.max()` of an empty array"),
        "{report}"
    );
    Ok(())
}
