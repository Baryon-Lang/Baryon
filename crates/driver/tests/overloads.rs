//! Overloaded library functions (decision 0067), end to end: programs call `special.loggamma`,
//! whose real and complex forms share the name, and the rendered diagnostics are compared in
//! full. A library file of the test's own, under `<std>/`, holds the overloads the library does
//! not have yet: a generic one beside concrete ones.
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

/// Checks a package of one file, `main.byn`. The program's first newline, which only lets a
/// test start it on its own line, is dropped.
fn check_one(text: &str) -> String {
    check(&[("main.byn", text.strip_prefix('\n').unwrap_or(text))])
}

#[test]
fn a_call_takes_the_overload_its_arguments_fit() {
    let program = r"
from special import loggamma


def main():
    real: f64 = loggamma(2.5)
    whole: f64 = loggamma(3)
    z: complex128 = loggamma(complex(1.0, 2.0))
    print(real, whole, z)
";
    assert_eq!(check_one(program), "");
}

#[test]
fn a_call_no_overload_fits_lists_the_overloads() {
    let program = r#"
from special import loggamma


def main():
    print(loggamma("three"))
"#;
    assert_eq!(
        check_one(program),
        "\
error: no overload of `loggamma` takes these arguments
 --> main.byn:5:11
  |
5 |     print(loggamma(\"three\"))
  |           ^^^^^^^^^^^^^^^^^ no overload fits
  = note: the overloads are `def loggamma(x: f64) -> f64`, `def loggamma(z: complex128) -> complex128`"
    );
}

#[test]
fn an_overloaded_name_as_a_value_takes_the_callable_it_fills() {
    let program = r"
import special


def twice(f: Callable[[complex128], complex128], z: complex128) -> complex128:
    return f(f(z))


def main():
    print(twice(special.loggamma, complex(3.0, 1.0)))
";
    assert_eq!(check_one(program), "");
}

#[test]
fn an_overloaded_name_needs_a_callable_to_be_a_value() {
    let program = r"
from special import loggamma


def main():
    f = loggamma
";
    assert_eq!(
        check_one(program),
        "\
error: `loggamma` has overloads, so it is a value only where a `Callable` is expected
 --> main.byn:5:9
  |
5 |     f = loggamma
  |         ^^^^^^^^ an overloaded function
  = note: the overloads are `def loggamma(x: f64) -> f64`, `def loggamma(z: complex128) -> complex128`"
    );
}

#[test]
fn a_user_module_cannot_define_a_name_twice() {
    let program = r"
def half(x: f64) -> f64:
    return 0.5 * x


def half(x: complex128) -> complex128:
    return 0.5 * x
";
    assert_eq!(
        check_one(program),
        "\
error: `half` is defined twice in this module
 --> main.byn:5:5
  |
5 | def half(x: complex128) -> complex128:
  |     ^^^^ defined again here
 ::: main.byn:1:5
  |
1 | def half(x: f64) -> f64:
  |     ---- first defined here
  = note: one name means one thing in a module; rename one of them"
    );
}

#[test]
fn a_generic_overload_must_exclude_the_concrete_ones() {
    let library = r"
def scale(x: complex128) -> complex128:
    return x


def scale[T: Number](x: T) -> T:
    return x


def shift(x: complex128) -> complex128:
    return x


def shift[T: Float](x: T) -> T:
    return x
";
    let program = r"
def main():
    pass
";
    assert_eq!(
        check(&[("<std>/overloads_test.byn", library), ("main.byn", program)]),
        "\
error: this generic overload also takes the arguments of `def scale(x: complex128) -> complex128`
 --> <std>/overloads_test.byn:6:5
  |
6 | def scale[T: Number](x: T) -> T:
  |     ^^^^^ overlaps a concrete overload
  = note: a generic overload's bound must exclude every concrete overload's types"
    );
}

/// Two generic overloads of one name are an error where the second is defined (decision 0067,
/// point 6), not at each call, which no overload could take (roadmap 1.9.76).
#[test]
fn a_name_has_at_most_one_generic_overload() {
    let library = r"
def solve(x: complex128) -> complex128:
    return x


def solve[T: Float](x: T) -> T:
    return x


def solve[T: Float](x: Array[T, 1]) -> T:
    return x[0]
";
    let program = r"
def main():
    pass
";
    assert_eq!(
        check(&[("<std>/overloads_test.byn", library), ("main.byn", program)]),
        "\
error: `solve` already has a generic overload, `def solve[T: Float](x: T) -> T`
  --> <std>/overloads_test.byn:10:5
   |
10 | def solve[T: Float](x: Array[T, 1]) -> T:
   |     ^^^^^ a second generic overload
   = note: one generic overload may sit beside concrete ones, never two"
    );
}

#[test]
fn an_argument_that_moves_is_checked_once() {
    // Choosing the overload reads the arguments' types; the move in `first(xs)` must still
    // happen once, or the chosen overload's check reports a read after the move.
    let program = r"
from special import loggamma


def first(xs: Owned[list[f64]]) -> f64:
    return xs[0]


def main():
    xs = [3.0, 4.0]
    print(loggamma(first(xs)))
";
    assert_eq!(check_one(program), "");
}

#[test]
fn an_argument_that_borrows_is_checked_once() {
    let program = r"
from special import loggamma


def grow(xs: InOut[list[f64]]) -> f64:
    xs.append(1.0)
    return xs[0]


def main():
    xs = [3.0, 4.0]
    print(loggamma(grow(xs)))
    xs.append(2.0)
    print(len(xs))
";
    assert_eq!(check_one(program), "");
}
