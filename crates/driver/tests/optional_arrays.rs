//! Arrays with missing values, `Array[T | None, 1]` (roadmap 3.13.6, decision 0045), through
//! every stage up to the type checker: the type, building one from a list, what reads it,
//! and the mistakes and the uses not built yet that it reports.
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

/// A program with one mistake, or one use of an array with missing values that is not built
/// yet, on each line.
const MISTAKES: &str = r#"
class Book(Struct):
    depth: i64
    bids: Array[f64 | None, 1]


def uses(asks: Array[f64, 1]):
    bids: Array[f64 | None, 1] = Array([1.5, None])
    grid: Array[f64 | None, 2] = Array([1.0])
    texts: Array[str | None, 1] = Array(["a"])
    plain: Array[f64, 1] = Array([1.0, None])
    wrong: Array[f64 | None, 1] = asks.copy()
    number: f64 = bids[0]
    both = bids[0, 1]
    part = bids[1:]
    chosen = bids[asks > 1.0]
    bids[0] = 2.0
    more = bids + 1.0
    less = -bids
    same = bids == asks
    found = 1.5 in bids
    total = bids.std()
    filled = bids.fillna("none")
    shape = bids.shape
    least = min(bids)
    for bid in bids:
        pass
    print((bids.copy(), 1))
    text = f"{[bids.copy()]}"
    moved = bids
    again = len(bids)
"#;

/// What the checker reports for [`MISTAKES`], in source order.
const EXPECTED: &str = r#"error: a struct field cannot hold an array with missing values yet
 --> main.byn:3:11
  |
3 |     bids: Array[f64 | None, 1]
  |           ^^^^^^^^^^^^^^^^^^^^ holds an array that may hold `None`
  = note: keep the array in a variable and pass it beside the struct; like a frame in a struct field, this is roadmap idea I.49

error: an array with missing values has one dimension
 --> main.byn:8:29
  |
8 |     grid: Array[f64 | None, 2] = Array([1.0])
  |                             ^ not 1
  = note: write `Array[f64 | None, 1]`; more dimensions are roadmap item 3.24

error: an array cannot hold `str | None` yet
 --> main.byn:9:18
  |
9 |     texts: Array[str | None, 1] = Array(["a"])
  |                  ^^^^^^^^^^ not a number or `bool`, or `None`
  = note: an array with missing values holds numbers or `bool`s today, such as `Array[f64 | None, 1]`; the rest comes with roadmap item 3.13.6

error: list items must share one type, found `f64` and `None`
  --> main.byn:10:40
   |
10 |     plain: Array[f64, 1] = Array([1.0, None])
   |                                        ^^^^ a different type

error: expected `Array[f64 | None, 1]`, found `Array[f64, 1]`
  --> main.byn:11:35
   |
11 |     wrong: Array[f64 | None, 1] = asks.copy()
   |                                   ^^^^^^^^^^^ the annotation says `Array[f64 | None, 1]`

error: this value may be `None`
  --> main.byn:12:19
   |
12 |     number: f64 = bids[0]
   |                   ^^^^^^^ has type `f64 | None`
   = note: give it a name and check it first: `if value is not None:`

error: an array with missing values has one dimension, so it takes one index
  --> main.byn:13:12
   |
13 |     both = bids[0, 1]
   |            ^^^^^^^^^^ more than one index
   = note: for example `values[i]`

error: a slice, a mask or a list of positions is not built for an array with missing values yet
  --> main.byn:14:12
   |
14 |     part = bids[1:]
   |            ^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: a slice, a mask or a list of positions is not built for an array with missing values yet
  --> main.byn:15:14
   |
15 |     chosen = bids[asks > 1.0]
   |              ^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: assigning an element is not built for an array with missing values yet
  --> main.byn:16:5
   |
16 |     bids[0] = 2.0
   |     ^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: `+` is not built for an array with missing values yet
  --> main.byn:17:12
   |
17 |     more = bids + 1.0
   |            ^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: `-` is not built for an array with missing values yet
  --> main.byn:18:12
   |
18 |     less = -bids
   |            ^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: `==` is not built for an array with missing values yet
  --> main.byn:19:16
   |
19 |     same = bids == asks
   |                ^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: `in` is not built for an array with missing values yet
  --> main.byn:20:16
   |
20 |     found = 1.5 in bids
   |                ^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: the method `std` is not built for an array with missing values yet
  --> main.byn:21:13
   |
21 |     total = bids.std()
   |             ^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: expected `f64`, found `str`
  --> main.byn:22:26
   |
22 |     filled = bids.fillna("none")
   |                          ^^^^^^ the array holds `f64`

error: `shape` is not built for an array with missing values yet
  --> main.byn:23:13
   |
23 |     shape = bids.shape
   |             ^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: `min` is not built for an array with missing values yet
  --> main.byn:24:17
   |
24 |     least = min(bids)
   |                 ^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: a `for` loop is not built for an array with missing values yet
  --> main.byn:25:16
   |
25 |     for bid in bids:
   |                ^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: a value that holds an array with missing values cannot be shown yet
  --> main.byn:27:11
   |
27 |     print((bids.copy(), 1))
   |           ^^^^^^^^^^^^^^^^ holds an array that may hold `None`
   = note: show the array or the frame alone: `print(values)`

error: a value that holds an array with missing values cannot be shown yet
  --> main.byn:28:15
   |
28 |     text = f"{[bids.copy()]}"
   |               ^^^^^^^^^^^^^ holds an array that may hold `None`
   = note: show the array or the frame alone: `print(values)`

error: `bids` was moved
  --> main.byn:30:17
   |
30 |     again = len(bids)
   |                 ^^^^ read after the move
  ::: main.byn:29:13
   |
29 |     moved = bids
   |             ---- `bids` moved here
   = note: to keep both, move a copy: `bids.copy()`"#;

/// A program that uses an array with missing values only in the ways that are built.
const BUILT: &str = r"
def fine(bids: Array[f64 | None, 1]) -> Array[bool, 1]:
    count = len(bids)
    first = bids[0]
    if first is not None:
        count += int(first)
    last: f64 | None = bids[count - 1]
    flags: Array[bool | None, 1] = Array([True, None])
    whole: Array[i64 | None, 1] = Array([None, 2])
    twin = whole.copy()
    there = bids.notna() & flags.isna()
    print(bids[0], whole[1], len(flags))
    spread = bids.max() - bids.min() + bids.sum() / bids.mean() + f64(whole.sum())
    count = flags.count() + bids.count() + whole.max()
    return there | bids.isna()
";

/// Each mistake and each use that is not built yet is reported once, where it is, with what
/// an array with missing values has today.
#[test]
fn mistakes_with_an_array_with_missing_values_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}

/// The type, `Array(items)`, `len`, an element read as a `T | None` and narrowed, `isna`,
/// `notna` and `copy` check clean.
#[test]
fn the_built_uses_of_an_array_with_missing_values_check_clean() {
    assert_eq!(check_one(BUILT), "");
}
