//! Frame columns that may hold `None` (roadmap 3.13.6, decision 0045), built and run: a
//! frame made from its columns, counted, passed, returned and copied, its columns read as
//! arrays with missing values and replaced.

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

/// A frame whose schema has fields that may be `None` is built from arrays with missing
/// values, the schema giving a list written with `None` its type. `len`, `shape` and
/// `columns` count it; a column reads as an array with missing values, by attribute and by
/// name, with `isna`, `notna` and elements that are the value or `None`. A function takes
/// the frame and returns one. A copy has its own columns: replacing them leaves the first
/// frame as it was.
#[test]
fn a_frame_holds_columns_with_missing_values() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    price: f64
    bid: f64 | None
    size: i64 | None


def gaps(quotes: DataFrame[Quote]) -> int:
    count = 0
    for at in range(len(quotes)):
        if quotes.bid[at] is None:
            count += 1
    return count


def load() -> DataFrame[Quote]:
    bids: Array[f64 | None, 1] = Array([1.0, None, 3.0])
    sizes: Array[i64 | None, 1] = Array([None, 2, 3])
    return DataFrame({"price": Array([1.5, 2.5, 3.5]), "bid": bids, "size": sizes})


def main():
    quotes = load()
    print(len(quotes), quotes.shape, quotes.columns, gaps(quotes))
    print(quotes.bid.isna(), quotes["size"].notna(), quotes.size[0], quotes.size[2])
    twin = quotes.copy()
    twin.bid = Array([None, None, 9.0])
    twin["size"] = Array([7, 8, None])
    print(twin.bid[2], twin.bid[0], twin.size[0], quotes.bid[0], gaps(twin))
    mixed = DataFrame({"flag": Array([True, False]), "seen": quotes.bid.isna()[0:2].copy()})
    print(len(mixed))
    worked: DataFrame[Quote] = DataFrame(
        {"price": Array([1.0]), "bid": Array([None]), "size": Array([4])}
    )
    print(worked.bid[0], worked.size[0])
"#;
    let (code, output, _) = run("frame_missing", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "3 (3, 3) ['price', 'bid', 'size'] 1\n\
         [False  True False] [False  True  True] None 3\n\
         9.0 None 7 1.0 2\n\
         2\n\
         None 4\n"
    );
    Ok(())
}

/// A column that may hold `None` of another length than the frame's first column panics when
/// the frame is built, as a column of numbers does.
#[test]
fn a_column_with_missing_values_of_another_length_panics() -> io::Result<()> {
    let program = r#"
def main():
    bids: Array[f64 | None, 1] = Array([1.0, None])
    quotes = DataFrame({"price": Array([1.5, 2.5, 3.5]), "bid": bids})
    print(len(quotes))
"#;
    let (code, output, report) = run("frame_missing_lengths", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 3 and 2"),
        "{report}"
    );
    Ok(())
}

/// `dropna()` of a frame gives the rows with no `None` in any column, its text and number
/// columns chosen with the others, and `fillna` every row with the value in place of each
/// `None`: one value for every column that may hold `None`, or a dict that fills the columns
/// it names and leaves the rest. The columns filled hold no `None` afterwards, so they sum
/// and sort as numbers and the frame prints. A list of names chooses columns of a frame with
/// missing values. A frame with no column that may hold `None` is copied by both.
#[test]
fn a_frame_drops_and_fills_missing_values() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    symbol: str
    bid: f64 | None
    size: i64 | None
    live: bool | None
    lot: i64


class Trade(Struct):
    symbol: str
    lot: i64


def load() -> DataFrame[Quote]:
    return DataFrame(
        {
            "symbol": Array(["a", "b", "c", "d"]),
            "bid": Array([1.5, None, 2.5, None]),
            "size": Array([10, 20, None, None]),
            "live": Array([True, None, False, None]),
            "lot": Array([1, 2, 3, 4]),
        }
    )


def spread(quotes: DataFrame[Quote]) -> f64:
    whole = quotes.dropna()
    return whole.bid.max() - whole.bid.min()


def main():
    quotes = load()
    whole = quotes.dropna()
    print(whole)
    print(len(whole), len(quotes), spread(quotes), whole.bid.sum())
    print(quotes[["bid", "lot"]].fillna(0.0))
    part = quotes.fillna({"bid": 9.5, "size": 7})
    print(len(part), part.bid, part["size"], part.live.isna())
    full = part.fillna(value=True)
    print(full)
    print(full.sort_values("bid").head(2))
    print(quotes.fillna({"size": 0, "live": False}).dropna())
    trades: DataFrame[Trade] = DataFrame({"symbol": Array(["x", "y"]), "lot": Array([5, 6])})
    print(trades.dropna())
    print(trades.fillna(0).lot, len(quotes[["bid"]].dropna()))
