//! Arrays with missing values, `Array[T | None, 1]` (roadmap 3.13.6, decision 0045), built
//! and run: one made from a list, counted, read one element at a time, asked where its gaps
//! are, copied, passed, returned, moved and kept in a tuple, narrowed by `dropna` and
//! `fillna`, and reduced by the methods that skip `None`.

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

/// An array with missing values holds the items of the list it is made from, `None`s among
/// them, across more than one byte of validity bits, and leaves the list usable. An element
/// reads as the number or `None`, by position and from the end; `isna` and `notna` give the
/// masks pandas gives for the same values. A function takes one and returns one; floats,
/// 32-bit integers and `bool`s are held; an empty one has no elements; a moved one, one in a
/// tuple and a copy still read.
#[test]
fn arrays_with_missing_values_are_built_and_read() -> io::Result<()> {
    let program = r"
def total(values: Array[f64 | None, 1]) -> f64:
    result = 0.0
    for at in range(len(values)):
        value = values[at]
        if value is not None:
            result += value
    return result


def bids() -> Array[f64 | None, 1]:
    return Array([1.5, None, 2.5, None, None, 6.0, 7.0, 8.0, 9.0, None])


def main():
    values = bids()
    print(len(values), total(values))
    print(values.isna())
    print(values.notna())
    print(values[0], values[1], values[-1], values[-2])
    flags: Array[bool | None, 1] = Array([True, None])
    print(flags[0], flags[1], flags[-1], flags.isna())
    counts: Array[i32 | None, 1] = Array([1, None, 3])
    print(counts[2], counts[1], len(counts))
    empty: Array[f64 | None, 1] = Array([])
    print(len(empty), empty.isna())
    kept = values
    pair = (kept, counts)
    print(len(pair[0]), pair[1][0], pair[0][8])
    items: list[i64 | None] = [None, 4]
    made = Array(items)
    position = 1
    print(made[position], made[0], len(items), made.notna())
    twin = made.copy()
    print(twin[1], twin[0], made[1], len(twin))
";
    let (code, output, _) = run("optional_arrays", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "10 34.0\n\
         [False  True False  True  True False False False False  True]\n\
         [ True False  True False False  True  True  True  True False]\n\
         1.5 None None 9.0\n\
         True None None [False  True]\n\
         3 None 3\n\
         0 []\n\
         10 1 9.0\n\
         4 None 2 [False  True]\n\
         4 None 4 2\n"
    );
    Ok(())
}

/// An index past the end of an array with missing values panics as an array's does, naming
/// the index and the length.
#[test]
fn an_index_past_the_end_of_an_array_with_missing_values_panics() -> io::Result<()> {
    let program = r"
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None])
    at = 2
    print(bids[at])
";
    let (code, output, report) = run("optional_array_range", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: index 2 is out of range for dimension 0 of length 2"),
        "{report}"
    );
    Ok(())
}

/// `dropna()` gives the elements that are there, in order, as an array of numbers a function
/// of `Array[f64, 1]` takes, and `fillna(value)` every element with the value where one was
/// `None`, the value given by position or as `value=` and converted to the element's type.
/// The array they read is left as it was. Floats, 32-bit integers and `bool`s are narrowed;
/// an array of `None`s only and an empty one drop to no elements.
#[test]
fn missing_values_are_dropped_and_filled() -> io::Result<()> {
    let program = r"
def bids() -> Array[f64 | None, 1]:
    return Array([1.5, None, 2.5, None, None, 6.0, 7.0, 8.0, 9.0, None])


def mean(values: Array[f64, 1]) -> f64:
    return values.sum() / f64(len(values))


def main():
    values = bids()
    there = values.dropna()
    print(there, len(there), mean(there))
    print(values.fillna(0.0))
    print(values.fillna(value=-1), mean(values.fillna(mean(there))))
    print(len(values), values[1], values.isna()[1])
    counts: Array[i32 | None, 1] = Array([1, None, 3])
    print(counts.dropna(), counts.fillna(2), counts.fillna(2).sum())
    flags: Array[bool | None, 1] = Array([None, True, None])
    print(flags.dropna(), flags.fillna(False))
    none: Array[i64 | None, 1] = Array([None, None])
    print(none.dropna(), len(none.dropna()), none.fillna(7))
    empty: Array[f64 | None, 1] = Array([])
    print(empty.dropna(), empty.fillna(1.0))
";
    let (code, output, _) = run("optional_arrays_narrowed", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "[1.5 2.5 6.  7.  8.  9. ] 6 5.666666666666667\n\
         [1.5 0.  2.5 0.  0.  6.  7.  8.  9.  0. ]\n\
         [ 1.5 -1.   2.5 -1.  -1.   6.   7.   8.   9.  -1. ] 5.666666666666666\n\
         10 None True\n\
         [1 3] [1 2 3] 6\n\
         [ True] [False  True False]\n\
         [] 0 [7 7]\n\
         [] []\n"
    );
    Ok(())
}

