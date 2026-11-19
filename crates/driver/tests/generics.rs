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
   = note: a type parameter is bounded by `Number`, `Float`, `VectorSpace`, `Copy`, or a trait declared as `class Body(Protocol)`

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

/// An argument that cannot fix a type parameter because it has the wrong shape is reported at
/// the call (roadmap 1.9.27). Before, the unfixed `T` became unknown, matched anything, and the
/// call reached the backend: "the LLVM backend does not compile values of type `unknown` yet".
/// A `None` for a `T | None` still fits.
#[test]
fn an_argument_of_the_wrong_shape_is_reported_at_the_call() {
    let program = r"
def first[T: Number](values: Array[T, 1]) -> T:
    return values[0]


def maybe[T](value: T | None) -> bool:
    return value is None


def use_them() -> f64:
    maybe(None)
    return first([1.0])
";
    assert_eq!(
        check_one(program),
        "\
error: `values` has type `Array[T, 1]` here, found `list[f64]`
  --> main.byn:11:18
   |
11 |     return first([1.0])
   |                  ^^^^^ type `list[f64]`"
    );
}

/// A generic struct's method called through its class with no type arguments written finds
/// them from its arguments, as the constructor does (roadmap 1.9.30). A class type parameter
/// no argument fixes is reported at the call, which asks for it to be written on the class;
/// written there, the call checks. A wrong argument is reported against the type found.
#[test]
fn generic_methods_called_through_their_class_find_their_type_arguments() {
    let program = r"
class Bag[T](Struct):
    items: list[T]

    def of(item: Owned[T]) -> Bag[T]:
        return Bag([item])

    def empty() -> Bag[T]:
        items: list[T] = []
        return Bag(items)

    def count(self) -> int:
        return len(self.items)


def use_them() -> int:
    a = Bag.empty()
    b = Bag[f64].empty()
    c = Bag.of(1.5)
    return Bag.count(c) + Bag[i64].count(c)
";
    assert_eq!(
        check_one(program),
        "\
error: `Bag.empty` does not say what `T` is from its arguments
  --> main.byn:16:9
   |
16 |     a = Bag.empty()
   |         ^^^^^^^^^^^ type argument not found
   = note: write it on the class, for example `Bag[f64].empty(...)`

error: `self` has type `Bag[i64]`, found `Bag[f64]`
  --> main.byn:19:42
   |
19 |     return Bag.count(c) + Bag[i64].count(c)
   |                                          ^ type `Bag[f64]`"
    );
}

/// Calls the checker cannot type are reported where they are written (roadmap 1.9.48 and
/// 1.9.49): a type parameter called as a conversion under a bound that allows none, a generic struct built with its type
/// arguments written, a method on a type parameter with no trait that has it, and a generic
/// function whose arguments do not fix its type parameter. Before, each got an unknown type
/// with no error, and the build stopped on "lowering does not handle this call yet" or "the
/// LLVM backend does not compile values of type `unknown` yet".
#[test]
fn calls_the_checker_cannot_type_are_reported() {
    let program = r"
class Box[T](Struct):
    items: list[T]


def twice[T: VectorSpace](xs: list[T]) -> T:
    s = xs[0] + xs[0]
    return s * T(len(xs))


def last[T](xs: list[T]) -> T:
    return xs[len(xs) - 1].copy()


def make[T]() -> list[T]:
    out: list[T] = []
    return out


def use_them() -> int:
    b = Box[f64]([1.0])
    xs = make()
    return len(b.items) + len(xs)
";
    assert_eq!(
        check_one(program),
        "\
error: `T` is a type parameter, which cannot be called
 --> main.byn:7:16
  |
7 |     return s * T(len(xs))
  |                ^^^^^^^^^^ not a function
  = note: a type parameter converts a number only under `Number` or `Float`, as `T(n)`; a \
conversion otherwise names a number type, such as `f64(n)`

error: `T` has no method `copy`
  --> main.byn:11:12
   |
11 |     return xs[len(xs) - 1].copy()
   |            ^^^^^^^^^^^^^^^^^^^^ not known for a type parameter
   = note: a type parameter has only what its bound allows, and a trait has methods only: bound \
it by a trait that lists the method

error: `Box` is built without its type arguments written
  --> main.byn:20:9
   |
20 |     b = Box[f64]([1.0])
   |         ^^^^^^^^^^^^^^^ type arguments written
   = note: the arguments say what they are, `Box(...)`; give an empty list a typed variable \
first, `items: list[f64] = []`

error: `make` does not say what `T` is from its arguments
  --> main.byn:21:10
   |
21 |     xs = make()
   |          ^^^^^^ type argument not found
   = note: a type argument is never written at a call; give the function a parameter whose \
argument fixes `T`"
    );
}

/// `T(x)` under `Number` or `Float` converts one real number (decision 0066, roadmap 1.9.59):
/// a complex value, a `bool` or two values are errors at the call, as each is for `f64(...)`.
#[test]
fn a_conversion_to_a_type_parameter_takes_one_real_number() {
    let program = r"
def wrong[T: Number](x: T, z: complex128, flag: bool) -> T:
    a = T(z)
    b = T(flag)
    return x + T(1, 2)
";
    assert_eq!(
        check_one(program),
        "\
error: `T(...)` converts a real number, found `complex128`
 --> main.byn:2:11
  |
2 |     a = T(z)
  |           ^ not a real number
  = note: an integer or a float of any width converts to a type parameter

error: `T(...)` converts a real number, found `bool`
 --> main.byn:3:11
  |
3 |     b = T(flag)
  |           ^^^^ not a real number
  = note: an integer or a float of any width converts to a type parameter

error: `T(...)` converts one value, found 2
 --> main.byn:4:16
  |
4 |     return x + T(1, 2)
  |                ^^^^^^^ wrong number of arguments"
    );
}
