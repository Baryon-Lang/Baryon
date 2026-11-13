//! Joining two frames (roadmap 3.13.8, decision 0045), built and run: the inner `merge` on
//! one key and on several, with the printed frames those pandas 3.0.5 prints for the same
//! joins once both are sorted the same way.

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

/// An inner join on an integer key where rows match several rows, one row and none; on two
/// keys, one of them a float; on a `bool` key; on a float key, where NaN joins NaN as in
/// pandas; with a column the two frames share, which gets `_x` and `_y`; from borrowed
/// frames in a function; with a frame of no rows; and on the frame `groupby` gives.
#[test]
fn frames_are_joined_on_their_keys() {
    let program = r#"
import math


class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64


class Listing(Struct):
    symbol: i64
    lot: i64
    price: f64


def joined_value(trades: DataFrame[Trade], listings: DataFrame[Listing]) -> i64:
    both = trades.merge(listings, on="symbol")
    return (both.volume * both.lot).sum()


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array([7, 3, 7, 5, 9, 3]),
        "price": Array([189.5, 410.0, 191.0, 12.0, 171.0, math.nan]),
        "volume": Array([100, 250, 300, 1, 75, 50]),
    })
    listings: DataFrame[Listing] = DataFrame({
        "symbol": Array([3, 7, 8, 7]),
        "lot": Array([10, 100, 1, 50]),
        "price": Array([400.0, 190.0, 1.0, 191.0]),
    })
    both = trades.merge(listings, on="symbol").sort_values(["volume", "lot"])
    print(both)
    print(both.columns)
    print(joined_value(trades, listings))
    print(trades.merge(listings, on=["symbol", "price"], how="inner"))
    flags = DataFrame({"open": Array([True, False, True]), "x": Array([1.5, math.nan, 2.5])})
    names = DataFrame({"x": Array([math.nan, 2.5, 2.5]), "open": Array([False, True, False]), "n": Array([1, 2, 3])})
    print(flags.merge(names, on="open").sort_values(["x_x", "n"]))
    print(flags.merge(names, on=["x"]).sort_values(["open_x", "n"]))
    print(flags.merge(names, on=["x", "open"]))
    print(trades.merge(listings[listings.lot > 1000], on="symbol"))
    print(trades.groupby("symbol").agg(total=("volume", "sum")).merge(listings, on="symbol").sort_values("lot"))
"#;
    let expected = [
        "   symbol  price_x  volume  lot  price_y\n",
        "0       3      NaN      50   10    400.0\n",
        "1       7    189.5     100   50    191.0\n",
        "2       7    189.5     100  100    190.0\n",
        "3       3    410.0     250   10    400.0\n",
        "4       7    191.0     300   50    191.0\n",
        "5       7    191.0     300  100    190.0\n",
        "['symbol', 'price_x', 'volume', 'lot', 'price_y']\n",
        "63000\n",
        "   symbol  price  volume  lot\n",
        "0       7  191.0     300   50\n",
        "    open  x_x  x_y  n\n",
        "0   True  1.5  2.5  2\n",
        "1   True  2.5  2.5  2\n",
        "2  False  NaN  NaN  1\n",
        "3  False  NaN  2.5  3\n",
        "   open_x    x  open_y  n\n",
        "0   False  NaN   False  1\n",
        "1    True  2.5    True  2\n",
        "2    True  2.5   False  3\n",
        "    open    x  n\n",
        "0  False  NaN  1\n",
        "1   True  2.5  2\n",
        "Empty DataFrame\n",
        "Columns: [symbol, price_x, volume, lot, price_y]\n",
        "Index: []\n",
        "   symbol  total  lot  price\n",
        "0       3    300   10  400.0\n",
        "1       7    400   50  191.0\n",
        "2       7    400  100  190.0\n",
    ];
    let (code, stdout, report) = run("frames-joined", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
