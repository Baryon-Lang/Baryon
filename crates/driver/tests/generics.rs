//! Generics, end to end: type arguments inferred at calls, bounds checked against them, and
//! the operations a bound allows inside a generic body, through every stage up to the type
//! checker.
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

/// A `Number` function with literals and ordering, a `VectorSpace` step used with `f64` and
/// `Fixed` states, a `Float` function, and a generic struct built and read back.
#[test]
fn valid_generic_code_checks_clean() {
    let program = r"
Vec3 = Fixed[f64, (3,)]


class Pair[T: Copy](Struct):
    first: T
    second: T


def largest[T: Number](a: T, b: T) -> T:
    if a > b:
        return a
    return b + 0


def half[T: Float](x: T) -> T:
    return x * 0.5 - -x


def step[S: VectorSpace](state: S, rate: S, h: f64) -> S:
    return state + h / 2 * rate - 2 * rate * h + -rate


def first_of[T: Copy](pair: Pair[T]) -> T:
    return pair.first


def use_them(small: f32, n: i32, position: Vec3, velocity: Vec3) -> f64:
    wide: f32 = largest(small, 0)
    count: i32 = largest(n, 3)
    narrow: f32 = half(small)
    moved: Vec3 = step(position, velocity, 0.1)
    pair = Pair(1.5, 2.5)
    value: f64 = first_of(pair)
    return value + step(1.0, 2.0, 0.1) + f64(wide) + f64(count) + f64(narrow) + moved[0]
";
    assert_eq!(check_one(program), "");
}

/// A type argument that breaks its bound, two arguments that disagree about `T`, arithmetic
/// and ordering the bound does not allow, a float literal under `Number`, and a bound that is
/// not a trait.
#[test]
fn generic_mistakes_are_reported() {
    let program = r"
def largest[T: Number](a: T, b: T) -> T:
    if a > b:
        return a
    return b * 0.5


def keep[T: Copy](value: T, other: T) -> T:
    if value < other:
        return value + other
    return value


class Real(Struct):
    value: f64


def odd[T: Real](value: T) -> T:
    return value


def use_them(flag: bool, x: f64, n: i64) -> f64:
    largest(flag, flag)
    return largest(x, n)
";
    assert_eq!(
        check_one(program),
        "\
error: a number literal cannot have the type `T`
 --> main.byn:4:16
  |
4 |     return b * 0.5
  |                ^^^ not allowed by the bound
  = note: `T` may be an integer type; bound it by `Float` to use this literal

error: `<` cannot order `T` values
 --> main.byn:8:13
  |
8 |     if value < other:
  |             ^^^^^^^^ no order
  = note: bound the type parameter by `Number` or `Float`: `[T: Number]`

error: `+` is not allowed between `T` and `T`
 --> main.byn:9:16
  |
9 |         return value + other
  |                ^^^^^^^^^^^^^ the type parameter's bound does not allow it
  = note: a `Number` or `Float` type parameter combines with itself and literals; a `VectorSpace` one adds to itself and scales by an `f64`

error: `Real` is not a trait
  --> main.byn:17:12
   |
17 | def odd[T: Real](value: T) -> T:
   |            ^^^^ expected a trait
   = note: a type parameter is bounded by `Number`, `Float`, `VectorSpace` or `Copy`

error: `largest` needs `T` to be a `Number`, found `bool`
  --> main.byn:22:5
   |
22 |     largest(flag, flag)
   |     ^^^^^^^^^^^^^^^^^^^ bound not satisfied

error: `b` has type `f64` here, found `i64`
  --> main.byn:23:23
   |
23 |     return largest(x, n)
   |                       ^ type `i64`"
    );
}

/// A method of a generic class compiles once per type its class is used at, its symbol naming
/// the class's type argument. Before roadmap 1.9.18 it was reported as not compiled yet.
#[test]
fn methods_of_generic_classes_compile_per_instance() {
    let program = "
class Pair[T: Number](Struct, Copy):
    a: T
    b: T

    def total(self) -> T:
        return self.a + self.b


def main():
    pair = Pair(1.5, 2.5)
    print(pair.total())
";
    let mut session = Session::new();
    session
        .add_file("main.byn", program.strip_prefix('\n').unwrap())
        .unwrap();
    assert_eq!(check_one(program), "");
    let module = session.compile(None, driver::Profile::Debug).unwrap();
    assert!(module.contains("@\"main.Pair.total[f64]\""), "{module}");
}
