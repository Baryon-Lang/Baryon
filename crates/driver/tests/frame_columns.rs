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
    two = df.assign(c=1.0)
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

error: a column is an array of rank 1, found `f64`
  --> main.byn:15:23
   |
15 |     two = df.assign(c=1.0)
   |                       ^^^ not a column
   = note: to give every row one value, build the array: `Array.fill(len(df), value)`

error: a column is an array of rank 1, found `list[str]`
  --> main.byn:16:25
   |
16 |     three = df.assign(c=["x", "y"])
   |                         ^^^^^^^^^^ not a column
   = note: to give every row one value, build the array: `Array.fill(len(df), value)`

error: `drop` takes the columns to leave out as `columns=`
  --> main.byn:17:12
   |
17 |     four = df.drop(["a"])
   |            ^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.drop(columns=["volume"])`

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

error: `rename` takes the new names as `columns=`, a dict from old name to new
  --> main.byn:20:13
   |
20 |     seven = df.rename({"a": "x"})
   |             ^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.rename(columns={"price": "cost"})`

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
