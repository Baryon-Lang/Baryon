//! Arrays, end to end: creating them, indexing and slicing, methods and attributes,
//! whole-array arithmetic and the matrix product, through every stage up to the type checker.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.npl` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.npl", text.strip_prefix('\n').unwrap_or(text)) {
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

/// Non-copy elements, too few indices, mismatched ranks and shapes, an `f32` array with an
/// `f64` literal, and `Fixed` arrays built with the wrong number of elements.
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
 --> main.npl:6:46
  |
6 | def arrays(grid: Array[f64, 2], names: Array[list[f64], 1], small: Array[f32, 1]):
  |                                              ^^^^^^^^^ not a copy type
  = note: a copy type is a number, `bool`, a `Fixed` array, an enum, or a struct declared `Copy`; use a `list` for other values

error: `Array[f64, 2]` takes 2 indices, found 1
 --> main.npl:7:9
  |
7 |     a = grid[1]
  |         ^^^^^^^ one index per dimension
  = note: take a row or column with a slice, such as `a[i, :]`

error: `+` cannot combine `Array[f64, 2]` and `Array[f32, 1]`
 --> main.npl:8:9
  |
8 |     b = grid + small
  |         ^^^^^^^^^^^^ shapes do not match
  = note: both arrays need the same rank, or the same shape for `Fixed` arrays, or one side a single number

error: `Vec3` needs its elements in the shape `(3,)`
  --> main.npl:10:9
   |
10 |     d = Vec3(1.0, 2.0)
   |         ^^^^^^^^^^^^^^ wrong number of elements
   = note: for example `Vec3(0.0, 0.0, 0.0)`

error: `+` cannot combine `Fixed[f64, (3,)]` and `Fixed[f64, (2,)]`
  --> main.npl:11:9
   |
11 |     e = Vec3(1.0, 2.0, 3.0) + Vec2(1.0, 2.0)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ shapes do not match
   = note: both arrays need the same rank, or the same shape for `Fixed` arrays, or one side a single number

error: `Mat3` needs its elements in the shape `(3, 3)`
  --> main.npl:12:9
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
    h = grid.mean()
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
 --> main.npl:2:9
  |
2 |     g = grid.dot(grid)
  |         ^^^^^^^^^^^^^^ not rank 1
  = note: for a matrix product, use `@`

error: `Array[f64, 2]` has no method `mean` taking these arguments
 --> main.npl:3:9
  |
3 |     h = grid.mean()
  |         ^^^^^^^^^^^ not defined
  = note: version 0 defines `copy`, `sum`, `min`, `max`, `dot`, `round` and `fill`

error: a shape's lengths are integers, found `f64`
 --> main.npl:4:22
  |
4 |     k = Array.zeros((2.5,))
  |                      ^^^ not an integer
  = note: a shape is a tuple of integers written out, such as `(nx, ny)`

error: an array's elements must be a copy type, found `str`
 --> main.npl:5:26
  |
5 |     m = Array.fill((3,), "x")
  |                          ^^^ not a copy type
  = note: a copy type is a number, `bool`, a `Fixed` array, an enum, or a struct declared `Copy`; use a `list` for other values

error: expected `i64`, found `f64`
 --> main.npl:6:19
  |
6 |     counts.append(1.5)
  |                   ^^^ the list holds `i64`

error: `list[i64]` has no method `push`
 --> main.npl:7:5
  |
7 |     counts.push(2)
  |     ^^^^^^^^^^^^^^ not defined
  = note: a list has `append`, `pop` and `copy`, and works with `len`, indexing, slicing, `in` and loops

error: `Array[f64, 2]` has no attribute `length`
 --> main.npl:8:9
  |
8 |     n = grid.length
  |         ^^^^^^^^^^^ not defined
  = note: an array has `shape` and `size`

error: expected `f64`, found `f32`
  --> main.npl:10:14
   |
10 |     q: f64 = p.sum()
   |              ^^^^^^^ the annotation says `f64`"#
    );
}
