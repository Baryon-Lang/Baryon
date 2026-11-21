//! The reductions of a grouping written as methods (roadmap 3.13.22, decision 0045's
//! amendment of 2026-11-19), through every stage up to the type checker: the type each
//! gives, and the mistakes it reports.
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

/// The program of `reductions_written_as_methods_are_typed_and_checked`.
const PROGRAM: &str = r#"
class Summary(Struct):
    k: i64
    v: f64


def main() -> None:
    df = DataFrame({"k": Array([2, 1]), "s": Array(["b", "a"]), "v": Array([1.5, 2.5])})
    kept: DataFrame[Summary] = df.groupby("k")["v"].mean()
    a: int = df.groupby("k").max()
    b: int = df.groupby(["k", "s"]).size()
    c: int = df.groupby("s").median(numeric_only=True)
    d = df.groupby("k").sum()
    e = df.groupby("k")["k"].sum()
    f = df.groupby("k").w.sum()
    g = df.groupby("k")["v"].sum(1)
    h = df.groupby("k")["s"].mean()
    i = df.groupby("k")["w"].max()
    j = df.groupby("k").sum(numeric_only=df)
    k = df.groupby("k").size(numeric_only=True)
    l = df.groupby("k")["s"].min(numeric_only=True)
    m = DataFrame({"size": Array([1])}).groupby("size").size()
    n = df.groupby("k").cumsum()
"#;

/// The diagnostics of [`PROGRAM`].
const EXPECTED: &str = r#"error: expected `i64`, found `DataFrame[k: i64, s: str, v: f64]`
 --> main.byn:9:14
  |
9 |     a: int = df.groupby("k").max()
  |              ^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `DataFrame[k: i64, s: str, size: i64]`
  --> main.byn:10:14
   |
10 |     b: int = df.groupby(["k", "s"]).size()
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `DataFrame[s: str, k: f64, v: f64]`
  --> main.byn:11:14
   |
11 |     c: int = df.groupby("s").median(numeric_only=True)
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`

error: `sum` of every column cannot reduce `s`, a column of `str`
  --> main.byn:12:25
   |
12 |     d = df.groupby("k").sum()
   |                         ^^^ a column of `str`
   = note: write `sum(numeric_only=True)` to leave the columns of `str` out, or choose the columns to reduce

error: a grouping cannot reduce `k`, one of its keys, yet
  --> main.byn:13:25
   |
13 |     e = df.groupby("k")["k"].sum()
   |                         ^^^ a key, not a value
   = note: the result already holds each group's value of it

error: the frame has no column `w` to reduce for each group
  --> main.byn:14:25
   |
14 |     f = df.groupby("k").w.sum()
   |                         ^ not a column
   = note: its columns are `k`, `s`, `v`

error: `sum` of a grouping takes `numeric_only=True` or nothing
  --> main.byn:15:9
   |
15 |     g = df.groupby("k")["v"].sum(1)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: to reduce some columns only, choose them first: `df.groupby("symbol")[["price", "volume"]].sum()`

error: `mean` cannot reduce `s`, a column of `str`
  --> main.byn:16:25
   |
16 |     h = df.groupby("k")["s"].mean()
   |                         ^^^ a column of `str`
   = note: a column of `str` has `count`, `size`, `nunique`, `first`, `last`, `min` and `max`

error: `DataFrame[k: i64, s: str, v: f64]` has no column `w`
  --> main.byn:17:25
   |
17 |     i = df.groupby("k")["w"].max()
   |                         ^^^ not a column
   = note: its columns are `k`, `s`, `v`

error: `sum` of a grouping takes `numeric_only=True` or nothing
  --> main.byn:18:9
   |
18 |     j = df.groupby("k").sum(numeric_only=df)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: to reduce some columns only, choose them first: `df.groupby("symbol")[["price", "volume"]].sum()`

error: `size` of a grouping takes no arguments
  --> main.byn:19:9
   |
19 |     k = df.groupby("k").size(numeric_only=True)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments

error: `numeric_only=True` leaves out `s`, the one column chosen
  --> main.byn:20:25
   |
20 |     l = df.groupby("k")["s"].min(numeric_only=True)
   |                         ^^^ a column of `str`
   = note: a column of `str` is not a column of numbers

error: the result would have two columns named `size`
  --> main.byn:21:57
   |
21 |     m = DataFrame({"size": Array([1])}).groupby("size").size()
   |                                                         ^^^^ one name for two columns

error: `groupby` is followed by a reduction, which says what to work out for each group
  --> main.byn:22:9
   |
22 |     n = df.groupby("k").cumsum()
   |         ^^^^^^^^^^^^^^^ groups with nothing worked out
   = note: for example `df.groupby("symbol").sum()` or `df.groupby("symbol").agg(total=("volume", "sum"))`"#;

/// A reduction as a method gives the key columns, then each reduced column under its own
/// name and with the type `agg` gives it, or a column `size`; `numeric_only=True` leaves
/// the columns of `str` out. The mistakes: text under a sum of every column or of a chosen
/// one, a key or a missing column chosen, wrong arguments, `numeric_only=True` for one
/// column of `str`, a second column `size`, and a method a grouping does not have.
#[test]
fn reductions_written_as_methods_are_typed_and_checked() {
    assert_eq!(check_one(PROGRAM), EXPECTED);
}
