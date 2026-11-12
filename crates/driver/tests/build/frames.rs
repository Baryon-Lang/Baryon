//! Frames (roadmap 3.13.2, decision 0045), built and run: a `DataFrame` built from arrays, its
//! columns read, changed and replaced, `len`, `shape`, `columns` and `copy`, and the panics
//! that keep every column of a frame one length.

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

/// A frame declared by a struct and one whose columns the compiler works out: passed to a
/// read-only and to an `InOut` parameter, returned, kept in a list, copied and moved; columns
/// read by attribute and by name, used in arithmetic, masks, reductions and a loop, changed
/// in place and replaced; and a column named `shape`, which only `df["shape"]` reaches.
#[test]
fn frames_hold_columns_that_work_as_arrays() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def value(trades: DataFrame[Trade]) -> f64:
    return (trades.price * trades["volume"]).sum()


def reprice(trades: InOut[DataFrame[Trade]], factor: f64):
    trades.price *= factor
    trades["volume"][0] = 7


def load(n: i64) -> DataFrame[Trade]:
    prices = Array([1.5 * i for i in range(n)])
    volumes = Array([i * i for i in range(n)])
    return DataFrame({"price": prices, "volume": volumes})


def main():
    trades: DataFrame[Trade] = DataFrame({
        "price": Array([189.5, 410.0, 171.0]),
        "volume": Array([100, 250, 50]),
    })
    print(len(trades), trades.shape, trades.columns)
    print(trades.price, trades["volume"])
    print(value(trades))
    print(trades.price[trades.volume > 75])
    print(trades.volume.max(), trades.price.mean())
    for price in trades.price:
        print(price)
    kept = trades.copy()
    reprice(trades, 2.0)
    print(trades.price, trades.volume)
    print(kept.price, kept.volume)
    trades["price"] = Array([1.0, 2.0, 3.0])
    trades.volume = trades.volume * 2
    print(trades.price, trades.volume)
    summary = DataFrame({"shape": Array([True, False]), "count": Array([3, 4])})
    print(summary.shape, summary["shape"], summary.count)
    print(value(DataFrame({"price": Array([2.0]), "volume": Array([21])})))
    several = [load(2), load(4)]
    print(len(several[1]), several[1].volume)
    moved = trades
    print(moved.shape)
"#;
    let (code, stdout, report) = run("frames-columns", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
3 (3, 2) ['price', 'volume']
[189.5 410.  171. ] [100 250  50]
130000.0
[189.5 410. ]
250 256.8333333333333
189.5
410.0
171.0
[379. 820. 342.] [  7 250  50]
[189.5 410.  171. ] [100 250  50]
[1. 2. 3.] [ 14 500 100]
(2, 2) [ True False] [3 4]
42.0
4 [0 1 4 9]
(3, 2)
"
    );
}

/// Columns of two lengths panic when the frame is built, and a new column of another length
/// panics when it is assigned, so a frame's columns always have one length.
#[test]
fn columns_of_two_lengths_panic() {
    let building = r#"
def main():
    frame = DataFrame({"a": Array([1, 2]), "b": Array([1.0])})
    print(frame.shape)
"#;
    let (code, stdout, report) = run("frames-lengths", building).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 2 and 1\n  --> main.byn:3:13\n"
    );
    let assigning = r#"
def main():
    frame = DataFrame({"a": Array([1, 2]), "b": Array([1.0, 2.0])})
    frame["b"] = Array([1.0, 2.0, 3.0])
    print(frame.shape)
"#;
    let (code, stdout, report) = run("frames-assigned-length", assigning).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 2 and 3\n  --> main.byn:4:5\n"
    );
}