"#;
    let (code, output, _) = run("frame_missing_narrowed", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "  symbol  bid  size  live  lot\n\
         0      a  1.5    10  True    1\n\
         1 4 0.0 1.5\n\
         \x20  bid  lot\n\
         0  1.5    1\n\
         1  0.0    2\n\
         2  2.5    3\n\
         3  0.0    4\n\
         4 [1.5 9.5 2.5 9.5] [10 20  7  7] [False  True False  True]\n\
         \x20 symbol  bid  size   live  lot\n\
         0      a  1.5    10   True    1\n\
         1      b  9.5    20   True    2\n\
         2      c  2.5     7  False    3\n\
         3      d  9.5     7   True    4\n\
         \x20 symbol  bid  size   live  lot\n\
         0      a  1.5    10   True    1\n\
         1      c  2.5     7  False    3\n\
         \x20 symbol  bid  size   live  lot\n\
         0      a  1.5    10   True    1\n\
         1      c  2.5     0  False    3\n\
         \x20 symbol  lot\n\
         0      x    5\n\
         1      y    6\n\
         [5 6] 2\n"
    );
    Ok(())
}

/// The rows of a frame with columns that may hold `None` are chosen as any frame's are, by
/// `head`, `tail`, a slice with a step or reversed, a mask, and `sort_values` on its other
/// columns, and each `None` stays with its row: the counts of the values that are there
/// and the frame with its gaps filled show which rows came through, across more than one
/// byte of validity bits. A choice of no rows gives an empty frame.
#[test]
fn rows_with_missing_values_are_chosen_and_sorted() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    symbol: str
    lot: i64
    bid: f64 | None
    size: i64 | None
    live: bool | None


def load() -> DataFrame[Quote]:
    return DataFrame(
        {
            "symbol": Array(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]),
            "lot": Array([5, 3, 9, 1, 7, 2, 8, 4, 6, 0]),
            "bid": Array([1.5, None, 2.5, None, None, 6.0, 7.0, 8.0, 9.0, None]),
            "size": Array([None, 20, 30, None, 50, 60, None, 80, 90, 100]),
            "live": Array([True, None, False, None, True, None, False, None, True, None]),
        }
    )


def show(quotes: DataFrame[Quote]):
    filled = quotes.fillna({"bid": -1.0, "size": -1, "live": False})
    print(len(quotes), quotes.bid.count(), quotes.size.count(), quotes.live.count())
    print(filled)


def main():
    quotes = load()
    show(quotes.head(3))
    show(quotes.tail(2))
    show(quotes[2:9:3])
    show(quotes[::-1].head(4))
    show(quotes[quotes.lot > 5])
    show(quotes[quotes.bid.notna() & quotes.size.isna()])
    show(quotes.sort_values("lot"))
    show(quotes.sort_values(["symbol"], ascending=False).tail(3))
    show(quotes[quotes.lot > 100])
    print(quotes.head(2).bid[1], quotes.sort_values("lot").size[0], quotes[1:2].dropna())
"#;
    let (code, output, _) = run("frame_missing_rows", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "3 2 2 2\n\
         \x20 symbol  lot  bid  size   live\n\
         0      a    5  1.5    -1   True\n\
         1      b    3 -1.0    20  False\n\
         2      c    9  2.5    30  False\n\
         2 1 2 1\n\
         \x20 symbol  lot  bid  size   live\n\
         0      i    6  9.0    90   True\n\
         1      j    0 -1.0   100  False\n\
         3 3 3 2\n\
         \x20 symbol  lot  bid  size   live\n\
         0      c    9  2.5    30  False\n\
         1      f    2  6.0    60  False\n\
         2      i    6  9.0    90   True\n\
         4 3 3 2\n\
         \x20 symbol  lot  bid  size   live\n\
         0      j    0 -1.0   100  False\n\
         1      i    6  9.0    90   True\n\
         2      h    4  8.0    80  False\n\
         3      g    8  7.0    -1  False\n\
         4 3 3 4\n\
         \x20 symbol  lot  bid  size   live\n\
         0      c    9  2.5    30  False\n\
         1      e    7 -1.0    50   True\n\
         2      g    8  7.0    -1  False\n\
         3      i    6  9.0    90   True\n\
         2 2 0 2\n\
         \x20 symbol  lot  bid  size   live\n\
         0      a    5  1.5    -1   True\n\
         1      g    8  7.0    -1  False\n\
         10 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      j    0 -1.0   100  False\n\
         1      d    1 -1.0    -1  False\n\
         2      f    2  6.0    60  False\n\
         3      b    3 -1.0    20  False\n\
         4      h    4  8.0    80  False\n\
         5      a    5  1.5    -1   True\n\
         6      i    6  9.0    90   True\n\
         7      e    7 -1.0    50   True\n\
         8      g    8  7.0    -1  False\n\
         9      c    9  2.5    30  False\n\
         3 2 2 2\n\
         \x20 symbol  lot  bid  size   live\n\
         0      c    9  2.5    30  False\n\
         1      b    3 -1.0    20  False\n\
         2      a    5  1.5    -1   True\n\
         0 0 0 0\n\
         Empty DataFrame\n\
         Columns: [symbol, lot, bid, size, live]\n\
         Index: []\n\
         None 100 Empty DataFrame\n\
         Columns: [symbol, lot, bid, size, live]\n\
         Index: []\n"
    );
    Ok(())
}

