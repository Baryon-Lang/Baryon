//! Text columns of a `DataFrame` (roadmap 3.13.3, decision 0045), built and run: a frame with
//! a column of `str` made from its columns, counted, copied, moved, passed and returned, and
//! its column read, looped over and replaced.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
pub(super) fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
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

/// A program that builds frames with a text column, with a schema and without, the text
/// column first, last and alone, and reads them every way that is built. Its last line gives
/// a frame columns of two lengths.
const PROGRAM: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64


def cheapest(trades: DataFrame[Trade]) -> str:
    best = 0
    for i in range(len(trades)):
        if trades.price[i] < trades.price[best]:
            best = i
    return trades.symbol[best].copy()


def load(count: int) -> DataFrame[Trade]:
    names: list[str] = []
    for i in range(count):
        names.append(f"S{i}")
    return DataFrame({"symbol": Array(names), "price": Array.zeros(count)})


def main():
    words = ["IBM", "", "AAPL", "βeta"]
    prices = Array([3.0, 1.0, 2.0, 4.0])
    trades: DataFrame[Trade] = DataFrame({"symbol": Array(words), "price": prices})
    print(len(trades), trades.shape, trades.columns)
    print(trades.symbol[0], trades["symbol"][3], len(trades.symbol))
    print(cheapest(trades) == "", trades.price.sum())
    for name in trades.symbol:
        print(name)
    kept = trades.copy()
    moved = trades
    print(kept.symbol[2], moved.symbol[2], len(load(3)), load(2).symbol[1])
    loose = DataFrame({"n": Array([1, 2]), "tag": Array(["a", "b"])})
    print(loose.tag[1], loose.n[1], len(loose))
    first = DataFrame({"tag": Array(["x", "yy", "zzz"])})
    print(len(first), first.shape)
    first.tag = Array(["p", "q", "r"])
    print(first.tag[2])
    for position, tag in enumerate(first.tag):
        print(position, tag)
    held = [load(1), load(2)]
    print(len(held[1]), held[1].symbol[1])
    bad = DataFrame({"tag": Array(["x"]), "n": Array([1, 2])})
"#;

/// A frame with a text column holds the strings it was built from, an empty one and one of
/// two-byte characters among them. `len`, `shape` and `columns` count it whether the text
/// column is first or not; the column is read by position and in loops; a copy and the moved
/// original both read; a function takes one and returns one; a list holds them; and a new
/// text array replaces the column. Every line is what pandas gives for the same columns. A
/// text column of another length than a column of numbers panics as two arrays do.
#[test]
fn frames_with_a_text_column_are_built_read_and_copied() {
    let expected = [
        "4 (4, 2) ['symbol', 'price']\n",
        "IBM βeta 4\n",
        "True 10.0\n",
        "IBM\n",
        "\n",
        "AAPL\n",
        "βeta\n",
        "AAPL AAPL 3 S1\n",
        "b 2 2\n",
        "3 (3, 1)\n",
        "r\n",
        "0 p\n",
        "1 q\n",
        "2 r\n",
        "2 S1\n",
    ];
    let (code, stdout, report) = run("text-columns", PROGRAM).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 1 and 2\n  --> main.byn:44:11\n"
    );
}

/// A program that prints frames with a text column: beside numbers, first and last, alone,
/// in an f-string, with cells of exactly 50 and of 60 characters, of 100 rows, and of none.
const PRINTED: &str = r#"
def main():
    long = "".join(["x" for i in range(60)])
    fifty = "".join(["y" for i in range(50)])
    symbols = Array(["IBM", "", "AAPL", "βeta", long])
    trades = DataFrame({"symbol": symbols, "price": Array([3.0, 1.0, 2.0, 4.0, 5.0])})
    print(trades)
    loose = DataFrame({"n": Array([1, 2]), "tag": Array(["a", "bb"])})
    print(loose)
    print(f"{loose}!")
    alone = DataFrame({"tag": Array([fifty, "-1", " pad "])})
    print(alone)
    names: list[str] = []
    for i in range(100):
        names.append(f"row {i}")
    many = DataFrame({"name": Array(names), "flag": Array.ones(100, bool)})
    print(many)
    empty: list[str] = []
    none = DataFrame({"tag": Array(empty)})
    print(none)
"#;

