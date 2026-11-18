//! Arithmetic on arrays with missing values (roadmap 3.13.6, decision 0045), up to the type
//! checker: the type each operator gives, and the mistakes and the operators not built yet
//! that it reports.
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

/// The result of each operator is an array with missing values whose elements have the type
/// the operator gives two plain arrays, so each annotation here holds.
#[test]
fn arithmetic_keeps_the_gaps_in_the_type() {
    let program = r"
def uses(asks: Array[f64, 1], counts: Array[i64 | None, 1]):
    bids: Array[f64 | None, 1] = Array([1.5, None])
    total: Array[f64 | None, 1] = bids + asks
    spread: Array[f64 | None, 1] = asks - bids
    scaled: Array[f64 | None, 1] = 2 * bids
    ratio: Array[f64 | None, 1] = bids / asks
    whole: Array[f64 | None, 1] = bids // 2.0
    rest: Array[f64 | None, 1] = bids % asks
    turned: Array[f64 | None, 1] = -bids
    more: Array[i64 | None, 1] = counts + 1
    fewer: Array[i64 | None, 1] = counts // counts
    shared: Array[f64 | None, 1] = counts / counts
    half: Array[f64 | None, 1] = counts / 2
    mixed: Array[f64 | None, 1] = counts * bids
    present: Array[f64, 1] = (bids + asks).fillna(0.0)
";
    assert_eq!(check_one(program), "");
}

/// A program with one mistake, or one operator on an array with missing values that is not
/// built yet, on each line.
const MISTAKES: &str = r#"
def uses(grid: Array[f64, 2]):
    bids: Array[f64 | None, 1] = Array([1.5, None])
    counts: Array[i64 | None, 1] = Array([1, None])
    flags: Array[bool | None, 1] = Array([True, None])
    names: Array[str | None, 1] = Array(["a", None])
    power = bids ** 2.0
    both = flags + flags
    joined = names + names
    wide = bids + grid
    text = bids + "x"
    flipped = ~counts
    turned = -flags
    masked = counts & counts
    larger = bids in bids
    bids += 1.0
    plain: Array[f64, 1] = bids + 1.0
"#;

/// What the checker reports for [`MISTAKES`], in source order.
const EXPECTED: &str = r#"error: `**` is not built for an array with missing values yet
 --> main.byn:6:13
  |
6 |     power = bids ** 2.0
  |             ^^^^^^^^^^^ an array that may hold `None`
  = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops over numbers and `bool`s, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//` and `%`; the rest comes with roadmap item 3.13.6

error: `+` cannot combine `Array[bool | None, 1]` and `Array[bool | None, 1]`
 --> main.byn:7:12
  |
7 |     both = flags + flags
  |            ^^^^^^^^^^^^^ an array that may hold `None`
  = note: an array of numbers with missing values combines with another, with an `Array` of numbers of one dimension, or with a number

error: `+` cannot combine `Array[str | None, 1]` and `Array[str | None, 1]`
 --> main.byn:8:14
  |
8 |     joined = names + names
  |              ^^^^^^^^^^^^^ an array that may hold `None`
  = note: an array of numbers with missing values combines with another, with an `Array` of numbers of one dimension, or with a number

error: `+` cannot combine `Array[f64 | None, 1]` and `Array[f64, 2]`
 --> main.byn:9:12
  |
9 |     wide = bids + grid
  |            ^^^^^^^^^^^ an array that may hold `None`
  = note: an array of numbers with missing values combines with another, with an `Array` of numbers of one dimension, or with a number

error: `+` cannot combine `Array[f64 | None, 1]` and `str`
  --> main.byn:10:12
   |
10 |     text = bids + "x"
   |            ^^^^^^^^^^ an array that may hold `None`
   = note: an array of numbers with missing values combines with another, with an `Array` of numbers of one dimension, or with a number

error: `~` is not built for an array with missing values yet
  --> main.byn:11:15
   |
11 |     flipped = ~counts
   |               ^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops over numbers and `bool`s, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//` and `%`; the rest comes with roadmap item 3.13.6

error: `-` does not apply to `Array[bool | None, 1]`
  --> main.byn:12:14
   |
12 |     turned = -flags
   |              ^^^^^^ its elements are not numbers

error: `&` is not built for an array with missing values yet
  --> main.byn:13:14
   |
13 |     masked = counts & counts
   |              ^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops over numbers and `bool`s, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//` and `%`; the rest comes with roadmap item 3.13.6

error: `in` is not built for an array with missing values yet
  --> main.byn:14:18
   |
14 |     larger = bids in bids
   |                  ^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops over numbers and `bool`s, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//` and `%`; the rest comes with roadmap item 3.13.6

error: `+=` is not built for an array with missing values yet
  --> main.byn:15:5
   |
15 |     bids += 1.0
   |     ^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None` and for numbers and `bool`s can be assigned, `for` loops over numbers and `bool`s, slices, masks and arrays of positions, the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and `all`, the comparisons, and for numbers `+`, `-`, `*`, `/`, `//` and `%`; the rest comes with roadmap item 3.13.6

error: expected `Array[f64, 1]`, found `Array[f64 | None, 1]`
  --> main.byn:16:28
   |
16 |     plain: Array[f64, 1] = bids + 1.0
   |                            ^^^^^^^^^^ the annotation says `Array[f64, 1]`"#;

#[test]
fn mistakes_in_arithmetic_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}
