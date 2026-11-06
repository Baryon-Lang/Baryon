//! Enums and `match`, end to end: patterns checked against the subject, captures bound, and
//! every value of the subject handled, through every stage up to the type checker.
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

/// Every member listed, `|` alternatives, a wildcard after literals, `True` and `False`, `None`
/// with a capture, and a tuple of an enum and a `bool` taken apart.
#[test]
fn exhaustive_matches_check_clean() {
    let program = r"
class Boundary(Enum):
    PERIODIC = 1
    REFLECTING = 2
    ABSORBING = 3


def wrap(boundary: Boundary, index: i64, n: i64, flag: bool, limit: f64 | None) -> f64:
    match boundary:
        case Boundary.PERIODIC:
            index = index % n
        case Boundary.REFLECTING | Boundary.ABSORBING:
            index = min(index, n - 1)
    match index:
        case 0 | 1:
            index = 2
        case other:
            index = other
    match flag:
        case True:
            index += 1
        case False:
            index -= 1
    match (boundary, flag):
        case (Boundary.PERIODIC, _):
            index = 0
        case (_, True):
            index = 1
        case (_, False):
            index = 2
    match limit:
        case None:
            return 0.0
        case value:
            return value + f64(index)
";
    assert_eq!(check_one(program), "");
}

/// A missing member, a guard that does not count, a pattern of the wrong type, a tuple of the
/// wrong length, a class pattern, and ordering enum members.
#[test]
fn match_mistakes_are_reported() {
    let program = r"
class Boundary(Enum):
    PERIODIC = 1
    REFLECTING = 2
    ABSORBING = 3


def wrap(boundary: Boundary, index: i64, pair: tuple[i64, bool]) -> i64:
    match boundary:
        case Boundary.PERIODIC | Boundary.REFLECTING:
            return 0
    match index:
        case 0:
            return 1
        case n if n > 0:
            return n
    match index:
        case 'zero':
            return 2
        case (a, b):
            return a
        case Boundary(x=1):
            return 3
        case _:
            return 4
    match pair:
        case (_, True):
            return 5
    if boundary < Boundary.ABSORBING:
        return 6
    return 7
";
    assert_eq!(
        check_one(program),
        "\
error: this `match` does not handle `Boundary.ABSORBING`
 --> main.npl:8:11
  |
8 |     match boundary:
  |           ^^^^^^^^ not every value is handled
  = note: add `case Boundary.ABSORBING:`, or `case _:` for every value not matched yet; a `case` with an `if` guard may not match, so it does not count

error: this `match` does not handle every value of type `i64`
  --> main.npl:11:11
   |
11 |     match index:
   |           ^^^^^ not every value is handled
   = note: add `case _:` at the end; a `case` with an `if` guard may not match, so it does not count

error: a pattern of type `str` cannot match a value of type `i64`
  --> main.npl:17:14
   |
17 |         case 'zero':
   |              ^^^^^^ does not match the subject

error: a tuple pattern of 2 items cannot match a value of type `i64`
  --> main.npl:19:14
   |
19 |         case (a, b):
   |              ^^^^^^ does not match the subject

error: this kind of pattern is not supported
  --> main.npl:21:14
   |
21 |         case Boundary(x=1):
   |              ^^^^^^^^^^^^^ not supported
   = note: a `case` matches literals, enum members, `True`, `False`, `None`, tuples, names and `_`

error: this `match` does not handle `(_, False)`
  --> main.npl:25:11
   |
25 |     match pair:
   |           ^^^^ not every value is handled
   = note: add `case (_, False):`, or `case _:` for every value not matched yet; a `case` with an `if` guard may not match, so it does not count

error: `<` cannot order `Boundary` values
  --> main.npl:28:16
   |
28 |     if boundary < Boundary.ABSORBING:
   |                ^^^^^^^^^^^^^^^^^^^^^ no order
   = note: enum members are compared with `==` and `!=`"
    );
}
