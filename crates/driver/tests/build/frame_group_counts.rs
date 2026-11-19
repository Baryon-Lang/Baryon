//! `size` and `nunique` in `agg` (roadmap 3.13.17, decision 0045), built and run, with the
//! printed frames pandas 3.0.5 prints for the same groups after `reset_index()`.

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

/// `size` counts every row of a group, whatever the column named holds. `nunique` counts
/// the different values of floats, integers, `bool`s and text, and of numbers and text that
/// may hold `None`; a NaN and a `None` are not counted, so a group of them has 0, where
/// `count` is 0 too. Both work on several keys and on a frame of no rows.
#[test]
fn groups_are_counted() {
    let program = r#"
import math


def main():
    names: Array[str | None, 1] = Array(["x", None, "x", "y", None, "z", "z"])
    bids: Array[f64 | None, 1] = Array([1.0, None, 1.0, math.nan, 2.0, None, None])
    df = DataFrame({
        "g": Array([1, 1, 1, 2, 2, 3, 3]),
        "f": Array([1.5, math.nan, 1.5, 2.0, 3.0, math.nan, math.nan]),
        "i": Array([4, 4, 5, 6, 6, 7, 8]),
        "b": Array([True, False, True, True, True, False, False]),
        "s": Array(["a", "b", "a", "c", "c", "d", "e"]),
        "names": names,
        "bids": bids,
    })
    print(df.groupby("g").agg(
        rows=("f", "size"), f=("f", "nunique"), i=("i", "nunique"), b=("b", "nunique"),
        s=("s", "nunique"), names=("names", "nunique"), bids=("bids", "nunique"),
        n=("names", "size"), c=("bids", "count"),
    ))
    print(df.groupby(["g", "b"]).agg(rows=("s", "size"), i=("i", "nunique")))
    print(df[df.g > 5].groupby("g").agg(rows=("s", "size"), i=("names", "nunique")))
"#;
    let expected = [
        "   g  rows  f  i  b  s  names  bids  n  c\n",
        "0  1     3  1  2  2  2      1     1  3  2\n",
        "1  2     2  2  1  1  1      1     1  2  1\n",
        "2  3     2  0  2  1  2      1     0  2  0\n",
        "   g      b  rows  i\n",
        "0  1  False     1  1\n",
        "1  1   True     2  2\n",
        "2  2   True     2  1\n",
        "3  3  False     2  2\n",
        "Empty DataFrame\n",
        "Columns: [g, rows, i]\n",
        "Index: []\n",
    ];
    let (code, stdout, report) = run("frames-group-counts", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
