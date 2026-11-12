//! Sorting the rows of a frame (roadmap 3.13.5, decision 0045), built and run:
//! `sort_values` by one column and by several, rising and falling, with the printed frames
//! those pandas 3.0.5 prints for the same sorts.

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

/// A frame sorted by a float column with a NaN, which goes last in both directions; by two
/// columns, where rows equal in the first are ordered by the second; with one direction for
/// each column and one for all; by a `bool` column; with a direction known only when
/// running; and after a mask. Rows equal in every column sorted by keep their order, and
/// the frame sorted keeps its own.
#[test]
fn rows_are_sorted_by_one_column_and_by_several() {
    let program = r#"
import math


class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64
    open: bool


def cheapest(trades: DataFrame[Trade], rising: bool) -> Trade:
    return trades.sort_values("price", ascending=rising)[0]


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array([2, 1, 2, 1, 3, 1]),
        "price": Array([189.5, 410.0, math.nan, 171.0, 189.5, 20.0]),
        "volume": Array([100, 250, 50, 250, 75, 10]),
        "open": Array([True, False, True, True, False, False]),
    })
    print(trades.sort_values("price"))
    print(trades.sort_values("price", ascending=False))
    print(trades.sort_values(["symbol", "price"]))
    print(trades.sort_values(by=["symbol", "volume"], ascending=[True, False]))
    print(trades.sort_values(["open", "volume"], ascending=False))
    print(cheapest(trades, True), cheapest(trades, False))
    print(trades.sort_values(["volume"]).head(2))
    print(trades[trades.price > 100.0].sort_values("volume", ascending=False).price)
    print(trades)
"#;
    let expected = [
        "   symbol  price  volume   open\n",
        "0       1   20.0      10  False\n",
        "1       1  171.0     250   True\n",
        "2       2  189.5     100   True\n",
        "3       3  189.5      75  False\n",
        "4       1  410.0     250  False\n",
        "5       2    NaN      50   True\n",
        "   symbol  price  volume   open\n",
        "0       1  410.0     250  False\n",
        "1       2  189.5     100   True\n",
        "2       3  189.5      75  False\n",
        "3       1  171.0     250   True\n",
        "4       1   20.0      10  False\n",
        "5       2    NaN      50   True\n",
        "   symbol  price  volume   open\n",
        "0       1   20.0      10  False\n",
        "1       1  171.0     250   True\n",
        "2       1  410.0     250  False\n",
        "3       2  189.5     100   True\n",
        "4       2    NaN      50   True\n",
        "5       3  189.5      75  False\n",
        "   symbol  price  volume   open\n",
        "0       1  410.0     250  False\n",
        "1       1  171.0     250   True\n",
        "2       1   20.0      10  False\n",
        "3       2  189.5     100   True\n",
        "4       2    NaN      50   True\n",
        "5       3  189.5      75  False\n",
        "   symbol  price  volume   open\n",
        "0       1  171.0     250   True\n",
        "1       2  189.5     100   True\n",
        "2       2    NaN      50   True\n",
        "3       1  410.0     250  False\n",
        "4       3  189.5      75  False\n",
        "5       1   20.0      10  False\n",
        "Trade(symbol=1, price=20.0, volume=10, open=False) Trade(symbol=1, price=410.0, volume=250, open=False)\n",
        "   symbol  price  volume   open\n",
        "0       1   20.0      10  False\n",
        "1       2    NaN      50   True\n",
        "[410.  171.  189.5 189.5]\n",
        "   symbol  price  volume   open\n",
        "0       2  189.5     100   True\n",
        "1       1  410.0     250  False\n",
        "2       2    NaN      50   True\n",
        "3       1  171.0     250   True\n",
        "4       3  189.5      75  False\n",
        "5       1   20.0      10  False\n",
    ];
    let (code, stdout, report) = run("frames-sorted", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
