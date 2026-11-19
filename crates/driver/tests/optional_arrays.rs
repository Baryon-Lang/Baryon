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
    texts: Array[list[i64] | None, 1] = Array([[1]])
    plain: Array[f64, 1] = Array([1.0, None])
    wrong: Array[f64 | None, 1] = asks.copy()
    number: f64 = bids[0]
    both = bids[0, 1]
    part = bids[[0.5]]
    chosen = bids[asks]
    bids[0] = "two"
    bids[asks > 1.0] = asks
    Array(["a", None])[0] = "b"
    more = bids @ bids
    less = ~bids
    same = bids is None
    found = 1.5 in bids
    total = bids.median()
    filled = bids.fillna("none")
    shape = bids.shape
    least = min(bids)
    for name in Array(["a", None]):
        size = len(name)
    print({1: bids.copy()})
    text = f"{[{2: bids.copy()}]}"
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

error: an array cannot hold `list[i64] | None` yet
 --> main.byn:9:18
  |
9 |     texts: Array[list[i64] | None, 1] = Array([[1]])
  |                  ^^^^^^^^^^^^^^^^ not a number, a `bool` or `str`, or `None`
  = note: an array with missing values holds numbers, `bool`s or `str` today, such as `Array[f64 | None, 1]`

error: an array's elements must be a copy type, found `list[i64]`
 --> main.byn:9:47
  |
9 |     texts: Array[list[i64] | None, 1] = Array([[1]])
  |                                               ^^^^^ not a copy type

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

error: `Array[f64 | None, 1]` cannot be indexed by `list[f64]`
  --> main.byn:14:12
   |
14 |     part = bids[[0.5]]
   |            ^^^^^^^^^^^ not a choice of elements
   = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: `Array[f64 | None, 1]` cannot be indexed by `Array[f64, 1]`
  --> main.byn:15:14
   |
15 |     chosen = bids[asks]
   |              ^^^^^^^^^^ not a choice of elements
   = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: expected `f64 | None`, found `str`
  --> main.byn:16:15
   |
16 |     bids[0] = "two"
   |               ^^^^^ the target holds `f64 | None`

error: the chosen elements take one `f64 | None`, found `Array[f64, 1]`
  --> main.byn:17:24
   |
17 |     bids[asks > 1.0] = asks
   |                        ^^^^ not a single value
   = note: every chosen element is set to the same value, as in `a[a < 0.0] = 0.0`

error: assigning an element of text is not built for an array with missing values yet
  --> main.byn:18:5
   |
18 |     Array(["a", None])[0] = "b"
   |     ^^^^^^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: `@` is not built for an array with missing values yet
  --> main.byn:19:12
   |
19 |     more = bids @ bids
   |            ^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: `~` does not apply to `Array[f64 | None, 1]`
  --> main.byn:20:12
   |
20 |     less = ~bids
   |            ^^^^^ its elements are not whole numbers or `bool`s

error: `is` is not built for an array with missing values yet
  --> main.byn:21:16
   |
21 |     same = bids is None
   |                ^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: `in` is not built for an array with missing values yet
  --> main.byn:22:16
   |
22 |     found = 1.5 in bids
   |                ^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: the method `median` is not built for an array with missing values yet
  --> main.byn:23:13
   |
23 |     total = bids.median()
   |             ^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: expected `f64`, found `str`
  --> main.byn:24:26
   |
24 |     filled = bids.fillna("none")
   |                          ^^^^^^ the array holds `f64`

error: `shape` is not built for an array with missing values yet
  --> main.byn:25:13
   |
25 |     shape = bids.shape
   |             ^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: `min` is not built for an array with missing values yet
  --> main.byn:26:17
   |
26 |     least = min(bids)
   |                 ^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`, for whole numbers `&`, `|`, `^` and `~`; the rest comes with roadmap item 3.24

error: a value that holds an array with missing values cannot be shown yet
  --> main.byn:29:11
   |
29 |     print({1: bids.copy()})
   |           ^^^^^^^^^^^^^^^^ holds an array that may hold `None`
   = note: show the array or the frame alone, `print(values)`, or in a tuple or a list; a dict, a `T | None` and a frame inside another value are not built

error: a value that holds an array with missing values cannot be shown yet
  --> main.byn:30:15
   |
30 |     text = f"{[{2: bids.copy()}]}"
   |               ^^^^^^^^^^^^^^^^^^ holds an array that may hold `None`
   = note: show the array or the frame alone, `print(values)`, or in a tuple or a list; a dict, a `T | None` and a frame inside another value are not built

error: `bids` was moved
  --> main.byn:32:17
   |
32 |     again = len(bids)
   |                 ^^^^ read after the move
  ::: main.byn:31:13
   |
31 |     moved = bids
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
    twin[0] = 7
    twin[1] = None
    flags[1] = flags[0]
    there = bids.notna() & flags.isna()
    part: Array[f64 | None, 1] = bids[1:]
    for bid in bids:
        if bid is not None:
            count += int(bid)
    for at, flag in enumerate(reversed(flags)):
        count += at
    kept: Array[f64 | None, 1] = bids[there]
    known: Array[bool | None, 1] = flags[flags.notna()]
    taken: Array[i64 | None, 1] = whole[Array([1, 0])]
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

/// A generic function that shows its `T` is an error at a call that makes `T` a frame, an
/// array with missing values, a text array or a `Path`, or a value that holds one (roadmap
/// 1.9.67): its body is built once, where the value's type is `T`, so it would print the
/// parts the compiler stores the value in. A function that shows something else, and a call
/// at a type that prints alone, are not reported.
#[test]
fn values_shown_through_a_type_parameter_are_reported() {
    let program = r#"
from pathlib import Path


def say[T](value: T):
    print(value)


def first[T](xs: list[T]) -> str:
    return f"{xs[0]}"


def count[T](value: T) -> i64:
    print("counting")
    return 1


def main():
    gaps: Array[f64 | None, 1] = Array([1.0, None])
    say(gaps)
    say(DataFrame({"price": Array([1.5])}))
    print(first([Path("/a")]))
    say(Array(["x"]))
    n = count(gaps)
    say(1.5)
"#;
    assert_eq!(
        check_one(program),
        "\
error: `say` shows its `T`, an array with missing values here, which cannot be shown \
through a type parameter yet
  --> main.byn:19:5
   |
19 |     say(gaps)
   |     ^^^^^^^^^ shown through a type parameter
   = note: show the value outside the generic function, `print(value)`; showing it \
through `T` is idea I.71

error: `say` shows its `T`, a frame here, which cannot be shown \
through a type parameter yet
  --> main.byn:20:5
   |
20 |     say(DataFrame({\"price\": Array([1.5])}))
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ shown through a type parameter
   = note: show the value outside the generic function, `print(value)`; showing it \
through `T` is idea I.71

error: `first` shows its `T`, a `Path` here, which cannot be shown \
through a type parameter yet
  --> main.byn:21:11
   |
21 |     print(first([Path(\"/a\")]))
   |           ^^^^^^^^^^^^^^^^^^^ shown through a type parameter
   = note: show the value outside the generic function, `print(value)`; showing it \
through `T` is idea I.71

error: `say` shows its `T`, a text array here, which cannot be shown \
through a type parameter yet
  --> main.byn:22:5
   |
22 |     say(Array([\"x\"]))
   |     ^^^^^^^^^^^^^^^^^ shown through a type parameter
   = note: show the value outside the generic function, `print(value)`; showing it \
through `T` is idea I.71"
    );
}
