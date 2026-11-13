//! Leaving out the rows of a frame that repeat, and counting them (roadmap 3.13.9, decision
//! 0045), built and run: `drop_duplicates` of every column and of a `subset`, and
//! `value_counts`, with the printed frames those pandas 3.0.5 prints for the same calls.

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

/// The rows that repeat in every column are left out and the first of each kept, in the
/// frame's order; two NaN count as the same value, as in pandas. `subset` compares one
/// column, by keyword or position, or several, a `bool` column among them. A frame with no
/// rows gives none, the method chains after a sort, and the frame read keeps its rows.
#[test]
fn rows_that_repeat_are_left_out() {
    let program = r#"
import math


class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64
    open: bool


def symbols(trades: DataFrame[Trade]) -> int:
    return len(trades.drop_duplicates(subset="symbol"))


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array([2, 1, 2, 1, 3, 2]),
        "price": Array([189.5, 410.0, math.nan, 410.0, 189.5, math.nan]),
        "volume": Array([100, 250, 50, 250, 75, 50]),
        "open": Array([True, False, True, False, False, True]),
    })
    print(trades.drop_duplicates())
    print(trades.drop_duplicates(subset="symbol"))
    print(trades.drop_duplicates(subset=["price", "open"]))
    print(trades.drop_duplicates(["open"]))
    print(symbols(trades), symbols(trades.head(0)))
    print(trades.sort_values("price").drop_duplicates("symbol").price)
    print(len(trades))
"#;
    let expected = [
        "   symbol  price  volume   open\n",
        "0       2  189.5     100   True\n",
        "1       1  410.0     250  False\n",
        "2       2    NaN      50   True\n",
        "3       3  189.5      75  False\n",
        "   symbol  price  volume   open\n",
        "0       2  189.5     100   True\n",
        "1       1  410.0     250  False\n",
        "2       3  189.5      75  False\n",
        "   symbol  price  volume   open\n",
        "0       2  189.5     100   True\n",
        "1       1  410.0     250  False\n",
        "2       2    NaN      50   True\n",
        "3       3  189.5      75  False\n",
        "   symbol  price  volume   open\n",
        "0       2  189.5     100   True\n",
        "1       1  410.0     250  False\n",
        "3 0\n",
        "[189.5 189.5 410. ]\n",
        "6\n",
    ];
    let (code, stdout, report) = run("frames-distinct", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}

/// `value_counts` gives one row for each distinct value of the columns named, with the
/// number of rows that hold it in `count`: the largest count first, and values with one
/// count in the order they first appear, as in pandas. A NaN is no value, so its rows are
/// not counted; empty text is a value. The columns come in the order named, a `bool`
/// column and every column among them, a frame with no rows gives none, and the result is
/// a frame like any other.
#[test]
fn rows_are_counted_by_their_values() {
    let program = r#"
from math import nan


def main():
    trades = DataFrame({
        "symbol": Array(["b", "a", "b", "a", "c", "", "d", "d"]),
        "price": Array([2.0, 1.0, 2.0, 1.0, 5.0, nan, 7.0, 7.0]),
        "day": Array([1, 1, 1, 2, 2, 2, 3, 3]),
        "live": Array([True, False, True, True, False, True, True, True]),
    })
    print(trades.value_counts("symbol"))
    print(trades.value_counts("price"))
    print(trades.value_counts(["symbol", "day"]))
    print(trades.value_counts(subset=["day", "symbol"]))
    print(trades.value_counts(["live"]))
    print(trades.value_counts())
    top = trades.value_counts("day")
    print(top.day[0], top.count[0], len(top), top.count.sum())
    print(len(trades.head(0).value_counts("symbol")), len(trades))
    print(trades.value_counts("symbol").sort_values("symbol").head(2))
"#;
    let expected = [
        "  symbol  count\n",
        "0      b      2\n",
        "1      a      2\n",
        "2      d      2\n",
        "3      c      1\n",
        "4             1\n",
        "   price  count\n",
        "0    2.0      2\n",
        "1    1.0      2\n",
        "2    7.0      2\n",
        "3    5.0      1\n",
        "  symbol  day  count\n",
        "0      b    1      2\n",
        "1      d    3      2\n",
        "2      a    1      1\n",
        "3      a    2      1\n",
        "4      c    2      1\n",
        "5           2      1\n",
        "   day symbol  count\n",
        "0    1      b      2\n",
        "1    3      d      2\n",
        "2    1      a      1\n",
        "3    2      a      1\n",
        "4    2      c      1\n",
        "5    2             1\n",
        "    live  count\n",
        "0   True      6\n",
        "1  False      2\n",
        "  symbol  price  day   live  count\n",
        "0      b    2.0    1   True      2\n",
        "1      d    7.0    3   True      2\n",
        "2      a    1.0    1  False      1\n",
        "3      a    1.0    2   True      1\n",
        "4      c    5.0    2  False      1\n",
        "1 3 3 8\n",
        "0 8\n",
        "  symbol  count\n",
        "0             1\n",
        "1      a      2\n",
    ];
    let (code, stdout, report) = run("frames-counts", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
