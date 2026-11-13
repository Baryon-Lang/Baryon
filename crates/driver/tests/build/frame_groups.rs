//! Grouping the rows of a frame (roadmap 3.13.7, decision 0045), built and run:
//! `groupby(...).agg(...)` by one key and by two, with every reduction, and the printed
//! frames those pandas 3.0.5 prints for the same groups.

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

/// A frame grouped by an integer column, by a `bool` and an integer column together, and by
/// a float column, whose NaN row belongs to no group; reduced by all ten reductions, over
/// integer, float, `f32` and `bool` columns, with NaN left out of each; from a borrowed
/// frame in a function; and a frame with no rows, which gives no groups.
#[test]
fn rows_are_grouped_and_reduced() {
    let program = r#"
import math


class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64
    open: bool
    fee: f32


def totals(trades: DataFrame[Trade]) -> f64:
    by_symbol = trades.groupby("symbol").agg(total=("volume", "sum"), mean=("price", "mean"))
    return by_symbol.mean.sum()


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array([2, 1, 2, 1, 3, 1]),
        "price": Array([189.5, 410.0, math.nan, 171.0, 189.5, 20.0]),
        "volume": Array([100, 250, 50, 250, 75, 10]),
        "open": Array([True, False, True, True, False, False]),
        "fee": Array([f32(0.5), f32(1.5), f32(0.25), f32(2.0), f32(1.0), f32(0.75)]),
    })
    print(trades.groupby("symbol").agg(
        total=("volume", "sum"),
        mean_price=("price", "mean"),
        trades=("price", "count"),
        low=("price", "min"),
        high=("price", "max"),
    ))
    print(trades.groupby(["open", "symbol"]).agg(
        first=("volume", "first"),
        last=("volume", "last"),
        middle=("volume", "median"),
        spread=("price", "std"),
        var=("volume", "var"),
        fees=("fee", "sum"),
        fee=("fee", "mean"),
    ))
    print(trades.groupby("price").agg(any_open=("open", "max"), opened=("open", "sum"), n=("symbol", "count")))
    print(totals(trades))
    print(trades[trades.volume > 1000].groupby(by="symbol").agg(n=("price", "count")))
"#;
    let expected = [
        "   symbol  total  mean_price  trades    low   high\n",
        "0       1    510  200.333333       3   20.0  410.0\n",
        "1       2    150  189.500000       1  189.5  189.5\n",
        "2       3     75  189.500000       1  189.5  189.5\n",
        "    open  symbol  first  last  middle      spread      var  fees    fee\n",
        "0  False       1    250    10   130.0  275.771645  28800.0  2.25  1.125\n",
        "1  False       3     75    75    75.0         NaN      NaN  1.00  1.000\n",
        "2   True       1    250   250   250.0         NaN      NaN  2.00  2.000\n",
        "3   True       2    100    50    75.0         NaN   1250.0  0.75  0.375\n",
        "   price  any_open  opened  n\n",
        "0   20.0     False       0  1\n",
        "1  171.0      True       1  1\n",
        "2  189.5      True       1  2\n",
        "3  410.0     False       0  1\n",
        "579.3333333333334\n",
        "Empty DataFrame\n",
        "Columns: [symbol, n]\n",
        "Index: []\n",
    ];
    let (code, stdout, report) = run("frames-grouped", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
