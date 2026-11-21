//! Choosing and changing the columns of a frame (roadmap 3.13.4, decision 0045), through
//! every stage up to the type checker: `df[["a", "b"]]`, `assign`, `drop` and `rename`, the
//! type each gives, and the mistakes each reports.
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

/// Each result's type lists its columns: those picked, in the order written; those of the
/// frame with `assign`'s new ones at the end and a replaced one where it stood, with its new
/// element type; those `drop` leaves; and those under `rename`'s names, which match a
/// struct only in the struct's order.
#[test]
fn chosen_and_changed_columns_give_the_frame_its_type() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def main():
    trades: DataFrame[Trade] = DataFrame({"price": Array([1.5]), "volume": Array([2])})
    a: int = trades[["volume", "price"]]
    b: int = trades.copy().assign(notional=trades.price * trades.volume, volume=trades.price.copy())
    c: int = trades.copy().drop(columns="price")
    d: int = trades.copy().rename(columns={"price": "cost"})
    e: DataFrame[Trade] = trades.copy().rename(columns={"price": "volume", "volume": "price"})
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `i64`, found `DataFrame[volume: i64, price: f64]`
 --> main.byn:8:14
  |
8 |     a: int = trades[["volume", "price"]]
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `DataFrame[price: f64, volume: f64, notional: f64]`
 --> main.byn:9:14
  |
9 |     b: int = trades.copy().assign(notional=trades.price * trades.volume, volume=trades.price.copy())
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `DataFrame[volume: i64]`
  --> main.byn:10:14
   |
10 |     c: int = trades.copy().drop(columns="price")
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `DataFrame[cost: f64, volume: i64]`
  --> main.byn:11:14
   |
11 |     d: int = trades.copy().rename(columns={"price": "cost"})
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `DataFrame[Trade]`, found `DataFrame[volume: f64, price: i64]`
  --> main.byn:12:27
   |
12 |     e: DataFrame[Trade] = trades.copy().rename(columns={"price": "volume", "volume": "price"})
   |                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `DataFrame[Trade]`"#
    );
}

/// `df[[...]]` naming a column the frame does not have, one column twice, a name that is not
/// a string literal, and no columns at all are each reported.
#[test]
fn column_choosing_mistakes_are_reported() {
    let program = r#"
def main():
    df = DataFrame({"a": Array([1, 2]), "b": Array([3, 4])})
    name = "a"
    print(df[["a", "c"]])
    print(df[["a", "a"]])
    print(df[[name]])
    print(df[[]])
"#;
    assert_eq!(
        check_one(program),
        r#"error: `DataFrame[a: i64, b: i64]` has no column `c`
 --> main.byn:4:20
  |
4 |     print(df[["a", "c"]])
  |                    ^^^ not a column
  = note: its columns are `a`, `b`

error: the column `a` is named twice
 --> main.byn:5:20
  |
5 |     print(df[["a", "a"]])
  |                    ^^^ a second time

error: a column's name is written as a string literal
 --> main.byn:6:15
  |
6 |     print(df[[name]])
  |               ^^^^ not a string literal
  = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: a `DataFrame` needs at least one column
 --> main.byn:7:14
  |
7 |     print(df[[]])
  |              ^^ no columns"#
    );
}

