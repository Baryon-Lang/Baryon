//! Frames (roadmap 3.13.2, decision 0045), built and run: a `DataFrame` built from arrays, its
//! columns read, changed and replaced, `len`, `shape`, `columns` and `copy`, and the panics
//! that keep every column of a frame one length, and a frame as a generic function's type
//! argument.

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

/// A frame is the type argument of a generic function, moved in and returned, alone and
/// inside a tuple: the call site sees the frame's storage where the callee names `T`.
#[test]
fn frames_pass_through_generic_functions() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64

def first[T](x: Owned[T]) -> T:
    return x

def pair[T](x: Owned[T], n: i64) -> tuple[T, i64]:
    return (x, n)

def main():
    df: DataFrame[Trade] = DataFrame({"price": Array([1.5, 2.5]), "volume": Array([1, 2])})
    back = first(df)
    print(len(back), back.price[1])
    both = pair(back, 3)
    print(both[1], both[0].volume[0])
"#;
    let (code, stdout, report) = run("frames-generic", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, "2 2.5\n3 1\n");
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

/// A frame built from a list of rows has the rows' struct as its schema and leaves the list
/// usable; `df[i]` is that struct again, with a negative literal counting from the end, kept,
/// passed on and collected into a new list; and an empty list gives a frame of no rows.
#[test]
fn frames_are_built_from_rows_and_give_rows_back() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64
    buy: bool


def worth(trade: Trade) -> f64:
    return trade.price * trade.volume


def main():
    rows = [Trade(189.5, 100, True), Trade(410.0, 250, False), Trade(171.0, 50, True)]
    trades = DataFrame(rows)
    print(trades.shape, trades.columns, len(rows))
    print(trades.price, trades.volume, trades.buy)
    first = trades[0]
    last = trades[-1]
    print(first.price, first.volume, first.buy, last)
    total = 0.0
    for i in range(len(trades)):
        total += worth(trades[i])
    print(total)
    kept = [trades[1], trades[2]]
    again = DataFrame(kept)
    print(again.price)
    declared: DataFrame[Trade] = DataFrame({
        "price": Array([1.0]),
        "volume": Array([2]),
        "buy": Array([True]),
    })
    print(declared[0])
    empty: DataFrame[Trade] = DataFrame([])
    print(empty.shape, empty.price)
"#;
    let (code, stdout, report) = run("frames-rows", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
(3, 3) ['price', 'volume', 'buy'] 3
[189.5 410.  171. ] [100 250  50] [ True False  True]
189.5 100 True Trade(price=171.0, volume=50, buy=True)
130000.0
[410. 171.]
Trade(price=1.0, volume=2, buy=True)
(0, 3) []
"
    );
}

/// A row number past the end panics as an array index does.
#[test]
fn a_row_out_of_range_panics() {
    let program = r"
class Trade(Struct):
    price: f64
    volume: i64


def main():
    trades = DataFrame([Trade(189.5, 100), Trade(410.0, 250)])
    row = 2
    print(trades[row])
";
    let (code, stdout, report) = run("frames-row-range", program).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: index 2 is out of range for dimension 0 of length 2\n  --> main.byn:10:18\n"
    );
}

/// A field read straight off a row, `df[0].price`, reads the row's new struct: in a call's
/// arguments, in arithmetic, in a condition, and from a frame held in a list.
#[test]
fn fields_are_read_straight_off_a_row() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64
    ok: bool

def main():
    trades: DataFrame[Trade] = DataFrame({
        "price": Array([1.5, 2.25, -3.0]),
        "volume": Array([10, 200, 3]),
        "ok": Array([True, False, True]),
    })
    print(trades[0].price, trades[-1].volume, trades[1].ok)
    total = trades[0].price + trades[1].price
    print(total)
    several = [trades.copy(), trades.copy()]
    print(several[1][2].price)
    if trades[2].ok:
        print(len(trades))
"#;
    let (code, stdout, report) = run("frames-row-fields", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, "1.5 3 False\n3.75\n-3.0\n3\n");
}
