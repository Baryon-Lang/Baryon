//! pandas's arguments on `drop_duplicates`, `value_counts`, `describe` and `sort_values`
//! (roadmap 3.13.16, decision 0045), built and run, with the printed frames those pandas
//! 3.0.5 prints for the same calls, after `reset_index()` where pandas makes an index.

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

/// Every new argument once, on a frame with rows that repeat, a NaN and a `None`.
/// `keep="last"` and `keep=False` keep rows in the order they stand in. `value_counts`
/// sorted rising or not at all keeps values with one count in the order they first appear;
/// `normalize=True` names the last column `proportion`; `dropna=False` counts a NaN and a
/// `None` as values. `describe` shows the percentiles given, sorted, named as pandas names
/// them, and none for an empty list. `na_position="first"` puts the rows with a missing
/// value first in each column sorted by, whatever its direction.
const PROGRAM: &str = r#"
from math import nan


def main():
    sizes: Array[i64 | None, 1] = Array([10, None, 10, 30, None, 5, 7])
    trades = DataFrame({
        "symbol": Array(["b", "a", "b", "c", "a", "d", "b"]),
        "day": Array([1, 1, 1, 2, 2, 2, 3]),
        "price": Array([2.5, nan, 2.5, 4.0, nan, 1.5, 3.0]),
        "size": sizes,
    })
    print(trades.drop_duplicates(subset=["symbol", "day"], keep="last"))
    print(trades.drop_duplicates("symbol", keep=False))
    print(trades.drop_duplicates(keep="first"))
    print(trades.value_counts("symbol", ascending=True))
    print(trades.value_counts("symbol", sort=False))
    print(trades.value_counts("symbol", normalize=True))
    print(trades.value_counts("price"))
    print(trades.value_counts("price", dropna=False))
    print(trades.value_counts(["size"], dropna=False, normalize=True, sort=False))
    print(trades[["day", "price"]].describe(percentiles=[0.9, 0.1]))
    print(trades[["day", "price"]].describe(percentiles=[0, 0.125, 0.999, 1]))
    print(trades[["day", "price"]].describe([]))
    print(trades.sort_values("price", na_position="first"))
    print(trades.sort_values(["size", "price"], ascending=[False, True], na_position="first"))
    print(trades.sort_values(by="symbol", na_position="last"))
"#;

