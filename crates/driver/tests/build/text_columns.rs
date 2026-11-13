//! Text columns of a `DataFrame` (roadmap 3.13.3, decision 0045), built and run: a frame with
//! a column of `str` made from its columns, counted, copied, moved, passed and returned, and
//! its column read, looped over and replaced.

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
