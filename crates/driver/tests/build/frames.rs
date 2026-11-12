//! Frames (roadmap 3.13.2, decision 0045), built and run: a `DataFrame` built from arrays, its
//! columns read, changed and replaced, `len`, `shape`, `columns` and `copy`, and the panics
//! that keep every column of a frame one length, a frame as a generic function's type
//! argument, and a frame printed as pandas prints it.

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

/// `print(df)` and a frame in an f-string give the table pandas 3.0.5 prints for the same
/// frame; the expected text is pandas's own. A float column keeps the decimals its longest
/// value needs, switches to powers of ten for a tiny or a huge value, and prints `NaN` and
/// `-inf`; an `f32` column prints as its values; a frame with no rows prints its columns.
#[test]
fn frames_print_as_pandas_prints() {
    let program = r#"
import math

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
    print(trades)
    print("trades:", trades, sep="\n", end="\n--\n")
    print(f"<{trades.copy()}>")
    single: Array[f32, 1] = Array([0.1, 0.5])
    odd = DataFrame({
        "tiny": Array([1.0e-7, 2.5]),
        "huge": Array([1.0e10, 2.5]),
        "none": Array([math.nan, -math.inf]),
        "single": single,
    })
    print(odd)
    empty: DataFrame[Trade] = DataFrame([])
    print(empty)
"#;
    let expected = [
        "   price  volume     ok\n",
        "0   1.50      10   True\n",
        "1   2.25     200  False\n",
        "2  -3.00       3   True\n",
        "trades:\n",
        "   price  volume     ok\n",
        "0   1.50      10   True\n",
        "1   2.25     200  False\n",
        "2  -3.00       3   True\n",
        "--\n",
        "<   price  volume     ok\n",
        "0   1.50      10   True\n",
        "1   2.25     200  False\n",
        "2  -3.00       3   True>\n",
        "           tiny          huge  none  single\n",
        "0  1.000000e-07  1.000000e+10   NaN     0.1\n",
        "1  2.500000e+00  2.500000e+00  -inf     0.5\n",
        "Empty DataFrame\n",
        "Columns: [price, volume, ok]\n",
        "Index: []\n",
    ];
    let (code, stdout, report) = run("frames-print", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}

/// A frame of more than 60 rows prints its first and last 5 with dots between, a frame wider
/// than 80 characters loses the columns in its middle, and each ends with its size, as in
/// pandas 3.0.5, whose text this is.
#[test]
fn long_and_wide_frames_are_cut_as_pandas_cuts_them() {
    let program = r#"
def main():
    rows = 100
    long = DataFrame({
        "a": Array([f64(row) * 1.5 for row in range(rows)]),
        "b": Array([row for row in range(rows)]),
    })
    print(long)
    wide = DataFrame({
        "column_number_0": Array([0.0, 2.0]),
        "column_number_1": Array([1.0, 2.0]),
        "column_number_2": Array([2.0, 2.0]),
        "column_number_3": Array([3.0, 2.0]),
        "column_number_4": Array([4.0, 2.0]),
        "column_number_5": Array([5.0, 2.0]),
        "column_number_6": Array([6.0, 2.0]),
    })
    print(wide)
"#;
    let expected = [
        "        a   b\n",
        "0     0.0   0\n",
        "1     1.5   1\n",
        "2     3.0   2\n",
        "3     4.5   3\n",
        "4     6.0   4\n",
        "..    ...  ..\n",
        "95  142.5  95\n",
        "96  144.0  96\n",
        "97  145.5  97\n",
        "98  147.0  98\n",
        "99  148.5  99\n",
        "\n",
        "[100 rows x 2 columns]\n",
        "   column_number_0  column_number_1  ...  column_number_5  column_number_6\n",
        "0              0.0              1.0  ...              5.0              6.0\n",
        "1              2.0              2.0  ...              2.0              2.0\n",
        "\n",
        "[2 rows x 7 columns]\n",
    ];
    let (code, stdout, report) = run("frames-print-cut", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
