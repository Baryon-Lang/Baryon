//! pandas's arguments on `drop_duplicates`, `value_counts`, `describe` and `sort_values`
//! (roadmap 3.13.16, decision 0045), up to the type checker: the mistakes it reports in
//! them.
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

/// Mistakes in the arguments: a `keep` that is none of the three, an argument twice or one
/// the method does not take, a flag that is not `True` or `False` written out, a column
/// named `proportion` counted with `normalize=True`, percentiles outside 0 to 1, listed
/// twice or not a list of numbers, and an `na_position` that is neither place or is not
/// written out.
const MISTAKES: &str = r#"
def main():
    flag = True
    where = "first"
    df = DataFrame({"symbol": Array(["a", "b"]), "proportion": Array([1.0, 2.0])})
    print(df.drop_duplicates(keep="middle"))
    print(df.drop_duplicates(keep=True))
    print(df.drop_duplicates("symbol", subset="symbol"))
    print(df.drop_duplicates(keep="last", keep="first"))
    print(df.drop_duplicates(sort=False))
    print(df.value_counts("symbol", sort=flag))
    print(df.value_counts("symbol", normalize=1))
    print(df.value_counts(normalize=True))
    print(df.value_counts("symbol", keep="last"))
    print(df.describe(percentiles=[0.5, 1.5, 0.5]))
    print(df.describe(percentiles=[flag]))
    print(df.describe(percentiles=0.5))
    print(df.describe(include="all"))
    print(df.sort_values("symbol", na_position="middle"))
    print(df.sort_values("symbol", na_position=where))
    print(df.sort_values("symbol", by="symbol"))
    print(df.sort_values("symbol", kind="stable"))
"#;

/// What the checker reports for [`MISTAKES`].
const EXPECTED: &str = r#"error: `keep` is `"first"`, `"last"` or `False`, written out
 --> main.byn:5:35
  |
5 |     print(df.drop_duplicates(keep="middle"))
  |                                   ^^^^^^^^ not a way to keep rows
  = note: `"first"` keeps the first of the rows that repeat, `"last"` the last, and `False` none of them

error: `keep` is `"first"`, `"last"` or `False`, written out
 --> main.byn:6:35
  |
6 |     print(df.drop_duplicates(keep=True))
  |                                   ^^^^ not a way to keep rows
  = note: `"first"` keeps the first of the rows that repeat, `"last"` the last, and `False` none of them

error: `drop_duplicates` takes `subset=`, the columns to compare, and `keep=`
 --> main.byn:7:11
  |
7 |     print(df.drop_duplicates("symbol", subset="symbol"))
  |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.drop_duplicates()` or `df.drop_duplicates(subset=["symbol", "day"], keep="last")`; the first of the rows that repeat is kept unless `keep=` says otherwise

error: `drop_duplicates` takes `subset=`, the columns to compare, and `keep=`
 --> main.byn:8:11
  |
8 |     print(df.drop_duplicates(keep="last", keep="first"))
  |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.drop_duplicates()` or `df.drop_duplicates(subset=["symbol", "day"], keep="last")`; the first of the rows that repeat is kept unless `keep=` says otherwise

error: `drop_duplicates` takes `subset=`, the columns to compare, and `keep=`
 --> main.byn:9:11
  |
9 |     print(df.drop_duplicates(sort=False))
  |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `df.drop_duplicates()` or `df.drop_duplicates(subset=["symbol", "day"], keep="last")`; the first of the rows that repeat is kept unless `keep=` says otherwise

error: `sort` is `True` or `False`, written out
  --> main.byn:10:42
   |
10 |     print(df.value_counts("symbol", sort=flag))
   |                                          ^^^^ not `True` or `False`
   = note: what the compiler builds depends on it, so it must be known when compiling

error: `normalize` is `True` or `False`, written out
  --> main.byn:11:47
   |
11 |     print(df.value_counts("symbol", normalize=1))
   |                                               ^ not `True` or `False`
   = note: what the compiler builds depends on it, so it must be known when compiling

error: `value_counts` cannot count by a column named `proportion`
  --> main.byn:12:11
   |
12 |     print(df.value_counts(normalize=True))
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the result would have two columns named `proportion`
   = note: rename it first: `df.rename(columns={"proportion": "n"})`

error: `value_counts` takes `subset=`, the columns to count by, then `normalize=`, `sort=`, `ascending=` and `dropna=`
  --> main.byn:13:11
   |
13 |     print(df.value_counts("symbol", keep="last"))
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.value_counts("symbol")` or `df.value_counts(["symbol", "day"], normalize=True)`

error: a percentile is from 0 to 1, found `1.5`
  --> main.byn:14:41
   |
14 |     print(df.describe(percentiles=[0.5, 1.5, 0.5]))
   |                                         ^^^ outside 0 to 1
   = note: a percentile is a part of 1: the 90th is `0.9`

error: `percentiles` lists `0.5` twice
  --> main.byn:14:46
   |
14 |     print(df.describe(percentiles=[0.5, 1.5, 0.5]))
   |                                              ^^^ listed before

error: `percentiles` is a list of numbers from 0 to 1, written out
  --> main.byn:15:35
   |
15 |     print(df.describe(percentiles=[flag]))
   |                                   ^^^^^^ not a list of numbers
   = note: for example `df.describe(percentiles=[0.1, 0.5, 0.9])`

error: `percentiles` is a list of numbers from 0 to 1, written out
  --> main.byn:16:35
   |
16 |     print(df.describe(percentiles=0.5))
   |                                   ^^^ not a list of numbers
   = note: for example `df.describe(percentiles=[0.1, 0.5, 0.9])`

error: `describe` takes `percentiles=`, or nothing
  --> main.byn:17:11
   |
17 |     print(df.describe(include="all"))
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: it describes every column of numbers; choose columns first with `df[["price", "volume"]].describe()`

error: `na_position` is `"first"` or `"last"`, written out
  --> main.byn:18:48
   |
18 |     print(df.sort_values("symbol", na_position="middle"))
   |                                                ^^^^^^^^ not a place for missing values
   = note: missing values come last when it is left out

error: `na_position` is `"first"` or `"last"`, written out
  --> main.byn:19:48
   |
19 |     print(df.sort_values("symbol", na_position=where))
   |                                                ^^^^^ not a place for missing values
   = note: missing values come last when it is left out

error: `sort_values` takes the columns to sort by, then `ascending=` and `na_position=`
  --> main.byn:20:11
   |
20 |     print(df.sort_values("symbol", by="symbol"))
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.sort_values("price")` or `df.sort_values(["symbol", "price"], ascending=[True, False])`; the sort is always stable, and missing values come last unless `na_position="first"`

error: `sort_values` takes the columns to sort by, then `ascending=` and `na_position=`
  --> main.byn:21:11
   |
21 |     print(df.sort_values("symbol", kind="stable"))
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `df.sort_values("price")` or `df.sort_values(["symbol", "price"], ascending=[True, False])`; the sort is always stable, and missing values come last unless `na_position="first"`"#;

#[test]
fn mistakes_in_the_arguments_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}