/// `assign`, `drop` and `rename` each report arguments in another form than theirs, a column
/// that is not an array of rank 1, a column the frame does not have, a frame left with no
/// columns or with one name twice, a name that is not a string literal, a borrowed frame,
/// and a frame read after one of them took it. A call with a mistake takes nothing.
#[test]
fn column_changing_mistakes_are_reported() {
    let program = r#"
class Pair(Struct):
    a: i64
    b: i64


def borrowed(df: DataFrame[Pair]) -> i64:
    other = df.drop(columns=["a"])
    return other.b.sum()


def main():
    df = DataFrame({"a": Array([1, 2]), "b": Array([3, 4])})
    name = "a"
    one = df.assign(Array([1, 2]))
    two = df.assign(c=[1])
    three = df.assign(c=["x", "y"])
    four = df.drop(["a"])
    five = df.drop(columns=["a", "b"])
    six = df.drop(columns=["z"])
    seven = df.rename({"a": "x"})
    eight = df.rename(columns={"a": "b"})
    nine = df.rename(columns={"z": "b"})
    ten = df.rename(columns={"a": name})
    kept = df.drop(columns=["a"])
    print(df.a)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `df` is borrowed from the caller, so it cannot be moved
 --> main.byn:7:13
  |
7 |     other = df.drop(columns=["a"])
  |             ^^ moved here
  = note: move a copy instead: `df.copy()`, or take the parameter as `Owned[...]`

error: `assign` takes each new column by name
  --> main.byn:14:11
   |
14 |     one = df.assign(Array([1, 2]))
   |           ^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.assign(notional=df.price * df.volume)`

error: a column is an array of rank 1 or one value for every row, found `list[i64]`
  --> main.byn:15:23
   |
15 |     two = df.assign(c=[1])
   |                       ^^^ not a column
   = note: one value is a number, a `bool` or a `str`: `df.assign(fee=0.5)`

error: a column is an array of rank 1 or one value for every row, found `list[str]`
  --> main.byn:16:25
   |
16 |     three = df.assign(c=["x", "y"])
   |                         ^^^^^^^^^^ not a column
   = note: one value is a number, a `bool` or a `str`: `df.assign(fee=0.5)`

error: `drop` takes the columns to leave out as `columns=`, or first with `axis=1`
  --> main.byn:17:12
   |
17 |     four = df.drop(["a"])
   |            ^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.drop(columns=["volume"])` or `df.drop("volume", axis=1)`

error: `drop` would leave the `DataFrame` with no columns
  --> main.byn:18:28
   |
18 |     five = df.drop(columns=["a", "b"])
   |                            ^^^^^^^^^^ every column

error: `DataFrame[a: i64, b: i64]` has no column `z`
  --> main.byn:19:28
   |
19 |     six = df.drop(columns=["z"])
   |                            ^^^ not a column
   = note: its columns are `a`, `b`

error: `rename` takes the new names as `columns=`, a dict from old name to new, or first with `axis=1`
  --> main.byn:20:13
   |
20 |     seven = df.rename({"a": "x"})
   |             ^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.rename(columns={"price": "cost"})` or `df.rename({"price": "cost"}, axis=1)`

error: `rename` would give two columns the name `b`
  --> main.byn:21:31
   |
21 |     eight = df.rename(columns={"a": "b"})
   |                               ^^^^^^^^^^ one name for two columns

error: `DataFrame[a: i64, b: i64]` has no column `z`
  --> main.byn:22:31
   |
22 |     nine = df.rename(columns={"z": "b"})
   |                               ^^^ not a column
   = note: its columns are `a`, `b`

error: a column's name is written as a string literal
  --> main.byn:23:35
   |
23 |     ten = df.rename(columns={"a": name})
   |                                   ^^^^ not a string literal
   = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: `df` was moved
  --> main.byn:25:11
   |
25 |     print(df.a)
   |           ^^ read after the move
  ::: main.byn:24:12
   |
24 |     kept = df.drop(columns=["a"])
   |            -- `df` moved here
   = note: to keep both, move a copy: `df.copy()`"#
    );
}

/// One value for every row is a number, a `bool` or a `str` (roadmap 3.13.18), so a value
/// that may be `None` or a tuple is refused. `drop` and `rename` name columns only: `axis`
/// is `1` or `"columns"` written out, a missing `axis` gets the usage message, since pandas
/// would then act on row labels, and `columns=` does not go with a first argument.
#[test]
fn one_value_and_axis_mistakes_are_reported() {
    let program = r#"
def main():
    gone: f64 | None = None
    across = 1
    df = DataFrame({"symbol": Array(["a", "b"]), "price": Array([1.0, 2.0])})
    a = df.copy().assign(fee=gone)
    b = df.copy().assign(pair=(1, 2))
    c = df.copy().drop("price")
    d = df.copy().drop("price", axis=0)
    e = df.copy().drop("price", axis="index")
    f = df.copy().drop("price", axis=across)
    g = df.copy().drop("cost", axis=1)
    h = df.copy().drop("price", columns="symbol", axis=1)
    i = df.copy().rename({"price": "cost"})
    j = df.copy().rename({"price": "cost"}, axis=0)
    k = df.copy().rename("price", axis=1)
    l = df.copy().rename(mapper={"cost": "price"}, axis="columns")
"#;
    assert_eq!(
        check_one(program),
        r#"error: a column is an array of rank 1 or one value for every row, found `f64 | None`
 --> main.byn:5:30
  |
5 |     a = df.copy().assign(fee=gone)
  |                              ^^^^ not a column
  = note: one value is a number, a `bool` or a `str`: `df.assign(fee=0.5)`

error: a column is an array of rank 1 or one value for every row, found `tuple[i64, i64]`
 --> main.byn:6:31
  |
6 |     b = df.copy().assign(pair=(1, 2))
  |                               ^^^^^^ not a column
  = note: one value is a number, a `bool` or a `str`: `df.assign(fee=0.5)`

error: `drop` takes the columns to leave out as `columns=`, or first with `axis=1`
 --> main.byn:7:9
  |
7 |     c = df.copy().drop("price")
  |         ^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.drop(columns=["volume"])` or `df.drop("volume", axis=1)`

error: `axis` is `1` or `"columns"`, written out
 --> main.byn:8:38
  |
8 |     d = df.copy().drop("price", axis=0)
  |                                      ^ not the columns
  = note: a `DataFrame` has no row index, so there are no row labels to name; choose rows with a mask, `df[df.price > 0.0]`

error: `axis` is `1` or `"columns"`, written out
 --> main.byn:9:38
  |
9 |     e = df.copy().drop("price", axis="index")
  |                                      ^^^^^^^ not the columns
  = note: a `DataFrame` has no row index, so there are no row labels to name; choose rows with a mask, `df[df.price > 0.0]`

error: `axis` is `1` or `"columns"`, written out
  --> main.byn:10:38
   |
10 |     f = df.copy().drop("price", axis=across)
   |                                      ^^^^^^ not the columns
   = note: a `DataFrame` has no row index, so there are no row labels to name; choose rows with a mask, `df[df.price > 0.0]`

error: `DataFrame[symbol: str, price: f64]` has no column `cost`
  --> main.byn:11:24
   |
11 |     g = df.copy().drop("cost", axis=1)
   |                        ^^^^^^ not a column
   = note: its columns are `symbol`, `price`

error: `drop` takes the columns to leave out as `columns=`, or first with `axis=1`
  --> main.byn:12:9
   |
12 |     h = df.copy().drop("price", columns="symbol", axis=1)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.drop(columns=["volume"])` or `df.drop("volume", axis=1)`

error: `rename` takes the new names as `columns=`, a dict from old name to new, or first with `axis=1`
  --> main.byn:13:9
   |
13 |     i = df.copy().rename({"price": "cost"})
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.rename(columns={"price": "cost"})` or `df.rename({"price": "cost"}, axis=1)`

error: `axis` is `1` or `"columns"`, written out
  --> main.byn:14:50
   |
14 |     j = df.copy().rename({"price": "cost"}, axis=0)
   |                                                  ^ not the columns
   = note: a `DataFrame` has no row index, so there are no row labels to name; choose rows with a mask, `df[df.price > 0.0]`

error: `rename` takes the new names as `columns=`, a dict from old name to new, or first with `axis=1`
  --> main.byn:15:9
   |
15 |     k = df.copy().rename("price", axis=1)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.rename(columns={"price": "cost"})` or `df.rename({"price": "cost"}, axis=1)`

error: `DataFrame[symbol: str, price: f64]` has no column `cost`
  --> main.byn:16:34
   |
16 |     l = df.copy().rename(mapper={"cost": "price"}, axis="columns")
   |                                  ^^^^^^ not a column
   = note: its columns are `symbol`, `price`"#
    );
}
