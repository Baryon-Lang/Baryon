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
    i = trades.sort_values(["price", "symbol"])
    kept = trades.symbol
    j = trades[1:]
    k = trades[["price"]]
    m = trades.drop(columns=["symbol"])
    numbers = DataFrame({"n": Array([1, 2])})
    n = numbers.assign(tag=Array(["a", "b"]))
    o = DataFrame({"tag": [1, 2]})
    p = trades.symbol + more.symbol
    q = trades["x"]
    r = trades[1.5]
    wrong: DataFrame[Trade] = DataFrame({"symbol": Array([1.0]), "price": Array([1.0])})
"#;

/// What the checker reports for [`PROGRAM`], in source order.
const EXPECTED: &str = r#"error: reading a row, `df[i]`, is not built for a `DataFrame` with a column of `str` yet
 --> main.byn:8:16
  |
8 |     b = trades[0]
  |                ^ has a column of `str`
  = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `DataFrame(rows)` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:10:19
   |
10 |     d = DataFrame(rows)
   |                   ^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `concat` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:11:9
   |
11 |     e = concat([trades, more])
   |         ^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `groupby` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:12:9
   |
12 |     g = trades.groupby("symbol").agg(total=("price", "sum"))
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: the method `itertuples` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:13:16
   |
13 |     for row in trades.itertuples():
   |                ^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: sorting by `symbol`, a column of `str`, is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:16:28
   |
16 |     i = trades.sort_values(["price", "symbol"])
   |                            ^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: a column cannot be moved out of its `DataFrame`
  --> main.byn:17:12
   |
17 |     kept = trades.symbol
   |            ^^^^^^^^^^^^^ moved out of the frame
   = note: a column can be read, passed to a call or assigned to; to keep it on its own, copy it: `trades.symbol.copy()`

error: choosing columns with `[[...]]` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:19:9
   |
19 |     k = trades[["price"]]
   |         ^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: the method `drop` is not built for a `DataFrame` with a column of `str` yet
  --> main.byn:20:9
   |
20 |     m = trades.drop(columns=["symbol"])
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ has a column of `str`
   = note: a frame with a column of `str` is built from its columns, `DataFrame({...})`, and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, slices, masks, `sort_values` by its other columns, and its columns, such as `df.symbol`, today; the rest comes with roadmap item 3.13.3

error: `assign` cannot add a column of `str` yet
  --> main.byn:22:28
   |
22 |     n = numbers.assign(tag=Array(["a", "b"]))
   |                            ^^^^^^^^^^^^^^^^^ an array of `str`
   = note: build the frame with the column in it: `DataFrame({"symbol": symbols, ...})`

error: a column is an array of rank 1, found `list[i64]`
  --> main.byn:23:27
   |
23 |     o = DataFrame({"tag": [1, 2]})
   |                           ^^^^^^ not a column

error: `+` is not built for an array of `str` yet
  --> main.byn:24:9
   |
24 |     p = trades.symbol + more.symbol
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, `for` loops and `copy()` today; the rest comes with roadmap item 3.13.3

error: `DataFrame[Trade]` has no column `x`
  --> main.byn:25:16
   |
25 |     q = trades["x"]
   |                ^^^ not a column
   = note: its columns are `symbol`, `price`

error: a `DataFrame` is indexed by a column's name, a row's number, a mask or a slice, found `f64`
  --> main.byn:26:16
   |
26 |     r = trades[1.5]
   |                ^^^ does not choose a column or rows
   = note: write a column's name as a string literal, `df["price"]`, a row's number, `df[0]`, a mask, an `Array[bool, 1]` such as `df[df.price > 100.0]`, or a slice, `df[1:3]`

error: list items must share one type, found `str` and `f64`
  --> main.byn:27:59
   |
27 |     wrong: DataFrame[Trade] = DataFrame({"symbol": Array([1.0]), "price": Array([1.0])})
   |                                                           ^^^ a different type"#;

/// Every use of a frame with a text column that is not built yet is an error that says so
/// and names what is built: a row, a list of columns, `DataFrame(rows)`, `concat`, `groupby`,
/// `itertuples`, `drop`, a sort by the text column, and `assign` of a text column. `head`,
/// a mask, a slice and a sort by a column of numbers are built, and get no error. The column itself is a
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
