//! `merge` with `left_on=`, `right_on=` and `suffixes=` (roadmap 3.13.15, decision 0045), up
//! to the type checker: the mistakes it reports in the keys and the suffixes.
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

/// Mistakes in the keys and the suffixes: `on=` beside `left_on=`, `left_on=` alone, sides
/// that name another number of columns, keys of two types, a name that is no column of its
/// frame, suffixes that leave two columns with one name, suffixes that are not two
/// literals, and no keys at all.
const MISTAKES: &str = r#"
def main():
    l = DataFrame({"sym": Array(["a", "b"]), "px": Array([1.0, 2.0]), "n": Array([1, 2])})
    r = DataFrame({"ticker": Array(["b"]), "px": Array([20.0]), "n": Array([5])})
    a = l.merge(r, on="sym", left_on="sym")
    b = l.merge(r, left_on="sym")
    c = l.merge(r, left_on=["sym", "n"], right_on="ticker")
    d = l.merge(r, left_on="sym", right_on="n")
    e = l.merge(r, left_on="sym", right_on="sym")
    f = l.merge(r, left_on="ticker", right_on="ticker")
    g = l.merge(r, left_on="sym", right_on="ticker", suffixes=("", ""))
    h = l.merge(r, left_on="sym", right_on="ticker", suffixes="ab")
    i = l.merge(r, left_on="sym", right_on="ticker", suffixes=("_a",))
    ends = ("_a", "_b")
    j = l.merge(r, left_on="sym", right_on="ticker", suffixes=ends)
    k = l.merge(r, left_on=[], right_on=[])
"#;

/// What the checker says of [`MISTAKES`].
const EXPECTED: &str = r#"error: `merge` takes the other frame, then the columns to join on as `on=`
 --> main.byn:4:9
  |
4 |     a = l.merge(r, on="sym", left_on="sym")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `trades.merge(names, on="symbol")` or `on=["symbol", "day"]`; a key with two names is `left_on=` and `right_on=` together, in place of `on=`

error: `merge` takes the other frame, then the columns to join on as `on=`
 --> main.byn:5:9
  |
5 |     b = l.merge(r, left_on="sym")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `trades.merge(names, on="symbol")` or `on=["symbol", "day"]`; a key with two names is `left_on=` and `right_on=` together, in place of `on=`

error: `left_on` names 2 columns and `right_on` names 1 column
 --> main.byn:6:51
  |
6 |     c = l.merge(r, left_on=["sym", "n"], right_on="ticker")
  |                                                   ^^^^^^^^ another number of columns
  = note: the first of each is one key, the second the next, and so on

error: the column `sym` is `str` and the column `n` is `i64`
 --> main.byn:7:44
  |
7 |     d = l.merge(r, left_on="sym", right_on="n")
  |                                            ^^^ two types for one key
  = note: a key has one type in both frames; convert one column first, with `assign`

error: `DataFrame[ticker: str, px: f64, n: i64]` has no column `sym`
 --> main.byn:8:44
  |
8 |     e = l.merge(r, left_on="sym", right_on="sym")
  |                                            ^^^^^ not a column
  = note: its columns are `ticker`, `px`, `n`

error: `DataFrame[sym: str, px: f64, n: i64]` has no column `ticker`
 --> main.byn:9:28
  |
9 |     f = l.merge(r, left_on="ticker", right_on="ticker")
  |                            ^^^^^^^^ not a column
  = note: its columns are `sym`, `px`, `n`

error: the joined frame would have two columns named `px`
  --> main.byn:10:9
   |
10 |     g = l.merge(r, left_on="sym", right_on="ticker", suffixes=("", ""))
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ one name for two columns
   = note: give `merge` two different `suffixes=`, or rename one of them first, with `rename(columns={...})`

error: `suffixes` is two string literals, such as `("_left", "_right")`
  --> main.byn:11:63
   |
11 |     h = l.merge(r, left_on="sym", right_on="ticker", suffixes="ab")
   |                                                               ^^^^ not two suffixes
   = note: the joined frame's column names depend on them, so they must be known when compiling; `None` or `""` adds nothing to a name

error: `suffixes` is two string literals, such as `("_left", "_right")`
  --> main.byn:12:63
   |
12 |     i = l.merge(r, left_on="sym", right_on="ticker", suffixes=("_a",))
   |                                                               ^^^^^^^ not two suffixes
   = note: the joined frame's column names depend on them, so they must be known when compiling; `None` or `""` adds nothing to a name

error: `suffixes` is two string literals, such as `("_left", "_right")`
  --> main.byn:14:63
   |
14 |     j = l.merge(r, left_on="sym", right_on="ticker", suffixes=ends)
   |                                                               ^^^^ not two suffixes
   = note: the joined frame's column names depend on them, so they must be known when compiling; `None` or `""` adds nothing to a name

error: `merge` needs at least one column to join on
  --> main.byn:15:28
   |
15 |     k = l.merge(r, left_on=[], right_on=[])
   |                            ^^ no columns

error: `merge` needs at least one column to join on
  --> main.byn:15:41
   |
15 |     k = l.merge(r, left_on=[], right_on=[])
   |                                         ^^ no columns"#;

#[test]
fn mistakes_in_keys_and_suffixes_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}
