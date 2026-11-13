//! Stacking frames (roadmap 3.13.9, decision 0045), built and run: `concat` of one frame
//! and of several, with the printed frames those pandas 3.0.5 prints for
//! `pd.concat(frames, ignore_index=True)`.

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

/// Three frames stacked, one of them twice, with a NaN and a `bool` column; a row of the
/// result as the frames' struct; one frame alone, which is copied; a frame with no rows;
/// frames borrowed from a caller; and a frame made by a mask. The frames read keep their
/// rows.
#[test]
fn frames_are_stacked() {
    let program = r#"
import math


class Trade(Struct):
    symbol: i64
    price: f64
    open: bool


def both(first: DataFrame[Trade], second: DataFrame[Trade]) -> DataFrame[Trade]:
    return concat([first, second])


def main():
    monday: DataFrame[Trade] = DataFrame({
        "symbol": Array([7, 3]),
        "price": Array([189.5, math.nan]),
        "open": Array([True, False]),
    })
    tuesday: DataFrame[Trade] = DataFrame({
        "symbol": Array([5]),
        "price": Array([20.0]),
        "open": Array([True]),
    })
    week = concat([monday, tuesday, monday])
    print(week)
    print(week[4])
    print(concat([monday]))
    print(concat([monday, monday.head(0)]))
    print(both(tuesday, monday).sort_values("symbol"))
    print(concat([monday[monday.open], tuesday]).price.sum())
    print(len(monday), len(tuesday))
"#;
    let expected = [
        "   symbol  price   open\n",
        "0       7  189.5   True\n",
        "1       3    NaN  False\n",
        "2       5   20.0   True\n",
        "3       7  189.5   True\n",
        "4       3    NaN  False\n",
        "Trade(symbol=3, price=nan, open=False)\n",
        "   symbol  price   open\n",
        "0       7  189.5   True\n",
        "1       3    NaN  False\n",
        "   symbol  price   open\n",
        "0       7  189.5   True\n",
        "1       3    NaN  False\n",
        "   symbol  price   open\n",
        "0       3    NaN  False\n",
        "1       5   20.0   True\n",
        "2       7  189.5   True\n",
        "209.5\n",
        "2 1\n",
    ];
    let (code, stdout, report) = run("frames-stacked", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
