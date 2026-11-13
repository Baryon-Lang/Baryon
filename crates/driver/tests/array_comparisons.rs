//! Element-wise comparisons of arrays (roadmap 3.13.1), through every stage up to the type
//! checker: `a > b` with an array on either side is an array of `bool`s, and `&`, `|`, `^` and
//! `~` combine such arrays.
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

/// A comparison of an array with a number, of two arrays of different ranks and element types,
/// and of `Fixed` arrays is an array of `bool`s of the broadcast shape; `&`, `|`, `^` and `~`
/// keep it one, and `~` applies to integer arrays too.
#[test]
fn comparisons_of_arrays_are_arrays_of_bools() {
    let program = r"
Vec3 = Fixed[f64, (3,)]
Flags = Fixed[bool, (3,)]


def masks(grid: Array[f64, 2], row: Array[f64, 1], counts: Array[i64, 1]) -> Array[bool, 2]:
    high: Array[bool, 1] = row > 1.5
    equal: Array[bool, 1] = row == counts
    low: Array[bool, 1] = 2 >= counts
    either: Array[bool, 1] = (high | low) ^ ~equal
    flipped: Array[i64, 1] = ~counts
    inside: Flags = (Vec3(1.0, 2.0, 3.0) > 0.5) & (Vec3(1.0, 2.0, 3.0) != Vec3.zeros())
    return (grid > row) & either
";
    assert_eq!(check_one(program), "");
}

/// A chain of comparisons, an array against a `str`, `~` and `&` of float arrays, an array of
/// `bool`s as a condition, and `<` and `+` of `bool` arrays are errors.
#[test]
fn comparison_mistakes_are_reported() {
    let program = r#"
def mistakes(a: Array[f64, 1]):
    c = 0.0 < a < 2.0
    d = a > "x"
    e = ~a
    f = a & a
    if a > 1.0:
        pass
    flags = a > 1.0
    h = flags < flags
    k = flags + flags
"#;
    assert_eq!(
        check_one(program),
        r#"error: a chain of comparisons cannot compare arrays
 --> main.byn:2:9
  |
2 |     c = 0.0 < a < 2.0
  |         ^^^^^^^^^^^^^ more than one comparison
  = note: compare in pairs and combine them: `(a < b) & (b < c)`

error: `>` cannot combine `Array[f64, 1]` and `str`
 --> main.byn:3:10
  |
3 |     d = a > "x"
  |          ^^^^^^ shapes do not match
  = note: combine two arrays, or an array and a single number

error: `~` does not apply to `Array[f64, 1]`
 --> main.byn:4:9
  |
4 |     e = ~a
  |         ^^ its elements are not `bool`s or integers

error: `&` cannot mix `f64` and `f64`
 --> main.byn:5:9
  |
5 |     f = a & a
  |         ^^^^^ different number types
  = note: convert one side so both are the same type, such as `f64(x)`

error: a condition must be a `bool`, found `Array[bool, 1]`
 --> main.byn:6:8
  |
6 |     if a > 1.0:
  |        ^^^^^^^ not a `bool`
  = note: compare explicitly, such as `n != 0` or `x is not None`

error: `<` cannot order `bool` values
 --> main.byn:9:14
  |
9 |     h = flags < flags
  |              ^^^^^^^^ no order

error: `+` cannot mix `bool` and `bool`
  --> main.byn:10:9
   |
10 |     k = flags + flags
   |         ^^^^^^^^^^^^^ `bool` is not a number"#
    );
}

/// `a[mask]` and `a[positions]` on a rank-1 `Array` are arrays of its type, may be kept, and
/// take one value when assigned to, a literal becoming the element type.
#[test]
fn choices_of_elements_are_new_arrays() {
    let program = r"
def choices(a: InOut[Array[f64, 1]], order: Array[i64, 1]) -> Array[f64, 1]:
    kept: Array[f64, 1] = a[a > 0.0]
    sorted_copy = a[order]
    a[a < 0.0] = 0
    a[order] = kept[0]
    return sorted_copy[sorted_copy > kept]
";
    assert_eq!(check_one(program), "");
}

