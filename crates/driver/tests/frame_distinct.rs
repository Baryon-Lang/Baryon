//! Leaving out the rows of a frame that repeat, and counting them (roadmap 3.13.9, decision
//! 0045), through every stage up to the type checker: `drop_duplicates` and `value_counts`,
//! the types they give, and the mistakes they report.
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

/// `drop_duplicates` gives a frame of the type it reads. It reports arguments in another
/// form than its own, such as pandas's `keep=`; a column the frame does not have, a name
/// that is not a string literal, no columns and one column twice; and a complex column
/// among those compared, which a `subset` of other columns leaves out.
#[test]
fn drop_duplicates_keeps_the_type_and_reports_mistakes() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


class Wave(Struct):
    phase: complex
    size: f64


def main():
    trades: DataFrame[Trade] = DataFrame({"price": Array([1.0]), "volume": Array([2])})
    waves: DataFrame[Wave] = DataFrame({"phase": Array([1j]), "size": Array([1.0])})
    name = "price"
    a = trades.drop_duplicates(keep="last")
    b = trades.drop_duplicates(subset="cost")
    c = trades.drop_duplicates(subset=name)
    d = trades.drop_duplicates(subset=[])
    e = trades.drop_duplicates(["price", "price"])
    f = waves.drop_duplicates()
    g = waves.drop_duplicates(subset="phase")
    h = waves.drop_duplicates(subset="size")
    wrong: int = trades.drop_duplicates()
"#;
    assert_eq!(
        check_one(program),
        r#"error: `drop_duplicates` takes `subset=`, the columns to compare, or nothing
  --> main.byn:15:9
   |
15 |     a = trades.drop_duplicates(keep="last")
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.drop_duplicates()` or `df.drop_duplicates(subset=["symbol", "day"])`; the first of the rows that repeat is the one kept

error: `DataFrame[Trade]` has no column `cost`
  --> main.byn:16:39
   |
16 |     b = trades.drop_duplicates(subset="cost")
   |                                       ^^^^^^ not a column
   = note: its columns are `price`, `volume`

error: a column's name is written as a string literal
  --> main.byn:17:39
   |
17 |     c = trades.drop_duplicates(subset=name)
   |                                       ^^^^ not a string literal
   = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: `subset` needs at least one column to compare
  --> main.byn:18:39
   |
18 |     d = trades.drop_duplicates(subset=[])
   |                                       ^^ no columns
   = note: leave `subset` out to compare every column

error: the column `price` is named twice
  --> main.byn:19:42
   |
19 |     e = trades.drop_duplicates(["price", "price"])
   |                                          ^^^^^^^ a second time

error: `drop_duplicates` cannot compare the column `phase`, which holds `complex128`, yet
  --> main.byn:20:9
   |
20 |     f = waves.drop_duplicates()
   |         ^^^^^^^^^^^^^^^^^^^^^^^ complex numbers
   = note: name the columns of real numbers to compare with `subset=`

error: `drop_duplicates` cannot compare the column `phase`, which holds `complex128`, yet
  --> main.byn:21:38
   |
21 |     g = waves.drop_duplicates(subset="phase")
   |                                      ^^^^^^^ complex numbers
   = note: name the columns of real numbers to compare with `subset=`

error: expected `i64`, found `DataFrame[Trade]`
  --> main.byn:23:18
   |
23 |     wrong: int = trades.drop_duplicates()
   |                  ^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`"#
    );
}

/// `value_counts` gives a frame of the columns counted by and `count`, an `i64`, which an
/// annotation of that schema accepts and whose columns are read by name. It reports
/// arguments in another form than its own, such as pandas's `sort=`; a column the frame
/// does not have; no columns; a column named `count` among those counted by, named or not;
/// a complex column; and the result where another type is expected.
#[test]
fn value_counts_gives_the_columns_counted_and_reports_mistakes() {
    let diagnostics = check_one(
        r#"
class Trade(Struct):
    symbol: str
    count: i64


class Counted(Struct):
    symbol: str
    count: i64


class Wave(Struct):
    phase: complex
    size: f64


def main():
    trades: DataFrame[Trade] = DataFrame({"symbol": Array(["a"]), "count": Array([2])})
    waves: DataFrame[Wave] = DataFrame({"phase": Array([1j]), "size": Array([1.0])})
    kept: DataFrame[Counted] = trades.value_counts("symbol")
    sizes = waves.value_counts("size")
    total: i64 = sizes.count.sum() + sizes.size.nunique()
    a = trades.value_counts("symbol", sort=False)
    b = trades.value_counts("cost")
    c = trades.value_counts(subset=[])
    d = trades.value_counts("count")
    e = trades.value_counts()
    f = waves.value_counts("phase")
    wrong: int = waves.value_counts("size")
"#,
    );
    assert_eq!(
        diagnostics,
        r#"error: `value_counts` takes `subset=`, the columns to compare, or nothing
  --> main.byn:22:9
   |
22 |     a = trades.value_counts("symbol", sort=False)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.value_counts("symbol")` or `df.value_counts(["symbol", "day"])`

error: `DataFrame[Trade]` has no column `cost`
  --> main.byn:23:29
   |
23 |     b = trades.value_counts("cost")
   |                             ^^^^^^ not a column
   = note: its columns are `symbol`, `count`

error: `subset` needs at least one column to compare
  --> main.byn:24:36
   |
24 |     c = trades.value_counts(subset=[])
   |                                    ^^ no columns
   = note: leave `subset` out to compare every column

error: `value_counts` cannot count by a column named `count`
  --> main.byn:25:29
   |
25 |     d = trades.value_counts("count")
   |                             ^^^^^^^ the result would have two columns named `count`
   = note: rename it first: `df.rename(columns={"count": "n"})`

error: `value_counts` cannot count by a column named `count`
  --> main.byn:26:9
   |
26 |     e = trades.value_counts()
   |         ^^^^^^^^^^^^^^^^^^^^^ the result would have two columns named `count`
   = note: rename it first: `df.rename(columns={"count": "n"})`

error: `value_counts` cannot compare the column `phase`, which holds `complex128`, yet
  --> main.byn:27:28
   |
27 |     f = waves.value_counts("phase")
   |                            ^^^^^^^ complex numbers
   = note: name the columns of real numbers to compare with `subset=`

error: expected `i64`, found `DataFrame[size: f64, count: i64]`
  --> main.byn:28:18
   |
28 |     wrong: int = waves.value_counts("size")
   |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`"#
    );
}
