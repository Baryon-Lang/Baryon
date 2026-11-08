//! Errors, end to end: `Raises`, `raise`, `try` and `except`, through every stage up to the
//! type checker.
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

/// Declared errors pass on through calls, a `try` handles one error and passes on another, an
/// `except` binds the error, a tuple `except` catches two, and a bare `raise` re-raises.
#[test]
fn handled_errors_check_clean() {
    let program = r"
class ConvergenceError(Exception):
    iterations: i64
    residual: f64


class ShapeError(Exception):
    expected: i64


def solve(n: i64) -> Raises[f64, ConvergenceError | ShapeError]:
    if n < 0:
        raise ShapeError(0)
    if n > 1000:
        raise ConvergenceError(n, 0.5)
    return 1.0


def passes_on(n: i64) -> Raises[f64, ConvergenceError | ShapeError]:
    return solve(n) * 2.0


def handles_one(n: i64) -> Raises[f64, ShapeError]:
    try:
        return solve(n)
    except ConvergenceError as error:
        return error.residual


def handles_all(n: i64) -> f64:
    try:
        x = solve(n)
    except (ConvergenceError, ShapeError):
        x = 0.0
    return x


def again(n: i64) -> Raises[f64, ConvergenceError | ShapeError]:
    try:
        return solve(n)
    except ConvergenceError:
        raise
";
    assert_eq!(check_one(program), "");
}

/// An error neither caught nor declared, from a call and from a `raise`, raising a value that
/// is not an error, catching a type that is not an error, and a bare `raise` outside `except`.
#[test]
fn unhandled_errors_are_reported() {
    let program = r"
class ConvergenceError(Exception):
    iterations: i64


class ShapeError(Exception):
    expected: i64


def solve(n: i64) -> Raises[f64, ConvergenceError | ShapeError]:
    return 1.0


def careless(n: i64) -> Raises[f64, ShapeError]:
    try:
        x = solve(n)
    except f64:
        x = 0.0
    if x < 0.0:
        raise ConvergenceError(n)
    return x


def main():
    raise 3
    raise
";
    assert_eq!(
        check_one(program),
        "\
error: `f64` is not an error type
  --> main.byn:16:12
   |
16 |     except f64:
   |            ^^^ expected an error type
   = note: an error type is a class with the base `Exception`

error: `solve` can fail with `ConvergenceError`, which is not handled here
  --> main.byn:15:13
   |
15 |         x = solve(n)
   |             ^^^^^^^^ can fail with `ConvergenceError`
   = note: catch it with `try:` and `except ConvergenceError:`, or let it pass on by declaring `-> Raises[f64, ShapeError | ConvergenceError]`

error: this `raise` fails with `ConvergenceError`, which is not handled here
  --> main.byn:19:15
   |
19 |         raise ConvergenceError(n)
   |               ^^^^^^^^^^^^^^^^^^^ can fail with `ConvergenceError`
   = note: catch it with `try:` and `except ConvergenceError:`, or let it pass on by declaring `-> Raises[f64, ShapeError | ConvergenceError]`

error: `raise` needs an error, found `i64`
  --> main.byn:24:11
   |
24 |     raise 3
   |           ^ not an error type
   = note: an error type is a class with the base `Exception`

error: a bare `raise` must be inside an `except` block
  --> main.byn:25:5
   |
25 |     raise
   |     ^^^^^ nothing to raise again
   = note: raise an error value: `raise SolverError(...)`"
    );
}
