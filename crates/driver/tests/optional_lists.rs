//! A list written with `None` among its items (roadmap 3.13.6, decision 0045 part 3), through
//! every stage up to the type checker: the type it takes with no annotation, and the lists
//! that stay mistakes.
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

/// A program with one mistake on each line: two types other than `None` in one list, with and
/// without a `None` between them, an item that may be `None` used as a number, and a `None`
/// in a list whose written type has no room for one.
const MISTAKES: &str = r#"
def uses():
    mixed = [1, "a"]
    more = [1.5, None, "a"]
    wide = [1, None, 2.5]
    bids = [1.5, None]
    first: f64 = bids[0]
    plain: list[f64] = [1.5, None]
"#;

/// What the checker reports for [`MISTAKES`], in source order.
const EXPECTED: &str = r#"error: list items must share one type, found `i64` and `str`
 --> main.byn:2:17
  |
2 |     mixed = [1, "a"]
  |                 ^^^ a different type

error: list items must share one type, found `f64` and `str`
 --> main.byn:3:24
  |
3 |     more = [1.5, None, "a"]
  |                        ^^^ a different type

error: list items must share one type, found `i64` and `f64`
 --> main.byn:4:22
  |
4 |     wide = [1, None, 2.5]
  |                      ^^^ a different type

error: this value may be `None`
 --> main.byn:6:18
  |
6 |     first: f64 = bids[0]
  |                  ^^^^^^^ has type `f64 | None`
  = note: give it a name and check it first: `if value is not None:`

error: list items must share one type, found `f64` and `None`
 --> main.byn:7:30
  |
7 |     plain: list[f64] = [1.5, None]
  |                              ^^^^ a different type"#;

/// Lists that are clean: each takes the type its annotation on the next line repeats, which
/// the checker would reject were the inferred type any other.
const INFERRED: &str = r"
def uses():
    bids = [1.5, None, 2.5]
    same: list[f64 | None] = bids
    first = [None, 4]
    counts: list[i64 | None] = first
    gaps = [None, None]
    nothing: list[None] = gaps
    nested = [[1.5, None], [2.5]]
    lists: list[list[f64 | None]] = nested
    array = Array([True, None])
    flags: Array[bool | None, 1] = array
";

#[test]
fn mistakes_in_lists_with_none_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}

#[test]
fn a_list_with_none_takes_an_optional_item_type() {
    assert_eq!(check_one(INFERRED), "");
}