/// The reductions skip the elements that are `None`: `count()` counts those that are there,
/// `sum()` and `mean()` add them, with the bits `dropna()` and the array's own method give,
/// and `min()` and `max()` compare them, a NaN that is there winning as in any array. With
/// no element there the count and the sum are 0 and the mean is NaN. Floats of both widths
/// and integers reduce, a `bool` array counts, and a frame's column reduces as an array.
#[test]
fn reductions_skip_missing_values() -> io::Result<()> {
    let program = r#"
from math import nan


class Quote(Struct):
    bid: f64 | None
    size: i64 | None


def spread(bids: Array[f64 | None, 1]) -> f64:
    return bids.max() - bids.min()


def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, None, None, 6.0, 7.0, 8.0, 9.0, None])
    print(bids.count(), len(bids), bids.sum(), bids.mean(), bids.min(), bids.max(), spread(bids))
    print(bids.sum() == bids.dropna().sum(), bids.mean() == bids.dropna().mean())
    sizes: Array[i32 | None, 1] = Array([4, None, 3])
    print(sizes.count(), sizes.sum(), sizes.mean(), sizes.min(), sizes.max())
    small: Array[f32 | None, 1] = Array([0.5, None, 0.25])
    print(small.sum(), small.mean(), small.max())
    none: Array[f64 | None, 1] = Array([None, None])
    print(none.count(), none.sum(), none.mean())
    whole: Array[i64 | None, 1] = Array([None])
    print(whole.count(), whole.sum(), whole.mean())
    empty: Array[i64 | None, 1] = Array([])
    print(empty.count(), empty.sum())
    flags: Array[bool | None, 1] = Array([True, None])
    print(flags.count())
    quotes: DataFrame[Quote] = DataFrame(
        {"bid": Array([1.0, None, 3.0]), "size": Array([None, 2, 4])}
    )
    print(quotes.bid.mean(), quotes["size"].sum(), quotes.bid.count(), quotes.size.max())
    odd: Array[f64 | None, 1] = Array([2.0, None, nan, 1.0])
    print(odd.min(), odd.max(), odd.count(), odd.sum())
"#;
    let (code, output, _) = run("optional_arrays_reduced", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "6 10 34.0 5.666666666666667 1.5 9.0 7.5\n\
         True True\n\
         2 7 3.5 3 4\n\
         0.75 0.375 0.5\n\
         0 0.0 nan\n\
         0 0 nan\n\
         0 0\n\
         1\n\
         2.0 6 2 4\n\
         nan nan 3 nan\n"
    );
    Ok(())
}

/// `min()` of an array whose elements are all `None` panics as `min` of an empty array
/// does, since no element is the smallest.
#[test]
fn the_smallest_of_no_elements_panics() -> io::Result<()> {
    let program = r"
def main():
    none: Array[f64 | None, 1] = Array([None, None])
    print(none.min())
";
    let (code, output, report) = run("optional_arrays_no_least", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: `none.min()` of an empty array"),
        "{report}"
    );
    Ok(())
}
