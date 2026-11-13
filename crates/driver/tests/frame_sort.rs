//! Sorting the rows of a frame (roadmap 3.13.5, decision 0045), through every stage up to the
//! type checker: `sort_values`, the type it gives, and the mistakes it reports.
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

/// `sort_values` gives a frame of the type it reads, which keeps its struct for rows, for a
/// frame declared by a struct and for one whose columns the compiler worked out; `ascending`
/// can be a `bool` known only when running, alone or in a list.
#[test]
fn a_sorted_frame_has_the_type_of_the_frame() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def sorted_trades(trades: DataFrame[Trade], rising: bool) -> DataFrame[Trade]:
    first: Trade = trades.sort_values("price")[0]
    plain = DataFrame({"a": Array([2, 1]), "b": Array([True, False])})
    wrong: int = plain.sort_values(["b", "a"], ascending=[rising, False])
    return trades.sort_values(by=["volume", "price"], ascending=rising)
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `i64`, found `DataFrame[a: i64, b: bool]`
 --> main.byn:9:18
  |
9 |     wrong: int = plain.sort_values(["b", "a"], ascending=[rising, False])
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`"#
    );
}

/// `sort_values` reports arguments in another form than its own, such as pandas's
/// `na_position=`; a column the frame does not have, a name that is not a string literal, no
/// columns and one column twice; an `ascending` that is not a `bool` or lists another number
/// of directions than there are columns; and a complex column, which has no order.
#[test]
fn sorting_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def sorting(trades: DataFrame[Trade], name: str, waves: Array[complex, 1]):
    a = trades.sort_values()
    b = trades.sort_values("price", "volume")
    c = trades.sort_values("price", na_position="first")
    d = trades.sort_values("cost")
    e = trades.sort_values(name)
    f = trades.sort_values([])
    g = trades.sort_values(["price", "price"])
    h = trades.sort_values("price", ascending=1)
    i = trades.sort_values(["price", "volume"], ascending=[True])
    j = trades.sort_values("price", ascending=[0.5])
    k = DataFrame({"z": waves.copy()}).sort_values("z")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `sort_values` takes the columns to sort by, then `ascending=`
 --> main.byn:7:9
  |
7 |     a = trades.sort_values()
  |         ^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.sort_values("price")` or `df.sort_values(["symbol", "price"], ascending=[True, False])`; the sort is always stable and missing values always come last

error: `sort_values` takes the columns to sort by, then `ascending=`
 --> main.byn:8:9
  |
8 |     b = trades.sort_values("price", "volume")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.sort_values("price")` or `df.sort_values(["symbol", "price"], ascending=[True, False])`; the sort is always stable and missing values always come last

error: `sort_values` takes the columns to sort by, then `ascending=`
 --> main.byn:9:9
  |
9 |     c = trades.sort_values("price", na_position="first")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.sort_values("price")` or `df.sort_values(["symbol", "price"], ascending=[True, False])`; the sort is always stable and missing values always come last

error: `DataFrame[Trade]` has no column `cost`
  --> main.byn:10:28
   |
10 |     d = trades.sort_values("cost")
   |                            ^^^^^^ not a column
   = note: its columns are `price`, `volume`

error: a column's name is written as a string literal
  --> main.byn:11:28
   |
11 |     e = trades.sort_values(name)
   |                            ^^^^ not a string literal
   = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: `sort_values` needs at least one column to sort by
  --> main.byn:12:28
   |
12 |     f = trades.sort_values([])
   |                            ^^ no columns

error: the column `price` is named twice
  --> main.byn:13:38
   |
13 |     g = trades.sort_values(["price", "price"])
   |                                      ^^^^^^^ a second time

error: `ascending` is `True` or `False`, found `i64`
  --> main.byn:14:47
   |
14 |     h = trades.sort_values("price", ascending=1)
   |                                               ^ not a `bool`

error: `ascending` lists 1 direction for 2 columns
  --> main.byn:15:59
   |
15 |     i = trades.sort_values(["price", "volume"], ascending=[True])
   |                                                           ^^^^^^ one for each column
   = note: give one `bool` for every column, or one for each column sorted by

error: `ascending` is `True` or `False`, found `f64`
  --> main.byn:16:48
   |
16 |     j = trades.sort_values("price", ascending=[0.5])
   |                                                ^^^ not a `bool`

error: the column `z` holds `complex128`, which has no order to sort by
  --> main.byn:17:52
   |
17 |     k = DataFrame({"z": waves.copy()}).sort_values("z")
   |                                                    ^^^ complex numbers
   = note: sort by a column of real numbers worked out from it, such as each value's size"#
    );
}