/// A mask on a matrix, positions that are floats, an array among several indices, a choice
/// from a list, and an array or a `str` assigned to a choice are errors.
#[test]
fn choice_mistakes_are_reported() {
    let program = r#"
def mistakes(a: InOut[Array[f64, 1]], grid: Array[f64, 2]):
    b = grid[grid > 0.0]
    c = a[Array([1.0])]
    d = a[a < 0.0, 1]
    e = [1, 2][Array([0])]
    a[a > 5.0] = a
    a[a > 5.0] = "x"
"#;
    assert_eq!(
        check_one(program),
        r#"error: `Array[f64, 2]` cannot be indexed by `Array[bool, 2]`
 --> main.byn:2:9
  |
2 |     b = grid[grid > 0.0]
  |         ^^^^^^^^^^^^^^^^ not a choice of elements
  = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: `Array[f64, 1]` cannot be indexed by `Array[f64, 1]`
 --> main.byn:3:9
  |
3 |     c = a[Array([1.0])]
  |         ^^^^^^^^^^^^^^^ not a choice of elements
  = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: `Array[f64, 1]` cannot be indexed by `Array[bool, 1]`
 --> main.byn:4:9
  |
4 |     d = a[a < 0.0, 1]
  |         ^^^^^^^^^^^^^ not a choice of elements
  = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: `list[i64]` cannot be indexed by `Array[i64, 1]`
 --> main.byn:5:9
  |
5 |     e = [1, 2][Array([0])]
  |         ^^^^^^^^^^^^^^^^^^ not a choice of elements
  = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: the chosen elements take one `f64`, found `Array[f64, 1]`
 --> main.byn:6:18
  |
6 |     a[a > 5.0] = a
  |                  ^ not a single value
  = note: every chosen element is set to the same value, as in `a[a < 0.0] = 0.0`

error: expected `f64`, found `str`
 --> main.byn:7:18
  |
7 |     a[a > 5.0] = "x"
  |                  ^^^ each chosen element holds `f64`"#
    );
}

/// `a.argsort()` of a rank-1 array is an `Array[i64, 1]`. An array of higher rank, complex
/// elements, which have no order, and `NumPy`'s `kind=` and `axis` arguments are reported.
#[test]
fn argsort_gives_positions_and_reports_mistakes() {
    let program = r#"
def sorting(grid: Array[f64, 2], waves: Array[complex, 1], prices: Array[f64, 1]):
    a = grid.argsort()
    b = waves.argsort()
    c = prices.argsort(kind="stable")
    d = prices.argsort(0)
    wrong: f64 = prices.argsort()
"#;
    assert_eq!(
        check_one(program),
        r#"error: `Array[f64, 2]` has no method `argsort` taking these arguments
 --> main.byn:2:9
  |
2 |     a = grid.argsort()
  |         ^^^^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill`, `tofile` and, for one dimension, `argsort`, `unique` and `nunique`

error: `argsort` needs elements with an order, found `Array[complex128, 1]`
 --> main.byn:3:9
  |
3 |     b = waves.argsort()
  |         ^^^^^^^^^^^^^^^ complex numbers

error: `Array[f64, 1]` has no method `argsort` taking these arguments
 --> main.byn:4:9
  |
4 |     c = prices.argsort(kind="stable")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill`, `tofile` and, for one dimension, `argsort`, `unique` and `nunique`

error: `Array[f64, 1]` has no method `argsort` taking these arguments
 --> main.byn:5:9
  |
5 |     d = prices.argsort(0)
  |         ^^^^^^^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill`, `tofile` and, for one dimension, `argsort`, `unique` and `nunique`

error: expected `f64`, found `Array[i64, 1]`
 --> main.byn:6:18
  |
6 |     wrong: f64 = prices.argsort()
  |                  ^^^^^^^^^^^^^^^^ the annotation says `f64`"#
    );
}

/// `a.unique()` of a rank-1 array is an array like it and `a.nunique()` an `i64`. An array
/// of higher rank, complex elements, and an argument are errors, and so is a result used as
/// the wrong type.
#[test]
fn unique_and_nunique_are_typed_and_report_mistakes() {
    let program = r"def distinct(
    grid: Array[f64, 2], waves: Array[complex128, 1], days: Array[i32, 1]
):
    kept: Array[i32, 1] = days.unique()
    count: i64 = days.nunique()
    a = grid.unique()
    b = waves.nunique()
    c = days.unique(False)
    wrong: f64 = days.unique()
";
    assert_eq!(
        check_one(program),
        r"error: `Array[f64, 2]` has no method `unique` taking these arguments
 --> main.byn:6:9
  |
6 |     a = grid.unique()
  |         ^^^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill`, `tofile` and, for one dimension, `argsort`, `unique` and `nunique`

error: `nunique` needs elements with an order, found `Array[complex128, 1]`
 --> main.byn:7:9
  |
7 |     b = waves.nunique()
  |         ^^^^^^^^^^^^^^^ complex numbers

error: `Array[i32, 1]` has no method `unique` taking these arguments
 --> main.byn:8:9
  |
8 |     c = days.unique(False)
  |         ^^^^^^^^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill`, `tofile` and, for one dimension, `argsort`, `unique` and `nunique`

error: expected `f64`, found `Array[i32, 1]`
 --> main.byn:9:18
  |
9 |     wrong: f64 = days.unique()
  |                  ^^^^^^^^^^^^^ the annotation says `f64`"
    );
}
