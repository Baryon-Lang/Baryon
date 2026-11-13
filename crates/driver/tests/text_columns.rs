//! Text columns of a `DataFrame` (roadmap 3.13.3, decision 0045) through every stage up to
//! the type checker: a frame with a column of `str` is typed, and each use of one that is not
//! built yet is reported.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A program that uses frames with a text column in ways that are not built yet, one on each
/// line of `uses`, among them a column moved out, an element assigned, and a column of the
/// wrong type.
const PROGRAM: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64


def uses(trades: DataFrame[Trade], more: DataFrame[Trade], rows: list[Trade]):
    a = trades.head(2)
    b = trades[0]
    c = trades[trades.price > 1.0]
    d = DataFrame(rows)
    e = concat([trades, more])
    g = trades.groupby("symbol").agg(total=("price", "sum"))
    for row in trades.itertuples():
        pass
    h = trades.sort_values("price")
    kept = trades.symbol
    trades.symbol[0] = "x"
    k = trades[["price"]]
    m = trades.drop(columns=["symbol"])
    numbers = DataFrame({"n": Array([1, 2])})
    n = numbers.assign(tag=Array(["a", "b"]))
    o = DataFrame({"tag": [1, 2]})
    p = trades.symbol + more.symbol
    wrong: DataFrame[Trade] = DataFrame({"symbol": Array([1.0]), "price": Array([1.0])})
"#;

/// What the checker reports for [`PROGRAM`], in source order.
const EXPECTED: &str = r#"error: the method `head` is not built for a `DataFrame` with a column of `str` yet
 --> main.byn:7:9
  |
7 |     a = trades.head(2)
  |         ^^^^^^^^^^^^^^ has a column of `str`
  = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: choosing rows or columns with `[...]` is not built for a `DataFrame` with a column of `str` yet
 --> main.byn:8:9
  |
8 |     b = trades[0]
  |         ^^^^^^^^^ has a column of `str`
  = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: choosing rows or columns with `[...]` is not built for a `DataFrame` with a column of `str` yet
 --> main.byn:9:9
  |
9 |     c = trades[trades.price > 1.0]
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
  = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `DataFrame(rows)` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:10:19
   |
10 |     d = DataFrame(rows)
   |                   ^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `concat` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:11:9
   |
11 |     e = concat([trades, more])
   |         ^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `groupby` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:12:9
   |
12 |     g = trades.groupby("symbol").agg(total=("price", "sum"))
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: the method `itertuples` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:13:16
   |
13 |     for row in trades.itertuples():
   |                ^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: the method `sort_values` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:15:9
   |
15 |     h = trades.sort_values("price")
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: a column cannot be moved out of its `DataFrame`
  --> main.byn:16:12
   |
16 |     kept = trades.symbol
   |            ^^^^^^^^^^^^^ moved out of the frame
   = note: a column can be read, passed to a call or assigned to; to keep it on its own, copy it: `trades.symbol.copy()`

error: `trades` is a read-only parameter, so it cannot be changed
  --> main.byn:17:5
   |
17 |     trades.symbol[0] = "x"
   |     ^^^^^^^^^^^^^^^^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = trades.copy()`

error: an element of an array of `str` cannot be assigned yet
  --> main.byn:17:5
   |
17 |     trades.symbol[0] = "x"
   |     ^^^^^^^^^^^^^^^^ assigned here
   = note: the elements are stored end to end, so one cannot change length in place; build a new array from a list, `Array(words)`

error: choosing rows or columns with `[...]` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:18:9
   |
18 |     k = trades[["price"]]
   |         ^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: the method `drop` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:19:9
   |
19 |     m = trades.drop(columns=["symbol"])
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print` and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `assign` cannot add a column of `str` yet
  --> main.byn:21:28
   |
21 |     n = numbers.assign(tag=Array(["a", "b"]))
   |                            ^^^^^^^^^^^^^^^^^ an array of `str`
   = note: build the frame with the column in it: `DataFrame({"symbol": symbols, ...})`

error: a column is an array of rank 1, found `list[i64]`
  --> main.byn:22:27
   |
22 |     o = DataFrame({"tag": [1, 2]})
   |                           ^^^^^^ not a column

error: `+` is not built for an array of `str` yet
  --> main.byn:23:9
   |
23 |     p = trades.symbol + more.symbol
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, `for` loops and `copy()` today; the rest comes with roadmap item 3.13.3

error: list items must share one type, found `str` and `f64`
  --> main.byn:24:59
   |
24 |     wrong: DataFrame[Trade] = DataFrame({"symbol": Array([1.0]), "price": Array([1.0])})
   |                                                           ^^^ a different type"#;

/// Every use of a frame with a text column that is not built yet is an error that says so
/// and names what is built: methods, rows, `[...]`, `DataFrame(rows)`, `concat`, `groupby`,
/// `itertuples`, and `assign` of a text column. The column itself is a
/// text array, so moving it out, assigning an element and adding two are that type's errors.
#[test]
fn uses_of_a_text_column_frame_not_built_yet_are_reported() {
    assert_eq!(check_one(PROGRAM), EXPECTED);
}

/// A frame with a text column is built from its columns, with or without a schema, counted,
/// copied, passed, returned and held in a list, and its column is read, looped over and
/// replaced, without a diagnostic.
#[test]
fn a_frame_with_a_text_column_is_typed() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64


def cheapest(trades: DataFrame[Trade]) -> str:
    best = 0
    for i in range(len(trades)):
        if trades.price[i] < trades.price[best]:
            best = i
    return trades.symbol[best].copy()


def load(words: list[str]) -> DataFrame[Trade]:
    return DataFrame({"symbol": Array(words), "price": Array.zeros(len(words))})


def main():
    trades = load(["IBM", "AAPL"])
    rows, columns = trades.shape
    names = trades.columns
    kept = trades.copy()
    kept.symbol = Array(["A", "B"])
    for symbol in kept["symbol"]:
        print(symbol, cheapest(trades), rows, columns, names)
    loose = DataFrame({"n": Array([1, 2]), "tag": Array(["a", "b"])})
    held = [trades, kept]
    print(len(held[0]), loose.tag[1], len(loose.tag.copy()))
"#;
    assert_eq!(check_one(program), "");
}
