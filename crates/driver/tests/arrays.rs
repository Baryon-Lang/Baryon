//! Arrays, end to end: creating them, indexing and slicing, methods and attributes,
//! whole-array arithmetic and the matrix product, through every stage up to the type checker.
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

/// Slices, the matrix product, `Fixed` construction and functions, shapes, `dtype`, element
/// types from the context, `fill`, `Array(values)` and the methods all check clean.
#[test]
fn valid_array_code_checks_clean() {
    let program = r"
Vec3 = Fixed[f64, (3,)]
Mat3 = Fixed[f64, (3, 3)]


def clean(grid: Array[f64, 2], small: Array[f32, 1], values: InOut[list[f64]]) -> f64:
    row = grid[1, :].copy()
    column = grid[:, 0].copy()
    total = row.sum() + column.max()
    scaled = 2.0 * grid - grid / 3.0
    product = grid @ grid
    matrix = Mat3((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))
    turned = matrix @ Vec3(1.0, 0.0, 0.0)
    up = Vec3.zeros()
    up += turned
    lengths = grid.shape
    nx, ny = lengths
    cube = Array.zeros((nx, ny, 4), dtype=Vec3)
    cube[0, 0, 0] = up
    narrow: Array[f32, 1] = Array.zeros(10)
    narrow += small * 2
    filled = Array.fill((3,), 1.5)
    copy = Array(values)
    values.append(copy[0])
    return total + up.dot(turned) + scaled[0, 0] + product[0, 0] + filled.min() + f64(grid.size)
";
    assert_eq!(check_one(program), "");
}

/// Non-copy elements, too few indices, mixed element types, `Fixed` shapes that do not
/// broadcast, an `f32` array with an `f64` literal, and `Fixed` arrays built with the wrong
/// number of elements.
#[test]
fn array_types_and_arithmetic_mistakes_are_reported() {
    let program = r"
Vec3 = Fixed[f64, (3,)]
Vec2 = Fixed[f64, (2,)]
Mat3 = Fixed[f64, (3, 3)]


def arrays(grid: Array[f64, 2], names: Array[list[f64], 1], small: Array[f32, 1]):
    a = grid[1]
    b = grid + small
    c = small * 2.0
    d = Vec3(1.0, 2.0)
    e = Vec3(1.0, 2.0, 3.0) + Vec2(1.0, 2.0)
    f = Mat3((1.0, 0.0, 0.0), (0.0, 1.0))
";
    assert_eq!(
        check_one(program),
        r"error: an array's elements must be a copy type, found `list[f64]`
 --> main.byn:6:46
  |
6 | def arrays(grid: Array[f64, 2], names: Array[list[f64], 1], small: Array[f32, 1]):
  |                                              ^^^^^^^^^ not a copy type
  = note: a copy type is a number, `bool`, a `Fixed` array, an enum, or a struct declared `Copy`; use a `list` for other values

error: `Array[f64, 2]` takes 2 indices, found 1
 --> main.byn:7:9
  |
7 |     a = grid[1]
  |         ^^^^^^^ one index per dimension
  = note: take a row or column with a slice, such as `a[i, :]`

error: `+` cannot mix `f64` and `f32`
 --> main.byn:8:9
  |
8 |     b = grid + small
  |         ^^^^^^^^^^^^ different number types
  = note: convert one side so both are the same type, such as `f64(x)`

error: `Vec3` needs its elements in the shape `(3,)`
  --> main.byn:10:9
   |
10 |     d = Vec3(1.0, 2.0)
   |         ^^^^^^^^^^^^^^ wrong number of elements
   = note: for example `Vec3(0.0, 0.0, 0.0)`

error: `+` cannot combine `Fixed[f64, (3,)]` and `Fixed[f64, (2,)]`
  --> main.byn:11:9
   |
11 |     e = Vec3(1.0, 2.0, 3.0) + Vec2(1.0, 2.0)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ shapes do not match
   = note: two `Fixed` arrays combine when, in each dimension counted from the last, their lengths are equal or one of them is 1

error: `Mat3` needs its elements in the shape `(3, 3)`
  --> main.byn:12:9
   |
12 |     f = Mat3((1.0, 0.0, 0.0), (0.0, 1.0))
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong number of elements
   = note: write each row as a tuple: `Mat3((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))`"
    );
}

