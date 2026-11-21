//! The loop over a grouping (roadmap 3.13.25, decision 0045's amendment of 2026-11-19, point
//! 5), through every stage up to the type checker: the types of the key and the group, and
//! the mistakes reported.
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

/// The program of `a_loop_over_a_grouping_is_typed_and_checked`.
const PROGRAM: &str = r#"
class Row(Struct):
    k: i64
    s: str


def main() -> None:
    df: DataFrame[Row] = DataFrame({"k": Array([2, 1]), "s": Array(["b", "a"])})
    kept: list[DataFrame[Row]] = []
    for key, group in df.groupby("k"):
        wrong: str = key
        also: int = group
        kept.append(group)
    for pair, group in df.groupby(["k"]):
        other: int = pair
    for key, group in df.groupby("k"):
        df = group.copy()
    for key, group in df.groupby("w"):
        print(key)
    for key, group in df.groupby("k", as_index=True):
        print(key)
    for key, group in df.groupby():
        print(key)
    for key, group in df.groupby("k"):
        print(key)
    print(group)
    for a, b, c in df.groupby("k"):
        print(a)
    frames = [group for key, group in df.groupby("s")]
    g = df.groupby("k")
"#;

/// The diagnostics of [`PROGRAM`].
const EXPECTED: &str = r#"error: expected `str`, found `i64`
  --> main.byn:10:22
   |
10 |         wrong: str = key
   |                      ^^^ the annotation says `str`

error: `group` borrows an element of `df.groupby("k")`, so it cannot be moved
  --> main.byn:11:21
   |
11 |         also: int = group
   |                     ^^^^^ moved here
   = note: to keep the element, keep a copy: `group.copy()`

error: expected `i64`, found `DataFrame[Row]`
  --> main.byn:11:21
   |
11 |         also: int = group
   |                     ^^^^^ the annotation says `i64`

error: `group` borrows an element of `df.groupby("k")`, so it cannot be moved
  --> main.byn:12:21
   |
12 |         kept.append(group)
   |                     ^^^^^ moved here
   = note: to keep the element, keep a copy: `group.copy()`

error: expected `i64`, found `tuple[i64]`
  --> main.byn:14:22
   |
14 |         other: int = pair
   |                      ^^^^ the annotation says `i64`

error: `df` cannot be changed while a loop goes over it
  --> main.byn:16:9
   |
16 |         df = group.copy()
   |         ^^ changed here
  ::: main.byn:15:23
   |
15 |     for key, group in df.groupby("k"):
   |                       -- the loop goes over `df` here
   = note: loop over the indices instead: `for i in range(len(df)):`

error: `DataFrame[Row]` has no column `w`
  --> main.byn:17:34
   |
17 |     for key, group in df.groupby("w"):
   |                                  ^^^ not a column
   = note: its columns are `k`, `s`

error: a `DataFrame` has no index for `as_index=True` to make of the keys
  --> main.byn:19:48
   |
19 |     for key, group in df.groupby("k", as_index=True):
   |                                                ^^^^ no index
   = note: leave it out or write `as_index=False`: keys are columns of the result

error: `groupby` takes the columns whose values make the groups
  --> main.byn:21:23
   |
21 |     for key, group in df.groupby():
   |                       ^^^^^^^^^^^^ wrong arguments
   = note: for example `df.groupby("symbol")` or `df.groupby(["symbol", "day"])`

error: `group` is read after its loop ended
  --> main.byn:25:11
   |
25 |     print(group)
   |           ^^^^^ read after the loop
  ::: main.byn:9:14
   |
 9 |     for key, group in df.groupby("k"):
   |              ----- `group` borrows each element only while the loop runs
   = note: to keep an element, copy it inside the loop: `last = group.copy()`

error: a value of type `tuple[i64, DataFrame[Row]]` cannot be unpacked into 3 names
  --> main.byn:26:9
   |
26 |     for a, b, c in df.groupby("k"):
   |         ^^^^^^^ does not match the value

error: `group` borrows an element of `df.groupby("s")`, so it cannot be moved
  --> main.byn:28:15
   |
28 |     frames = [group for key, group in df.groupby("s")]
   |               ^^^^^ moved here
   = note: to keep the element, keep a copy: `group.copy()`

error: `groupby` is followed by a reduction, which says what to work out for each group
  --> main.byn:29:9
   |
29 |     g = df.groupby("k")
   |         ^^^^^^^^^^^^^^^ groups with nothing worked out
   = note: for example `df.groupby("symbol").sum()` or `df.groupby("symbol").agg(total=("volume", "sum"))`"#;

/// The key is the key column's value, or a tuple when the keys are a list, and the group
/// has the frame's type. The loop borrows the group, so moving it out is an error that
/// shows `group.copy()`, as is reading it after the loop; the frame cannot be replaced
/// inside the loop; `groupby`'s own mistakes are reported as for `agg`; and a grouping is
/// still not a value.
#[test]
fn a_loop_over_a_grouping_is_typed_and_checked() {
    assert_eq!(check_one(PROGRAM), EXPECTED);
}
