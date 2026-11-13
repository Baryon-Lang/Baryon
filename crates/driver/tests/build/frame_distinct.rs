//! Leaving out the rows of a frame that repeat, counting them, and summing up its columns
//! (roadmap 3.13.9, decision 0045), built and run: `drop_duplicates` of every column and of
//! a `subset`, `value_counts` and `describe`, with the printed frames those pandas 3.0.5
//! prints for the same calls.

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

/// `describe` gives the count, mean, sample standard deviation, smallest, quartiles and
/// largest of each column of numbers, an integer and an `f32` column among them, with a
/// NaN left out and the columns of `str` and `bool` left out, as pandas gives them; pandas
/// has the names as its row index, put here in the column `statistic`. One row has no
/// standard deviation, and no rows have a count of 0 and NaN for the rest. The frame read
/// keeps its rows.
#[test]
fn columns_of_numbers_are_described() {
    let program = r#"
from math import nan


def main():
    trades = DataFrame({
        "symbol": Array(["b", "a", "b", "a", "c", "b", "d"]),
        "price": Array([2.0, 1.0, 2.5, 1.0, 5.0, nan, 7.25]),
        "volume": Array([100, 250, 50, 250, 75, 10, 3]),
        "live": Array([True, False, True, True, False, True, True]),
        "size": Array([f32(1.5), f32(2.5), f32(0.5), f32(4.0), f32(1.0), f32(8.0), f32(2.0)]),
    })
    summary = trades.describe()
    print(summary)
    print(summary.price[1], summary.statistic[5], len(summary), summary.columns)
    print(trades.head(1).describe())
    print(trades.head(0).describe())
    print(trades[["volume"]].describe().volume)
    print(len(trades))
"#;
    let expected = [
        "  statistic    price      volume      size\n",
        "0     count  6.00000    7.000000  7.000000\n",
        "1      mean  3.12500  105.428571  2.785714\n",
        "2       std  2.49875  104.425184  2.563480\n",
        "3       min  1.00000    3.000000  0.500000\n",
        "4       25%  1.25000   30.000000  1.250000\n",
        "5       50%  2.25000   75.000000  2.000000\n",
        "6       75%  4.37500  175.000000  3.250000\n",
        "7       max  7.25000  250.000000  8.000000\n",
        "3.125 50% 8 ['statistic', 'price', 'volume', 'size']\n",
        "  statistic  price  volume  size\n",
        "0     count    1.0     1.0   1.0\n",
        "1      mean    2.0   100.0   1.5\n",
        "2       std    NaN     NaN   NaN\n",
        "3       min    2.0   100.0   1.5\n",
        "4       25%    2.0   100.0   1.5\n",
        "5       50%    2.0   100.0   1.5\n",
        "6       75%    2.0   100.0   1.5\n",
        "7       max    2.0   100.0   1.5\n",
        "  statistic  price  volume  size\n",
        "0     count    0.0     0.0   0.0\n",
        "1      mean    NaN     NaN   NaN\n",
        "2       std    NaN     NaN   NaN\n",
        "3       min    NaN     NaN   NaN\n",
        "4       25%    NaN     NaN   NaN\n",
        "5       50%    NaN     NaN   NaN\n",
        "6       75%    NaN     NaN   NaN\n",
        "7       max    NaN     NaN   NaN\n",
        "[  7.         105.42857143 104.42518397   3.          30.\n",
        "  75.         175.         250.        ]\n",
        "7\n",
    ];
    let (code, stdout, report) = run("frames-described", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
