//! Joining two frames (roadmap 3.13.8, decision 0045), through every stage up to the type
//! checker: `merge`, the type it gives, and the mistakes it reports.
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

/// The joined frame lists the left frame's columns, then the right frame's that are no keys,
/// whichever order the keys are given in; a column the two share gets `_x` and `_y`; and the
/// result is a declared frame type when the columns match.
#[test]
fn joined_frames_have_the_left_columns_then_the_right() {
    let program = r#"
class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64


class Listing(Struct):
    symbol: i64
    lot: i32
    price: f64


class Both(Struct):
    symbol: i64
    price_x: f64
    volume: i64
    lot: i32
    price_y: f64


def joined(trades: DataFrame[Trade], listings: DataFrame[Listing]) -> DataFrame[Both]:
    a: int = trades.merge(listings, on=["price", "symbol"], how="inner")
    b: int = listings.merge(trades, on="symbol")
    return trades.merge(listings, on="symbol")
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `i64`, found `DataFrame[symbol: i64, price: f64, volume: i64, lot: i32]`
  --> main.byn:22:14
   |
22 |     a: int = trades.merge(listings, on=["price", "symbol"], how="inner")
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `DataFrame[symbol: i64, lot: i32, price_x: f64, price_y: f64, volume: i64]`
  --> main.byn:23:14
   |
23 |     b: int = listings.merge(trades, on="symbol")
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`"#
    );
}

/// `merge` without the other frame or without `on=`, with an argument it does not have, with
/// something that is no frame, and with a `how` that is not the inner join or not a string
/// literal are each reported.
#[test]
fn merge_arguments_in_another_form_are_reported() {
    let program = r#"
class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64


class Listing(Struct):
    symbol: i64
    lot: i32
    price: f64


def joining(trades: DataFrame[Trade], listings: DataFrame[Listing], name: str):
    a = trades.merge(on="symbol")
    b = trades.merge(listings)
    c = trades.merge(listings, on="symbol", suffixes=("_l", "_r"))
    d = trades.merge(trades.price, on="symbol")
    e = trades.merge(listings, on="symbol", how="left")
    f = trades.merge(listings, on="symbol", how=name)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `merge` takes the other frame, then the columns to join on as `on=`
  --> main.byn:14:9
   |
14 |     a = trades.merge(on="symbol")
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `trades.merge(names, on="symbol")` or `on=["symbol", "day"]`; the key columns have one name in both frames

error: `merge` takes the other frame, then the columns to join on as `on=`
  --> main.byn:15:9
   |
15 |     b = trades.merge(listings)
   |         ^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `trades.merge(names, on="symbol")` or `on=["symbol", "day"]`; the key columns have one name in both frames

error: `merge` takes the other frame, then the columns to join on as `on=`
  --> main.byn:16:9
   |
16 |     c = trades.merge(listings, on="symbol", suffixes=("_l", "_r"))
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `trades.merge(names, on="symbol")` or `on=["symbol", "day"]`; the key columns have one name in both frames

error: `merge` joins two `DataFrame`s, found `Array[f64, 1]`
  --> main.byn:17:22
   |
17 |     d = trades.merge(trades.price, on="symbol")
   |                      ^^^^^^^^^^^^ not a `DataFrame`

error: `merge` joins only with `how="inner"` yet
  --> main.byn:18:49
   |
18 |     e = trades.merge(listings, on="symbol", how="left")
   |                                                 ^^^^^^ not an inner join
   = note: a left, right or outer join gives rows with missing values, which come with roadmap item 3.13.6

error: `how` is `"inner"`, written as a string literal
  --> main.byn:19:49
   |
19 |     f = trades.merge(listings, on="symbol", how=name)
   |                                                 ^^^^ not an inner join
   = note: the result's type depends on it, so it must be known when compiling"#
    );
}

/// A key the other frame does not have, a key that is not a string literal, no keys, a key
/// of two types, and a joined frame with one name for two columns are each reported.
#[test]
fn merge_key_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64


class Listing(Struct):
    symbol: i64
    lot: i32
    price: f64


def joining(trades: DataFrame[Trade], listings: DataFrame[Listing], name: str):
    a = trades.merge(listings, on="volume")
    b = trades.merge(listings, on=name)
    c = trades.merge(listings, on=[])
    wide = listings.copy().assign(symbol=listings.price.copy())
    d = trades.merge(wide, on="symbol")
    named = listings.copy().rename(columns={"lot": "price_x"})
    e = trades.merge(named, on="symbol")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `DataFrame[Listing]` has no column `volume`
  --> main.byn:14:35
   |
14 |     a = trades.merge(listings, on="volume")
   |                                   ^^^^^^^^ not a column
   = note: its columns are `symbol`, `lot`, `price`

error: a column's name is written as a string literal
  --> main.byn:15:35
   |
15 |     b = trades.merge(listings, on=name)
   |                                   ^^^^ not a string literal
   = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: `merge` needs at least one column to join on
  --> main.byn:16:35
   |
16 |     c = trades.merge(listings, on=[])
   |                                   ^^ no columns

error: the column `symbol` is `i64` in one frame and `f64` in the other
  --> main.byn:18:31
   |
18 |     d = trades.merge(wide, on="symbol")
   |                               ^^^^^^^^ two types for one key
   = note: a key has one type in both frames; convert one column first, with `assign`

error: the joined frame would have two columns named `price_x`
  --> main.byn:20:9
   |
20 |     e = trades.merge(named, on="symbol")
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ one name for two columns
   = note: rename one of them first, with `rename(columns={...})`"#
    );
}
