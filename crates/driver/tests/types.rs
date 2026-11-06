//! Type checking, end to end: programs go through the parser, the syntax check, the resolver
//! and the checker's declarations, and the rendered diagnostics are compared in full.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks a package of `(path, text)` files and returns every diagnostic, rendered and joined
/// by blank lines. An empty string means the package is clean. A file that cannot be added
/// returns the reason instead, which fails the test's comparison.
fn check(files: &[(&str, &str)]) -> String {
    let mut session = Session::new();
    for (path, text) in files {
        if let Err(error) = session.add_file(*path, *text) {
            return error.to_string();
        }
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Checks a package of one file, `main.npl`. The program's first newline, which only lets a
/// test start it on its own line, is dropped.
fn check_one(text: &str) -> String {
    check(&[("main.npl", text.strip_prefix('\n').unwrap_or(text))])
}

/// The example programs in `docs/examples` declare everything correctly.
#[test]
fn the_example_programs_check_clean() {
    let examples = [
        (
            "conjugate_gradient.npl",
            include_str!("../../../docs/examples/conjugate_gradient.npl"),
        ),
        (
            "heat_equation.npl",
            include_str!("../../../docs/examples/heat_equation.npl"),
        ),
        (
            "lennard_jones.npl",
            include_str!("../../../docs/examples/lennard_jones.npl"),
        ),
        (
            "lorenz_rk4.npl",
            include_str!("../../../docs/examples/lorenz_rk4.npl"),
        ),
        (
            "monte_carlo_pi.npl",
            include_str!("../../../docs/examples/monte_carlo_pi.npl"),
        ),
    ];
    assert_eq!(check(&examples), "");
}

/// Types from another module, by import and through the module's name, generic structs, a
/// shape built from constants, and every builtin type form.
#[test]
fn valid_declarations_check_clean() {
    let physics = r"
N: Final = 3
Vec3 = Fixed[f64, (N,)]


class Body(Struct, Copy):
    position: Vec3
    mass: f64


class Pair[T](Struct):
    first: T
    second: T

    def swap(self: InOut[Self]):
        pass

    def into_first(self: Owned[Self]) -> T:
        return self.first


class SolverError(Exception):
    iterations: int
";
    let main = r#"
import physics
from physics import Body, Pair, SolverError, Vec3

Matrix = Fixed[f64, (physics.N, physics.N)]
SCALE: Final[f32] = 2


def solve(
    bodies: InOut[Array[Body, 1]],
    pairs: list[Pair[f64]],
    matrix: Matrix,
    step: Callable[[f64, Vec3], Vec3],
    last: physics.Body | None,
    sizes: tuple[i64, u8, complex],
) -> Raises[str, SolverError]:
    return "done"


def first[T](values: Owned[list[T]]) -> T:
    return values[0]
"#;
    let strip = |text: &'static str| text.strip_prefix('\n').unwrap_or(text);
    assert_eq!(
        check(&[("physics.npl", strip(physics)), ("main.npl", strip(main))]),
        ""
    );
}

/// A constant out of its type's range, of the wrong kind, dividing by zero, overflowing, or
/// calling a function.
#[test]
fn a_constant_the_compiler_cannot_compute_is_reported() {
    let program = r#"
SMALL: Final[u8] = 300
HALF: Final[i32] = 0.5
ZERO: Final = 1 // 0
BIG: Final = 2**64
COUNT: Final = len("abc")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `300` does not fit in `u8`
 --> main.npl:1:20
  |
1 | SMALL: Final[u8] = 300
  |                    ^^^ out of range

error: this value is not of type `i32`
 --> main.npl:2:20
  |
2 | HALF: Final[i32] = 0.5
  |                    ^^^ wrong type for the constant

error: division by zero in a constant
 --> main.npl:3:15
  |
3 | ZERO: Final = 1 // 0
  |               ^^^^^^ divides by zero

error: this constant does not fit in `i64`
 --> main.npl:4:14
  |
4 | BIG: Final = 2**64
  |              ^^^^^ overflows
  = note: integers are 64-bit; use a float constant for larger numbers

error: the compiler cannot compute this value
 --> main.npl:5:16
  |
5 | COUNT: Final = len("abc")
  |                ^^^^^^^^^^ not a constant expression
  = note: a constant is built from number literals, other constants and arithmetic"#
    );
}

/// Constants and aliases that need their own value: each one in the cycle is reported once.
#[test]
fn a_definition_in_terms_of_itself_is_reported() {
    let program = r"
A: Final = B + 1
B: Final = A * 2
Nested = list[Nested]
";
    assert_eq!(
        check_one(program),
        r"error: `A` is defined in terms of itself
 --> main.npl:1:1
  |
1 | A: Final = B + 1
  | ^ refers back to itself
  = note: an alias or constant cannot depend on its own value

error: `B` is defined in terms of itself
 --> main.npl:2:1
  |
2 | B: Final = A * 2
  | ^ refers back to itself
  = note: an alias or constant cannot depend on its own value

error: `Nested` is defined in terms of itself
 --> main.npl:3:1
  |
3 | Nested = list[Nested]
  | ^^^^^^ refers back to itself
  = note: an alias or constant cannot depend on its own value"
    );
}

/// Annotations that mean no type: a missing argument, a missing annotation, a union other
/// than `T | None`, `InOut` inside another type, and `Self` outside a class.
#[test]
fn annotations_that_mean_no_type_are_reported() {
    let program = r"
def solve(grid: Array[f64], steps, scale: f64 | i64, out: list[InOut[f64]]) -> Self:
    pass
";
    assert_eq!(
        check_one(program),
        r"error: `Array` takes 2 type arguments, found 1
 --> main.npl:1:17
  |
1 | def solve(grid: Array[f64], steps, scale: f64 | i64, out: list[InOut[f64]]) -> Self:
  |                 ^^^^^^^^^^ wrong number of arguments
  = note: for example `Array[f64, 1]`

error: parameter `steps` needs a type
 --> main.npl:1:29
  |
1 | def solve(grid: Array[f64], steps, scale: f64 | i64, out: list[InOut[f64]]) -> Self:
  |                             ^^^^^ no type annotation
  = note: every parameter's type is written: `steps: f64`

error: a union type can only add `None` to a type
 --> main.npl:1:43
  |
1 | def solve(grid: Array[f64], steps, scale: f64 | i64, out: list[InOut[f64]]) -> Self:
  |                                           ^^^^^^^^^ only `T | None` is a type
  = note: a value that may be one of several types is not supported; use separate functions, or an enum for a fixed set of choices

error: `InOut` can only wrap the type of a parameter
 --> main.npl:1:64
  |
1 | def solve(grid: Array[f64], steps, scale: f64 | i64, out: list[InOut[f64]]) -> Self:
  |                                                                ^^^^^^^^^^ not a parameter's type
  = note: it says how an argument is passed: `def f(out: InOut[Array[f64, 1]])`

error: `Self` is only a type inside a class
 --> main.npl:1:80
  |
1 | def solve(grid: Array[f64], steps, scale: f64 | i64, out: list[InOut[f64]]) -> Self:
  |                                                                                ^^^^ outside any class"
    );
}

/// A struct field with a default, an assignment that is not a field, an annotated enum member,
/// and an enum used as an error type.
#[test]
fn struct_and_enum_bodies_are_checked() {
    let program = r"
class Particle(Struct):
    mass: f64 = 1.0
    charge = 0.0


class Boundary(Enum):
    PERIODIC = 1
    REFLECTING: int = 2


def fail() -> Raises[None, Boundary]:
    pass
";
    assert_eq!(
        check_one(program),
        r"error: a struct field has no default value
 --> main.npl:2:17
  |
2 |     mass: f64 = 1.0
  |                 ^^^ remove this default
  = note: every field is given when a struct is built: `Particle(position, velocity, 1.0)`

error: a struct field needs a type, and no value
 --> main.npl:3:5
  |
3 |     charge = 0.0
  |     ^^^^^^^^^^^^ not a field
  = note: declare a field with its type: `mass: f64`

error: an enum member has a value, not a type
 --> main.npl:8:5
  |
8 |     REFLECTING: int = 2
  |     ^^^^^^^^^^^^^^^^^^^ not an enum member
  = note: write a member as `PERIODIC = 1`

error: `Boundary` is not an error type
  --> main.npl:11:28
   |
11 | def fail() -> Raises[None, Boundary]:
   |                            ^^^^^^^^ expected an error type
   = note: an error type is a class with the base `Exception`"
    );
}

/// A `Fixed` shape that is not a tuple, and an `Array` rank of zero.
#[test]
fn shapes_and_ranks_are_checked() {
    let program = r"
Vec3 = Fixed[f64, (3, 3, 3)]
Wrong = Fixed[f64, 3]
Flat = Array[f64, 0]
";
    assert_eq!(
        check_one(program),
        r"error: expected a shape
 --> main.npl:2:20
  |
2 | Wrong = Fixed[f64, 3]
  |                    ^ not a tuple
  = note: a shape is a tuple of lengths, such as `(3,)` or `(3, 3)`

error: an array's rank is a whole number of dimensions, at least 1
 --> main.npl:3:19
  |
3 | Flat = Array[f64, 0]
  |                   ^ not a valid rank
  = note: for example `Array[f64, 1]` for a vector or `Array[f64, 2]` for a grid"
    );
}

/// A struct whose fields leave padding gets one warning at its name, naming the order that takes
/// the fewest bytes; ties keep the order written. Error types are structs too.
#[test]
fn padding_that_reordering_saves_is_reported() {
    let program = r"
class Particle(Struct):
    alive: bool
    mass: f64
    charge: f32
    species: u8


class SolverError(Exception):
    converged: bool
    residual: f64
    diverged: bool
";
    assert_eq!(
        check_one(program),
        r#"warning: reordering the fields of `Particle` would save 8 bytes
 --> main.npl:1:7
  |
1 | class Particle(Struct):
  |       ^^^^^^^^ 24 bytes as declared, 16 reordered
  = note: declare the fields in the order `mass, charge, alive, species`; the compiler keeps the order written, so the layout matches C and `numpy`
  = note: if the order is fixed by a file format or a C library, decorate the struct `@layout("C")` to keep it without this warning

warning: reordering the fields of `SolverError` would save 8 bytes
 --> main.npl:8:7
  |
8 | class SolverError(Exception):
  |       ^^^^^^^^^^^ 24 bytes as declared, 16 reordered
  = note: declare the fields in the order `residual, converged, diverged`; the compiler keeps the order written, so the layout matches C and `numpy`
  = note: if the order is fixed by a file format or a C library, decorate the struct `@layout("C")` to keep it without this warning"#
    );
}

/// Field sizes come from the classes the fields name, in this module or another: an enum is a
/// one-byte tag and `Pair[f64]` is two `f64`s, its type argument substituted.
#[test]
fn padding_counts_fields_of_other_classes() {
    let shapes = r"
class Boundary(Enum):
    PERIODIC = 1
    REFLECTING = 2


class Pair[T](Struct):
    first: T
    second: T
";
    let main = r"
from shapes import Boundary, Pair


class Cell(Struct):
    boundary: Boundary
    extent: Pair[f64]
    count: i32
";
    assert_eq!(
        check(&[("shapes.npl", shapes), ("main.npl", main)]),
        r#"warning: reordering the fields of `Cell` would save 8 bytes
 --> main.npl:5:7
  |
5 | class Cell(Struct):
  |       ^^^^ 32 bytes as declared, 24 reordered
  = note: declare the fields in the order `extent, count, boundary`; the compiler keeps the order written, so the layout matches C and `numpy`
  = note: if the order is fixed by a file format or a C library, decorate the struct `@layout("C")` to keep it without this warning"#
    );
}

/// No warning for a struct already in its smallest order, a generic struct, which has no
/// layout until it is instantiated, or a struct whose field type is an error reported already.
#[test]
fn padding_is_not_reported_when_nothing_can_be_saved() {
    let program = r"
class Body(Struct, Copy):
    position: Fixed[f64, (3,)]
    mass: f32
    alive: bool


class Wrapper[T](Struct):
    alive: bool
    value: T


class Broken(Struct):
    alive: bool
    mass: Array[f64]
    charge: f64
";
    assert_eq!(
        check_one(program),
        r"error: `Array` takes 2 type arguments, found 1
  --> main.npl:14:11
   |
14 |     mass: Array[f64]
   |           ^^^^^^^^^^ wrong number of arguments
   = note: for example `Array[f64, 1]`"
    );
}

/// A struct decorated `@layout("C")` keeps its padding without a warning (decision 0017); a
/// struct without it still gets one.
#[test]
fn padding_is_not_reported_for_a_pinned_layout() {
    let program = r#"
@layout("C")
class Header(Struct, Copy):
    version: u8
    length: u64
    flags: u8


class Sample(Struct):
    valid: bool
    value: f64
    count: i32
"#;
    assert_eq!(
        check_one(program),
        r#"warning: reordering the fields of `Sample` would save 8 bytes
 --> main.npl:8:7
  |
8 | class Sample(Struct):
  |       ^^^^^^ 24 bytes as declared, 16 reordered
  = note: declare the fields in the order `value, count, valid`; the compiler keeps the order written, so the layout matches C and `numpy`
  = note: if the order is fixed by a file format or a C library, decorate the struct `@layout("C")` to keep it without this warning"#
    );
}