/// What [`PROGRAM`] prints, and pandas prints for the same calls.
const EXPECTED: [&str; 97] = [
    "  symbol  day  price  size\n",
    "0      a    1    NaN  <NA>\n",
    "1      b    1    2.5    10\n",
    "2      c    2    4.0    30\n",
    "3      a    2    NaN  <NA>\n",
    "4      d    2    1.5     5\n",
    "5      b    3    3.0     7\n",
    "  symbol  day  price  size\n",
    "0      c    2    4.0    30\n",
    "1      d    2    1.5     5\n",
    "  symbol  day  price  size\n",
    "0      b    1    2.5    10\n",
    "1      a    1    NaN  <NA>\n",
    "2      c    2    4.0    30\n",
    "3      a    2    NaN  <NA>\n",
    "4      d    2    1.5     5\n",
    "5      b    3    3.0     7\n",
    "  symbol  count\n",
    "0      c      1\n",
    "1      d      1\n",
    "2      a      2\n",
    "3      b      3\n",
    "  symbol  count\n",
    "0      b      3\n",
    "1      a      2\n",
    "2      c      1\n",
    "3      d      1\n",
    "  symbol  proportion\n",
    "0      b    0.428571\n",
    "1      a    0.285714\n",
    "2      c    0.142857\n",
    "3      d    0.142857\n",
    "   price  count\n",
    "0    2.5      2\n",
    "1    4.0      1\n",
    "2    1.5      1\n",
    "3    3.0      1\n",
    "   price  count\n",
    "0    2.5      2\n",
    "1    NaN      2\n",
    "2    4.0      1\n",
    "3    1.5      1\n",
    "4    3.0      1\n",
    "   size  proportion\n",
    "0    10    0.285714\n",
    "1  <NA>    0.285714\n",
    "2    30    0.142857\n",
    "3     5    0.142857\n",
    "4     7    0.142857\n",
    "  statistic       day     price\n",
    "0     count  7.000000  5.000000\n",
    "1      mean  1.714286  2.700000\n",
    "2       std  0.755929  0.908295\n",
    "3       min  1.000000  1.500000\n",
    "4       10%  1.000000  1.900000\n",
    "5       90%  2.400000  3.600000\n",
    "6       max  3.000000  4.000000\n",
    "  statistic       day     price\n",
    "0     count  7.000000  5.000000\n",
    "1      mean  1.714286  2.700000\n",
    "2       std  0.755929  0.908295\n",
    "3       min  1.000000  1.500000\n",
    "4        0%  1.000000  1.500000\n",
    "5     12.5%  1.000000  2.000000\n",
    "6     99.9%  2.994000  3.996000\n",
    "7      100%  3.000000  4.000000\n",
    "8       max  3.000000  4.000000\n",
    "  statistic       day     price\n",
    "0     count  7.000000  5.000000\n",
    "1      mean  1.714286  2.700000\n",
    "2       std  0.755929  0.908295\n",
    "3       min  1.000000  1.500000\n",
    "4       max  3.000000  4.000000\n",
    "  symbol  day  price  size\n",
    "0      a    1    NaN  <NA>\n",
    "1      a    2    NaN  <NA>\n",
    "2      d    2    1.5     5\n",
    "3      b    1    2.5    10\n",
    "4      b    1    2.5    10\n",
    "5      b    3    3.0     7\n",
    "6      c    2    4.0    30\n",
    "  symbol  day  price  size\n",
    "0      a    1    NaN  <NA>\n",
    "1      a    2    NaN  <NA>\n",
    "2      c    2    4.0    30\n",
    "3      b    1    2.5    10\n",
    "4      b    1    2.5    10\n",
    "5      b    3    3.0     7\n",
    "6      d    2    1.5     5\n",
    "  symbol  day  price  size\n",
    "0      a    1    NaN  <NA>\n",
    "1      a    2    NaN  <NA>\n",
    "2      b    1    2.5    10\n",
    "3      b    1    2.5    10\n",
    "4      b    3    3.0     7\n",
    "5      c    2    4.0    30\n",
    "6      d    2    1.5     5\n",
];

#[test]
fn the_arguments_do_what_pandas_does() {
    let (code, stdout, report) = run("frames-pandas-arguments", PROGRAM).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, EXPECTED.concat());
}

/// A frame with no rows goes through every new argument: no row is kept or counted, the
/// proportions divide nothing, and a percentile of no values is NaN.
#[test]
fn a_frame_with_no_rows_takes_the_arguments() {
    let program = r#"
def main():
    df = DataFrame({"symbol": Array(["a"]), "price": Array([1.5])})
    none = df[df["price"] > 2.0]
    print(none.drop_duplicates(keep="last"))
    print(none.drop_duplicates(keep=False))
    print(none.value_counts("symbol", normalize=True, ascending=True, dropna=False))
    print(none.describe(percentiles=[0.5]))
    print(none.sort_values("price", na_position="first"))
"#;
    let expected = [
        "Empty DataFrame\n",
        "Columns: [symbol, price]\n",
        "Index: []\n",
        "Empty DataFrame\n",
        "Columns: [symbol, price]\n",
        "Index: []\n",
        "Empty DataFrame\n",
        "Columns: [symbol, proportion]\n",
        "Index: []\n",
        "  statistic  price\n",
        "0     count    0.0\n",
        "1      mean    NaN\n",
        "2       std    NaN\n",
        "3       min    NaN\n",
        "4       50%    NaN\n",
        "5       max    NaN\n",
        "Empty DataFrame\n",
        "Columns: [symbol, price]\n",
        "Index: []\n",
    ];
    let (code, stdout, report) = run("frames-pandas-arguments-empty", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