/// `sort_values` by a column that may hold `None` sorts the rows that have a value, rising
/// or falling, a NaN after the numbers, and puts the rows that hold `None` last in both
/// directions, in the order they had. With several keys the sort stays stable: rows equal
/// in the first key are ordered by the next, their `None`s last. A `bool` column and an
/// integer column sort the same way, and a frame of only `None`s keeps its order.
#[test]
fn sorting_by_a_column_with_missing_values_puts_none_last() -> io::Result<()> {
    let program = r#"
from math import nan


class Quote(Struct):
    symbol: str
    lot: i64
    bid: f64 | None
    size: i64 | None
    live: bool | None


def load() -> DataFrame[Quote]:
    return DataFrame(
        {
            "symbol": Array(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]),
            "lot": Array([1, 1, 2, 2, 1, 2, 1, 2, 1, 2]),
            "bid": Array([6.0, None, 2.5, None, None, 6.0, nan, 8.0, 1.0, None]),
            "size": Array([None, 20, 30, None, 20, 60, None, 80, 20, 100]),
            "live": Array([True, None, False, None, True, None, False, None, True, None]),
        }
    )


def show(quotes: DataFrame[Quote]):
    print(quotes.symbol, quotes.bid.count(), quotes.size.count(), quotes.live.count())
    print(quotes.fillna({"bid": -1.0, "size": -1, "live": False}).head(4))


def main():
    quotes = load()
    show(quotes.sort_values("bid"))
    show(quotes.sort_values("bid", ascending=False))
    show(quotes.sort_values(["size", "bid"], ascending=[False, True]))
    show(quotes.sort_values(["lot", "size"]))
    show(quotes.sort_values("live"))
    show(quotes.sort_values(by=["live", "symbol"], ascending=False))
    show(quotes[quotes.lot > 5].sort_values("bid"))
    show(quotes[quotes.bid.isna()].sort_values("bid"))
"#;
    let (code, output, _) = run("frame_missing_sorted", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "['i' 'c' 'a' 'f' 'h' 'g' 'b' 'd' 'e' 'j'] 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      i    1  1.0    20   True\n\
         1      c    2  2.5    30  False\n\
         2      a    1  6.0    -1   True\n\
         3      f    2  6.0    60  False\n\
         ['h' 'a' 'f' 'c' 'i' 'g' 'b' 'd' 'e' 'j'] 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      h    2  8.0    80  False\n\
         1      a    1  6.0    -1   True\n\
         2      f    2  6.0    60  False\n\
         3      c    2  2.5    30  False\n\
         ['j' 'h' 'f' 'c' 'i' 'b' 'e' 'a' 'g' 'd'] 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      j    2 -1.0   100  False\n\
         1      h    2  8.0    80  False\n\
         2      f    2  6.0    60  False\n\
         3      c    2  2.5    30  False\n\
         ['b' 'e' 'i' 'a' 'g' 'c' 'f' 'h' 'j' 'd'] 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      b    1 -1.0    20  False\n\
         1      e    1 -1.0    20   True\n\
         2      i    1  1.0    20   True\n\
         3      a    1  6.0    -1   True\n\
         ['c' 'g' 'a' 'e' 'i' 'b' 'd' 'f' 'h' 'j'] 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      c    2  2.5    30  False\n\
         1      g    1  NaN    -1  False\n\
         2      a    1  6.0    -1   True\n\
         3      e    1 -1.0    20   True\n\
         ['i' 'e' 'a' 'g' 'c' 'j' 'h' 'f' 'd' 'b'] 6 7 5\n\
         \x20 symbol  lot  bid  size   live\n\
         0      i    1  1.0    20   True\n\
         1      e    1 -1.0    20   True\n\
         2      a    1  6.0    -1   True\n\
         3      g    1  NaN    -1  False\n\
         [] 0 0 0\n\
         Empty DataFrame\n\
         Columns: [symbol, lot, bid, size, live]\n\
         Index: []\n\
         ['b' 'd' 'e' 'j'] 0 3 1\n\
         \x20 symbol  lot  bid  size   live\n\
         0      b    1 -1.0    20  False\n\
         1      d    2 -1.0    -1  False\n\
         2      e    1 -1.0    20   True\n\
         3      j    2 -1.0   100  False\n"
    );
    Ok(())
}