/// `dot` on a matrix, undefined methods and attributes, a float shape, a non-copy element,
/// a list item of the wrong type, and a sum of the wrong type.
#[test]
fn array_methods_and_creation_mistakes_are_reported() {
    let program = r#"
def methods(grid: Array[f64, 2], counts: InOut[list[i64]]):
    g = grid.dot(grid)
    h = grid.median()
    k = Array.zeros((2.5,))
    m = Array.fill((3,), "x")
    counts.append(1.5)
    counts.push(2)
    n = grid.length
    p: Array[f32, 1] = Array.zeros(4)
    q: f64 = p.sum()
"#;
    assert_eq!(
        check_one(program),
        r#"error: `dot` needs a vector, found `Array[f64, 2]`
 --> main.byn:2:9
  |
2 |     g = grid.dot(grid)
  |         ^^^^^^^^^^^^^^ not rank 1
  = note: for a matrix product, use `@`

error: `Array[f64, 2]` has no method `median` taking these arguments
 --> main.byn:3:9
  |
3 |     h = grid.median()
  |         ^^^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill` and `tofile`

error: a shape's lengths are integers, found `f64`
 --> main.byn:4:22
  |
4 |     k = Array.zeros((2.5,))
  |                      ^^^ not an integer
  = note: a shape is a tuple of integers written out, such as `(nx, ny)`

error: an array's elements must be a copy type, found `str`
 --> main.byn:5:26
  |
5 |     m = Array.fill((3,), "x")
  |                          ^^^ not a copy type
  = note: a copy type is a number, `bool`, a `Fixed` array, an enum, or a struct declared `Copy`; use a `list` for other values

error: expected `i64`, found `f64`
 --> main.byn:6:19
  |
6 |     counts.append(1.5)
  |                   ^^^ the list holds `i64`

error: `list[i64]` has no method `push`
 --> main.byn:7:5
  |
7 |     counts.push(2)
  |     ^^^^^^^^^^^^^^ not defined
  = note: a list has `append`, `pop` and `copy`, and works with `len`, indexing, slicing, `in` and loops

error: `Array[f64, 2]` has no attribute `length`
 --> main.byn:8:9
  |
8 |     n = grid.length
  |         ^^^^^^^^^^^ not defined
  = note: an array has `shape` and `size`

error: expected `f64`, found `f32`
  --> main.byn:10:14
   |
10 |     q: f64 = p.sum()
   |              ^^^^^^^ the annotation says `f64`"#
    );
}

/// `-a` and `+a` work element by element on an array of numbers, and are reported on an array
/// of anything else.
#[test]
fn negating_an_array_needs_number_elements() {
    let program = r"
def negate(a: Array[f64, 1], flags: Array[bool, 1]) -> Array[f64, 1]:
    b = -flags
    return -a
";
    assert_eq!(
        check_one(program),
        r"error: `-` does not apply to `Array[bool, 1]`
 --> main.byn:2:9
  |
2 |     b = -flags
  |         ^^^^^^ its elements are not numbers"
    );
}

/// Assigning into a slice takes an array of its rank or a number for every element, and reads
/// a slice on the right in place without keeping it; a number of the wrong type is an error
/// that says what each element holds.
#[test]
fn slices_take_arrays_and_numbers() {
    let program = r#"
def fill(a: InOut[Array[f64, 1]], grid: InOut[Array[f64, 2]], n: i64):
    a[1:n] = 0.0
    a[1:n] = a[0:n - 1]
    grid[:, 0] = 1
    grid[0, :] += a[0:3]
    a[0:2] = "no"
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `f64`, found `str`
 --> main.byn:6:14
  |
6 |     a[0:2] = "no"
  |              ^^^^ each element of the slice holds `f64`"#
    );
}

/// Broadcasting (roadmap 3.9): arrays of different ranks combine into the higher rank, `Fixed`
/// shapes broadcast when compiling, a slice takes an array of lower rank, which it copies rather
/// than moves, and `+=` keeps its target's rank. `Fixed` shapes that do not broadcast, an `Array` mixed with a `Fixed` array,
/// and `+=` that would raise the target's rank are errors.
#[test]
fn arrays_broadcast_by_rank_and_fixed_shape() {
    let clean = r"
Row = Fixed[f64, (3,)]
Column = Fixed[f64, (2, 1)]
Grid = Fixed[f64, (2, 3)]


def clean(grid: Array[f64, 2], row: Array[f64, 1]) -> Array[f64, 2]:
    shifted = grid - row
    shifted += row * 2.0
    shifted[:, :] = row
    table: Grid = Column((1.0,), (2.0,)) + Row(1.0, 2.0, 3.0)
    return shifted + table[0, 0]
";
    assert_eq!(check_one(clean), "");
    let mistakes = r"
Row = Fixed[f64, (3,)]
Pair = Fixed[f64, (2,)]


def mistakes(grid: Array[f64, 2], row: InOut[Array[f64, 1]]):
    a = Row.zeros() + Pair.zeros()
    b = grid + Row.zeros()
    row += grid
";
    assert_eq!(
        check_one(mistakes),
        r"error: `+` cannot combine `Fixed[f64, (3,)]` and `Fixed[f64, (2,)]`
 --> main.byn:6:9
  |
6 |     a = Row.zeros() + Pair.zeros()
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^ shapes do not match
  = note: two `Fixed` arrays combine when, in each dimension counted from the last, their lengths are equal or one of them is 1

error: `+` cannot combine `Array[f64, 2]` and `Fixed[f64, (3,)]`
 --> main.byn:7:9
  |
7 |     b = grid + Row.zeros()
  |         ^^^^^^^^^^^^^^^^^^ shapes do not match
  = note: an `Array` and a `Fixed` array do not combine

error: `+=` gives `Array[f64, 2]`, but the target has type `Array[f64, 1]`
 --> main.byn:8:5
  |
8 |     row += grid
  |     ^^^^^^^^^^^ would change the target's type
  = note: a variable keeps one type for its whole scope; convert the value, or use a new name"
    );
}

/// `None` in a subscript adds a dimension of length 1 to the view: `x[:, None]` is an
/// `Array[f64, 2]`, and still takes one index or slice per dimension of `x`. On a list it is an
/// error.
#[test]
fn none_adds_an_axis_to_an_array() {
    let clean = r"
def clean(x: Array[f64, 1]) -> Array[f64, 2]:
    return x[:, None] - x[None, :]
";
    assert_eq!(check_one(clean), "");
    let mistakes = r"
def mistakes(x: Array[f64, 1], values: list[f64]):
    a = x[None]
    b = values[None]
";
    assert_eq!(
        check_one(mistakes),
        r"error: `Array[f64, 1]` takes 1 index, found 0
 --> main.byn:2:9
  |
2 |     a = x[None]
  |         ^^^^^^^ one index per dimension
  = note: `None` adds a dimension and takes none: write `:` for each of the array's, such as `x[None, :]`

error: `None` cannot index a value of type `list[f64]`
 --> main.byn:3:9
  |
3 |     b = values[None]
  |         ^^^^^^^^^^^^ not an array
  = note: `None` in a subscript adds a dimension of length 1 to an array"
    );
}

/// `sum`, `min` and `max` take one constant axis, positional or `axis=`, counted from the
/// last when negative, and give the array without that axis, or one number from a vector. An
/// axis outside the array, one that is not a constant, or any other argument is an error.
#[test]
fn reductions_take_one_constant_axis() {
    let clean = r"
LAST: Final = -1


def clean(a: Array[f64, 2], v: Array[f64, 1], m: Fixed[i64, (2, 3)]):
    rows: Array[f64, 1] = a.sum(axis=1)
    columns: Array[f64, 1] = a.max(0)
    last: Array[f64, 1] = a.min(axis=LAST)
    total: f64 = v.sum(axis=-1)
    fixed: Fixed[i64, (3,)] = m.sum(axis=0)
";
    assert_eq!(check_one(clean), "");
    let mistakes = r"
def mistakes(a: Array[f64, 2], k: i64):
    b = a.sum(axis=2)
    c = a.max(axis=k)
    d = a.min(keepdims=1)
    e = a.sum(0, 1)
";
    assert_eq!(
        check_one(mistakes),
        r"error: axis 2 is out of range for `Array[f64, 2]`, which has 2 axes
 --> main.byn:2:20
  |
2 |     b = a.sum(axis=2)
  |                    ^ no such axis
  = note: an array of rank 2 has axes 0 to 1, or -2 to -1 counted from the last

error: `k` is not a constant
 --> main.byn:3:20
  |
3 |     c = a.max(axis=k)
  |                    ^ used in a constant expression

error: `min` takes no arguments or one axis
 --> main.byn:4:9
  |
4 |     d = a.min(keepdims=1)
  |         ^^^^^^^^^^^^^^^^^ wrong arguments
  = note: the axis is a constant whole number, such as `a.sum(axis=0)` or `a.max(axis=-1)` for the last

error: `sum` takes no arguments or one axis
 --> main.byn:5:9
  |
5 |     e = a.sum(0, 1)
  |         ^^^^^^^^^^^ wrong arguments
  = note: the axis is a constant whole number, such as `a.sum(axis=0)` or `a.max(axis=-1)` for the last"
    );
}

/// `mean`, `var` and `std` (roadmap 3.16): the mean of floats keeps their type and the mean of
/// integers is an `f64`, `ddof` is an `int` keyword, and an axis, a positional argument, or
/// `var` of integers is an error that says what is built.
#[test]
fn statistics_methods_check_their_arguments() {
    let clean = r"
def clean(a: Array[f64, 2], s: Array[f32, 1], n: Array[i64, 1], v: Fixed[f64, (3,)]):
    m: f64 = a.mean()
    small: f32 = s.var(ddof=1)
    counted: f64 = n.mean()
    spread: f64 = v.std()
    sample: f64 = a.std(ddof=len(n))
";
    assert_eq!(check_one(clean), "");
    let mistakes = r"
def mistakes(a: Array[f64, 2], n: Array[i64, 1], z: Array[complex128, 1]):
    b = a.mean(axis=0)
    c = a.var(1)
    d = n.var()
    e = z.mean()
    f = a.std(ddof=0.5)
";
    assert_eq!(
        check_one(mistakes),
        r"error: `mean` takes no arguments
 --> main.byn:2:9
  |
2 |     b = a.mean(axis=0)
  |         ^^^^^^^^^^^^^^ wrong arguments
  = note: write `a.mean()` for the whole array; along an axis is not built yet

error: `var` takes no arguments or `ddof`
 --> main.byn:3:9
  |
3 |     c = a.var(1)
  |         ^^^^^^^^ wrong arguments
  = note: write `a.var()` for the whole array, or `ddof=1` to divide by n - 1; along an axis is not built yet

error: `var` of `Array[i64, 1]` is not built
 --> main.byn:4:9
  |
4 |     d = n.var()
  |         ^^^^^^^ not floats
  = note: `var` and `std` take an array of floats

error: `mean` of `Array[complex128, 1]` is not built
 --> main.byn:5:9
  |
5 |     e = z.mean()
  |         ^^^^^^^^ not real numbers
  = note: `mean` takes an array of integers or floats

error: expected `i64`, found `f64`
 --> main.byn:6:20
  |
6 |     f = a.std(ddof=0.5)
  |                    ^^^ `ddof` is `i64`"
    );
}
