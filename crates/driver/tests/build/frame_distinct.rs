//! Leaving out the rows of a frame that repeat (roadmap 3.13.9, decision 0045), built and
//! run: `drop_duplicates` of every column and of a `subset`, with the printed frames those
//! pandas 3.0.5 prints for the same calls.

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