/// `concat` stacks frames whose columns may hold `None`, two and three at a time, one alone
/// and empty ones, and each `None` stays with its row across the byte boundaries of the
/// validity bits. `assign` adds columns that may hold `None` and columns that do not,
/// narrows one it replaces with its filled values, and takes a list written with `None`
/// for a column that may hold it; `drop` and `rename` carry such columns along.
#[test]
fn frames_with_missing_values_are_stacked_and_reshaped() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    symbol: str
    bid: f64 | None
    size: i64 | None
    live: bool | None


def load(symbols: Array[str, 1], shift: f64) -> DataFrame[Quote]:
    return DataFrame(
        {
            "symbol": symbols.copy(),
            "bid": Array([1.5 + shift, None, 2.5 + shift, None, None]),
            "size": Array([None, 20, 30, None, 50]),
            "live": Array([True, None, False, None, True]),
        }
    )


def show(quotes: DataFrame[Quote]):
    print(len(quotes), quotes.symbol, quotes.bid.count(), quotes.size.count(), quotes.live.count())
    print(quotes.fillna({"bid": -1.0, "size": -1, "live": False}))


def main():
    first = load(Array(["a", "b", "c", "d", "e"]), 0.0)
    second = load(Array(["f", "g", "h", "i", "j"]), 10.0)
    show(concat([first, second]))
    show(concat([first[0:2], second, first[4:]]))
    show(concat([first.head(1)]))
    show(concat([first[0:0], second[0:0]]))
    wider = first.copy().assign(
        mid=first.bid.fillna(0.0), ask=first.bid.copy(), lot=first.size.isna()
    )
    print(wider.columns, wider.ask.count(), wider.mid.sum(), wider.lot)
    filled = first.copy().assign(bid=first.bid.fillna(0.0))
    print(filled.bid.sum(), filled.size.count())
    gaps: Array[f64 | None, 1] = Array([None, None, None, None, 9.0])
    print(first.copy().assign(bid=gaps).bid.count(), first.copy().drop(columns=["bid"]).columns)
    print(first.copy().assign(bid=Array([None, 1.0, None, 2.0, 3.0])).bid.sum())
    print(first.copy().drop(columns=["symbol", "live"]).size.sum())
    renamed = first.rename(columns={"bid": "best", "size": "count"})
    print(renamed.columns, renamed.best.count(), renamed["count"].max())
"#;
    let (code, output, _) = run("frame_missing_stacked", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "10 ['a' 'b' 'c' 'd' 'e' 'f' 'g' 'h' 'i' 'j'] 4 6 6\n\
         \x20 symbol   bid  size   live\n\
         0      a   1.5    -1   True\n\
         1      b  -1.0    20  False\n\
         2      c   2.5    30  False\n\
         3      d  -1.0    -1  False\n\
         4      e  -1.0    50   True\n\
         5      f  11.5    -1   True\n\
         6      g  -1.0    20  False\n\
         7      h  12.5    30  False\n\
         8      i  -1.0    -1  False\n\
         9      j  -1.0    50   True\n\
         8 ['a' 'b' 'f' 'g' 'h' 'i' 'j' 'e'] 3 5 5\n\
         \x20 symbol   bid  size   live\n\
         0      a   1.5    -1   True\n\
         1      b  -1.0    20  False\n\
         2      f  11.5    -1   True\n\
         3      g  -1.0    20  False\n\
         4      h  12.5    30  False\n\
         5      i  -1.0    -1  False\n\
         6      j  -1.0    50   True\n\
         7      e  -1.0    50   True\n\
         1 ['a'] 1 0 1\n\
         \x20 symbol  bid  size  live\n\
         0      a  1.5    -1  True\n\
         0 [] 0 0 0\n\
         Empty DataFrame\n\
         Columns: [symbol, bid, size, live]\n\
         Index: []\n\
         ['symbol', 'bid', 'size', 'live', 'mid', 'ask', 'lot'] 2 4.0 [ True False False  True False]\n\
         4.0 3\n\
         1 ['symbol', 'size', 'live']\n\
         6.0\n\
         100\n\
         ['symbol', 'best', 'count', 'live'] 2 50\n"
    );
    Ok(())
}