/// A frame with a text column prints as pandas 3 prints it: the text without quotes and to
/// the right, its heading no wider than its name, a cell of 50 characters or more cut to 46
/// and `...`, an empty string as an empty cell, and the same row and size rules as a frame
/// of numbers. The expected text is pandas's own output for the same frames.
#[test]
fn frames_with_a_text_column_print_as_pandas_prints_them() {
    let expected = [
        "                                              symbol  price\n",
        "0                                                IBM    3.0\n",
        "1                                                       1.0\n",
        "2                                               AAPL    2.0\n",
        "3                                               βeta    4.0\n",
        "4  xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx...    5.0\n",
        "   n tag\n",
        "0  1   a\n",
        "1  2  bb\n",
        "   n tag\n",
        "0  1   a\n",
        "1  2  bb!\n",
        "                                                 tag\n",
        "0  yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy...\n",
        "1                                                 -1\n",
        "2                                               pad \n",
        "      name  flag\n",
        "0    row 0  True\n",
        "1    row 1  True\n",
        "2    row 2  True\n",
        "3    row 3  True\n",
        "4    row 4  True\n",
        "..     ...   ...\n",
        "95  row 95  True\n",
        "96  row 96  True\n",
        "97  row 97  True\n",
        "98  row 98  True\n",
        "99  row 99  True\n",
        "\n",
        "[100 rows x 2 columns]\n",
        "Empty DataFrame\n",
        "Columns: [tag]\n",
        "Index: []\n",
    ];
    let (code, stdout, report) = run("text-columns-printed", PRINTED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that chooses rows of frames with a text column: `head` and `tail` with counts
/// of both signs, slices with a step and backwards, masks that keep some rows and none, and
/// `sort_values` by a column of numbers in both directions.
const CHOSEN: &str = r#"
def main():
    symbols = Array(["IBM", "", "AAPL", "βeta", "MSFT", "GOOG", "x"])
    prices = Array([3.0, 1.0, 2.0, 4.0, 0.5, 9.0, 2.0])
    trades = DataFrame({"symbol": symbols, "price": prices})
    print(trades.head(2))
    print(trades.tail(3))
    print(trades.head(-5))
    print(trades[1:6:2])
    print(trades[::-1])
    print(trades[trades.price > 1.5])
    print(trades[trades.price > 100.0])
    by = trades.sort_values("price")
    print(by)
    print(trades.sort_values("price", ascending=False).head(3).symbol[0])
    print(len(trades), trades.symbol[6])
    alone = DataFrame({"tag": Array(["b", "a", "c"])})
    print(alone[1:])
    print(alone.tail(1).tag[0], len(alone.head(0)))
"#;

/// The rows chosen from a frame with a text column carry their text with them: every frame
/// printed is what pandas 3 prints for the same rows, numbered from 0 again, and a stable
/// sort keeps rows of equal price in their order.
#[test]
fn rows_of_a_frame_with_a_text_column_are_chosen_and_sorted() {
    let expected = [
        "  symbol  price\n",
        "0    IBM    3.0\n",
        "1           1.0\n",
        "  symbol  price\n",
        "0   MSFT    0.5\n",
        "1   GOOG    9.0\n",
        "2      x    2.0\n",
        "  symbol  price\n",
        "0    IBM    3.0\n",
        "1           1.0\n",
        "  symbol  price\n",
        "0           1.0\n",
        "1   βeta    4.0\n",
        "2   GOOG    9.0\n",
        "  symbol  price\n",
        "0      x    2.0\n",
        "1   GOOG    9.0\n",
        "2   MSFT    0.5\n",
        "3   βeta    4.0\n",
        "4   AAPL    2.0\n",
        "5           1.0\n",
        "6    IBM    3.0\n",
        "  symbol  price\n",
        "0    IBM    3.0\n",
        "1   AAPL    2.0\n",
        "2   βeta    4.0\n",
        "3   GOOG    9.0\n",
        "4      x    2.0\n",
        "Empty DataFrame\n",
        "Columns: [symbol, price]\n",
        "Index: []\n",
        "  symbol  price\n",
        "0   MSFT    0.5\n",
        "1           1.0\n",
        "2   AAPL    2.0\n",
        "3      x    2.0\n",
        "4    IBM    3.0\n",
        "5   βeta    4.0\n",
        "6   GOOG    9.0\n",
        "GOOG\n",
        "7 x\n",
        "  tag\n",
        "0   a\n",
        "1   c\n",
        "c 0\n",
    ];
    let (code, stdout, report) = run("text-columns-chosen", CHOSEN).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that chooses and changes the columns of frames with a text column: a list of
/// columns, `rename`, `drop`, and `assign` that adds a text column, replaces one and adds
/// numbers beside one. Its last line assigns a text column of another length.
const COLUMNS: &str = r#"
def main():
    symbols = Array(["IBM", "", "AAPL", "βeta"])
    prices = Array([3.0, 1.0, 2.0, 4.0])
    trades = DataFrame({"symbol": symbols, "price": prices, "n": Array([1, 2, 3, 4])})
    print(trades[["n", "symbol"]])
    print(trades[["symbol"]])
    print(trades[["price"]].price.sum())
    named = trades.copy().rename(columns={"symbol": "ticker", "n": "count"})
    print(named)
    print(named.ticker[2], named.columns)
    less = trades.copy().drop(columns=["price"])
    print(less)
    numbers = trades.copy().drop(columns=["symbol"])
    print(numbers.shape)
    tagged = numbers.assign(tag=Array(["a", "b", "c", "d"]))
    print(tagged)
    swapped = trades.copy().assign(symbol=Array(["w", "x", "y", "z"]), half=trades.price / 2.0)
    print(swapped)
    first = DataFrame({"tag": Array(["p", "q"])}).assign(n=Array([1, 2]))
    print(first)
    bad = trades.assign(other=Array(["only one"]))
"#;

/// The columns of a frame with a text column are chosen, renamed, dropped and assigned as
/// those of a frame of numbers: every frame printed is what pandas 3 prints. A new text
/// column of another length than the frame panics at its argument.
#[test]
fn columns_of_a_frame_with_a_text_column_are_chosen_and_changed() {
    let expected = [
        "   n symbol\n",
        "0  1    IBM\n",
        "1  2       \n",
        "2  3   AAPL\n",
        "3  4   βeta\n",
        "  symbol\n",
        "0    IBM\n",
        "1       \n",
        "2   AAPL\n",
        "3   βeta\n",
        "10.0\n",
        "  ticker  price  count\n",
        "0    IBM    3.0      1\n",
        "1           1.0      2\n",
        "2   AAPL    2.0      3\n",
        "3   βeta    4.0      4\n",
        "AAPL ['ticker', 'price', 'count']\n",
        "  symbol  n\n",
        "0    IBM  1\n",
        "1         2\n",
        "2   AAPL  3\n",
        "3   βeta  4\n",
        "(4, 2)\n",
        "   price  n tag\n",
        "0    3.0  1   a\n",
        "1    1.0  2   b\n",
        "2    2.0  3   c\n",
        "3    4.0  4   d\n",
        "  symbol  price  n  half\n",
        "0      w    3.0  1   1.5\n",
        "1      x    1.0  2   0.5\n",
        "2      y    2.0  3   1.0\n",
        "3      z    4.0  4   2.0\n",
        "  tag  n\n",
        "0   p  1\n",
        "1   q  2\n",
    ];
    let (code, stdout, report) = run("text-columns-changed", COLUMNS).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 4 and 1\n  --> main.byn:22:31\n"
    );
}

/// A program that stacks frames with a text column: two, four with an empty one among
/// them, one alone, two empty ones, and frames of a text column only.
const STACKED: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64


def part(names: list[str], price: f64) -> DataFrame[Trade]:
    return DataFrame({"symbol": Array(names), "price": Array.fill(len(names), price)})


def main():
    first = part(["IBM", ""], 1.0)
    second = part(["AAPL", "βeta", "MSFT"], 2.0)
    empty: list[str] = []
    none = part(empty, 0.0)
    print(concat([first, second]))
    print(concat([first, none, second, first]))
    print(concat([second]))
    print(len(concat([none, none])), len(first), first.symbol[1] == "")
    tags = concat([DataFrame({"tag": Array(["a"])}), DataFrame({"tag": Array(["b", "c"])})])
    print(tags)
"#;

/// `concat` of frames with a text column puts their text end to end as it does their
/// numbers: every frame printed is what pandas 3 prints for `concat(..., ignore_index=True)`.
#[test]
fn frames_with_a_text_column_are_stacked() {
    let expected = [
        "  symbol  price\n",
        "0    IBM    1.0\n",
        "1           1.0\n",
        "2   AAPL    2.0\n",
        "3   βeta    2.0\n",
        "4   MSFT    2.0\n",
        "  symbol  price\n",
        "0    IBM    1.0\n",
        "1           1.0\n",
        "2   AAPL    2.0\n",
        "3   βeta    2.0\n",
        "4   MSFT    2.0\n",
        "5    IBM    1.0\n",
        "6           1.0\n",
        "  symbol  price\n",
        "0   AAPL    2.0\n",
        "1   βeta    2.0\n",
        "2   MSFT    2.0\n",
        "0 2 True\n",
        "  tag\n",
        "0   a\n",
        "1   b\n",
        "2   c\n",
    ];
    let (code, stdout, report) = run("text-columns-stacked", STACKED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}
