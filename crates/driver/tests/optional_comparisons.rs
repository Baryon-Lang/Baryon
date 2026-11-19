//! Comparisons on arrays with missing values (roadmap 3.13.6, decision 0045), up to the type
//! checker: the type each comparison gives, where such a mask is taken, and the mistakes and
//! the uses not built yet that it reports.
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

/// Each comparison gives an array of `bool`s with missing values, and so do `&`, `|`, `^` and
/// `~` of such masks, so each annotation here holds. Such a mask chooses rows of a frame and
/// chooses and sets elements of an array.
#[test]
fn comparisons_keep_the_gaps_in_the_type() {
    let program = r#"
def uses(asks: Array[f64, 1], counts: Array[i64 | None, 1]):
    bids: Array[f64 | None, 1] = Array([1.5, None])
    more: Array[bool | None, 1] = bids > asks
    less: Array[bool | None, 1] = asks <= bids
    same: Array[bool | None, 1] = bids == bids
    other: Array[bool | None, 1] = counts != 2
    mixed: Array[bool | None, 1] = counts >= bids
    summed: Array[bool | None, 1] = bids + asks < 2.0 * bids
    plain: Array[bool, 1] = (bids > 1.0).fillna(False)
    both: Array[bool | None, 1] = (bids > 1.0) & (counts < 3)
    either: Array[bool | None, 1] = (bids > 1.0) | (asks > 1.0)
    one: Array[bool | None, 1] = (asks > 1.0) ^ (bids > 1.0)
    turned: Array[bool | None, 1] = ~(bids > 1.0)
    names: Array[str | None, 1] = Array(["a", None])
    words = Array(["a", "b"])
    named: Array[bool | None, 1] = names == "a"
    ordered: Array[bool | None, 1] = "a" < names
    paired: Array[bool | None, 1] = names >= words
    twice: Array[bool | None, 1] = words != names
    flags: Array[bool | None, 1] = Array([True, None])
    set: Array[bool | None, 1] = flags == True
    unset: Array[bool | None, 1] = False != flags
    agree: Array[bool | None, 1] = flags == (bids > 1.0)
    plainly: Array[bool | None, 1] = (asks > 1.0) != flags
    counted: i64 = (bids > 1.0).sum()
    some: bool = (bids > 1.0).any()
    every: bool = flags.all()
    whole: bool = (asks > 1.0).all() or (asks > 1.0).any()
    chosen: Array[f64, 1] = asks[bids > 1.0]
    high: Array[f64 | None, 1] = bids[bids > 1.0]
    called: Array[str, 1] = words[names == "a"]
    known: Array[str | None, 1] = names[flags]
    work = asks.copy()
    work[~(bids > 1.0)] = 0.0
    frame = DataFrame({"bid": bids.copy(), "ask": asks.copy()})
    rows = frame[frame.bid > frame.ask]
    kept: Array[f64 | None, 1] = rows.bid.copy()
"#;
    assert_eq!(check_one(program), "");
}

/// A program with one mistake, or one use of a comparison on an array with missing values
/// that is not built yet, on each line.
const MISTAKES: &str = r#"
def uses(grid: Array[f64, 2], plain: Array[f64, 1]):
    bids: Array[f64 | None, 1] = Array([1.5, None])
    names: Array[str | None, 1] = Array(["a", None])
    flags: Array[bool | None, 1] = Array([True, None])
    a = bids > "x"
    b = names == 1
    c = flags < flags
    d = bids in bids
    e = bids is None
    f = 1.0 < bids < 2.0
    g = bids > grid
    h: Array[bool, 1] = bids > 1.0
    if bids > 1.0:
        pass
    i = grid[bids > 1.0]
    j = names[bids]
    k = (bids > 1.0) & True
    l = (bids > 1.0) | bids
    m = (bids > 1.0).any(axis=0)
    n = bids.any()
    o = plain.all()
"#;

/// What the checker reports for [`MISTAKES`], in source order.
const EXPECTED: &str = r#"error: `>` cannot compare `Array[f64 | None, 1]` and `str`
 --> main.byn:5:13
  |
5 |     a = bids > "x"
  |             ^^^^^^ an array that may hold `None`
  = note: an array with missing values compares with another, with an `Array` of one dimension, or with one value, of numbers, of text or of `bool`s

error: `==` cannot compare `Array[str | None, 1]` and `i64`
 --> main.byn:6:14
  |
6 |     b = names == 1
  |              ^^^^^ an array that may hold `None`
  = note: an array with missing values compares with another, with an `Array` of one dimension, or with one value, of numbers, of text or of `bool`s

error: `<` cannot order `bool` values
 --> main.byn:7:14
  |
7 |     c = flags < flags
  |              ^^^^^^^^ no order

error: `in` is not built for an array with missing values yet
 --> main.byn:8:13
  |
8 |     d = bids in bids
  |             ^^^^^^^^ an array that may hold `None`
  = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: `is` is not built for an array with missing values yet
 --> main.byn:9:13
  |
9 |     e = bids is None
  |             ^^^^^^^^ an array that may hold `None`
  = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: a chain of comparisons cannot compare arrays
  --> main.byn:10:9
   |
10 |     f = 1.0 < bids < 2.0
   |         ^^^^^^^^^^^^^^^^ more than one comparison
   = note: compare in pairs and combine them: `(a < b) & (b < c)`

error: `>` cannot compare `Array[f64 | None, 1]` and `Array[f64, 2]`
  --> main.byn:11:13
   |
11 |     g = bids > grid
   |             ^^^^^^^ an array that may hold `None`
   = note: an array with missing values compares with another, with an `Array` of one dimension, or with one value, of numbers, of text or of `bool`s

error: expected `Array[bool, 1]`, found `Array[bool | None, 1]`
  --> main.byn:12:25
   |
12 |     h: Array[bool, 1] = bids > 1.0
   |                         ^^^^^^^^^^ the annotation says `Array[bool, 1]`

error: a condition must be a `bool`, found `Array[bool | None, 1]`
  --> main.byn:13:8
   |
13 |     if bids > 1.0:
   |        ^^^^^^^^^^ not a `bool`
   = note: compare explicitly, such as `n != 0` or `x is not None`

error: choosing elements with it, other than of an array of one dimension by a mask, is not built for an array with missing values yet
  --> main.byn:15:9
   |
15 |     i = grid[bids > 1.0]
   |         ^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: choosing elements with it, other than of an array of one dimension by a mask, is not built for an array with missing values yet
  --> main.byn:16:9
   |
16 |     j = names[bids]
   |         ^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: `&` is not built for an array with missing values yet
  --> main.byn:17:9
   |
17 |     k = (bids > 1.0) & True
   |         ^^^^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: `|` is not built for an array with missing values yet
  --> main.byn:18:9
   |
18 |     l = (bids > 1.0) | bids
   |         ^^^^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: the method `any` is not built for an array with missing values yet
  --> main.byn:19:9
   |
19 |     m = (bids > 1.0).any(axis=0)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: the method `any` is not built for an array with missing values yet
  --> main.byn:20:9
   |
20 |     n = bids.any()
   |         ^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24

error: `all` needs an array of `bool`s, found `Array[f64, 1]`
  --> main.byn:21:9
   |
21 |     o = plain.all()
   |         ^^^^^^^^^^^ not `bool`s
   = note: compare first, for example `(a != 0).all()`"#;

#[test]
fn mistakes_in_comparisons_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}
