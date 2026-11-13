//! Leaving out the rows of a frame that repeat (roadmap 3.13.9, decision 0045), through
//! every stage up to the type checker: `drop_duplicates`, the type it gives, and the
//! mistakes it reports.
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
