//! Grouping the rows of a frame (roadmap 3.13.7, decision 0045), through every stage up to
//! the type checker: `groupby(...).agg(...)`, the type it gives, and the mistakes it reports.
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

/// The result lists the key columns, then one column for each argument of `agg`: a sum and
/// a count are `i64`, except a sum of floats, which keeps their type; `min`, `max`, `first`
/// and `last` keep the column's type; `mean`, `var`, `std` and `median` are `f64`, or `f32`
/// for an `f32` column. The result is a declared frame type when the columns match.
#[test]
fn grouped_frames_have_the_keys_then_the_new_columns() {
    let program = r#"
class Trade(Struct):
    symbol: i32
    price: f64
    volume: i64
    open: bool
    fee: f32


class Summary(Struct):
    symbol: i32
    total: i64
    mean: f64


def summary(trades: DataFrame[Trade]) -> DataFrame[Summary]:
    wrong: int = trades.groupby("symbol").agg(
        a=("volume", "sum"), b=("open", "sum"), c=("fee", "sum"), d=("price", "sum"),
        e=("open", "min"), f=("volume", "last"), g=("fee", "count"),
        h=("volume", "mean"), i=("fee", "median"), j=("price", "std"),
    )
    return trades.groupby("symbol").agg(total=("volume", "sum"), mean=("price", "mean"))
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `i64`, found `DataFrame[symbol: i32, a: i64, b: i64, c: f32, d: f64, e: bool, f: i64, g: i64, h: f64, i: f32, j: f64]`
  --> main.byn:16:18
   |
16 |     wrong: int = trades.groupby("symbol").agg(
   |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`"#
    );
}

/// `groupby` with no `agg`, with arguments in another form than its own, with a column the
/// frame does not have, a name that is not a string literal, no columns, and a complex
/// column as a key are each reported.
#[test]
fn groupby_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64


def grouping(trades: DataFrame[Trade], name: str, waves: Array[complex, 1]):
    a = trades.groupby("symbol")
    b = trades.groupby().agg(n=("price", "count"))
    c = trades.groupby("symbol", "price").agg(n=("price", "count"))
    d = trades.groupby("sign").agg(n=("price", "count"))
    e = trades.groupby(name).agg(n=("price", "count"))
    f = trades.groupby([]).agg(n=("price", "count"))
    waved = DataFrame({"z": waves.copy(), "k": Array([1])})
    g = waved.groupby("z").agg(n=("k", "count"))
"#;
    assert_eq!(
        check_one(program),
        r#"error: `groupby` is followed by `.agg(...)`, which says what to work out for each group
 --> main.byn:8:9
  |
8 |     a = trades.groupby("symbol")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^ groups with nothing worked out
  = note: for example `df.groupby("symbol").agg(total=("volume", "sum"))`

error: `groupby` takes the columns whose values make the groups
 --> main.byn:9:9
  |
9 |     b = trades.groupby().agg(n=("price", "count"))
  |         ^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.groupby("symbol")` or `df.groupby(["symbol", "day"])`

error: `groupby` takes the columns whose values make the groups
  --> main.byn:10:9
   |
10 |     c = trades.groupby("symbol", "price").agg(n=("price", "count"))
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.groupby("symbol")` or `df.groupby(["symbol", "day"])`

error: `DataFrame[Trade]` has no column `sign`
  --> main.byn:11:24
   |
11 |     d = trades.groupby("sign").agg(n=("price", "count"))
   |                        ^^^^^^ not a column
   = note: its columns are `symbol`, `price`, `volume`

error: a column's name is written as a string literal
  --> main.byn:12:24
   |
12 |     e = trades.groupby(name).agg(n=("price", "count"))
   |                        ^^^^ not a string literal
   = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: `groupby` needs at least one column
  --> main.byn:13:24
   |
13 |     f = trades.groupby([]).agg(n=("price", "count"))
   |                        ^^ no columns

error: the column `z` holds `complex128`, which `groupby` cannot group by
  --> main.byn:15:23
   |
15 |     g = waved.groupby("z").agg(n=("k", "count"))
   |                       ^^^ complex numbers"#
    );
}

/// `agg` with no arguments, with one not given by name or not a pair, with a column or a
/// reduction that does not exist, a reduction that is not a string literal, a key column as
/// a value, a name used twice, and a complex column as a value are each reported.
#[test]
fn agg_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    symbol: i64
    price: f64
    volume: i64


def grouping(trades: DataFrame[Trade], name: str, waves: Array[complex, 1]):
    g = trades.groupby("symbol").agg()
    h = trades.groupby("symbol").agg("sum")
    i = trades.groupby("symbol").agg(n="count")
    j = trades.groupby("symbol").agg(n=("cost", "sum"))
    k = trades.groupby("symbol").agg(n=("price", "total"))
    l = trades.groupby("symbol").agg(n=("price", name))
    m = trades.groupby("symbol").agg(n=("symbol", "count"))
    n = trades.groupby("symbol").agg(symbol=("price", "sum"))
    o = trades.groupby("symbol").agg(n=("price", "sum"), n=("volume", "sum"))
    waved = DataFrame({"z": waves.copy(), "k": Array([1])})
    q = waved.groupby("k").agg(n=("z", "sum"))
"#;
    assert_eq!(
        check_one(program),
        r#"error: `agg` takes each new column by name, as a pair of a column and a reduction
 --> main.byn:8:9
  |
8 |     g = trades.groupby("symbol").agg()
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `.agg(total=("volume", "sum"), mean_price=("price", "mean"))`

error: `agg` takes each new column by name, as a pair of a column and a reduction
 --> main.byn:9:9
  |
9 |     h = trades.groupby("symbol").agg("sum")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `.agg(total=("volume", "sum"), mean_price=("price", "mean"))`

error: a column of `agg` is a pair: the column to read and how to reduce it
  --> main.byn:10:40
   |
10 |     i = trades.groupby("symbol").agg(n="count")
   |                                        ^^^^^^^ not a pair
   = note: for example `total=("volume", "sum")`

error: `DataFrame[Trade]` has no column `cost`
  --> main.byn:11:41
   |
11 |     j = trades.groupby("symbol").agg(n=("cost", "sum"))
   |                                         ^^^^^^ not a column
   = note: its columns are `symbol`, `price`, `volume`

error: `agg` has no reduction `total`
  --> main.byn:12:50
   |
12 |     k = trades.groupby("symbol").agg(n=("price", "total"))
   |                                                  ^^^^^^^ not a reduction
   = note: it has `sum`, `min`, `max`, `first`, `last`, `count`, `mean`, `var`, `std`, `median`

error: a reduction of `agg` is written as a string literal
  --> main.byn:13:50
   |
13 |     l = trades.groupby("symbol").agg(n=("price", name))
   |                                                  ^^^^ not a string literal
   = note: one of `sum`, `min`, `max`, `first`, `last`, `count`, `mean`, `var`, `std`, `median`

error: `symbol` is a column the rows are grouped by
  --> main.byn:14:41
   |
14 |     m = trades.groupby("symbol").agg(n=("symbol", "count"))
   |                                         ^^^^^^^^ a key, not a value
   = note: each group has one value of it, which the result already holds

error: the result would have two columns named `symbol`
  --> main.byn:15:45
   |
15 |     n = trades.groupby("symbol").agg(symbol=("price", "sum"))
   |                                             ^^^^^^^^^^^^^^^^ one name for two columns

error: the result would have two columns named `n`
  --> main.byn:16:60
   |
16 |     o = trades.groupby("symbol").agg(n=("price", "sum"), n=("volume", "sum"))
   |                                                            ^^^^^^^^^^^^^^^^^ one name for two columns

error: the column `z` holds `complex128`, which `groupby` cannot reduce
  --> main.byn:18:35
   |
18 |     q = waved.groupby("k").agg(n=("z", "sum"))
   |                                   ^^^ complex numbers"#
    );
}
