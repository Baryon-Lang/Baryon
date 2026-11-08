//! Definite assignment and missing returns, end to end, through every stage up to the type
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

/// Assigned in every branch, in a `try` and its handler, in every case of a `match`, before a
/// loop, and functions that end in `return`, `raise`, an `if` whose branches all return, an
/// exhaustive `match` of returns, or `while True` with no `break`.
#[test]
fn assigned_variables_and_returns_check_clean() {
    let program = r"
class Boundary(Enum):
    PERIODIC = 1
    REFLECTING = 2


class SolverError(Exception):
    step: i64


def classify(energy: f64) -> str:
    if energy < 0.0:
        label = 'bound'
    elif energy == 0.0:
        label = 'threshold'
    else:
        label = 'free'
    return label


def by_case(boundary: Boundary) -> f64:
    match boundary:
        case Boundary.PERIODIC:
            factor = 1.0
        case Boundary.REFLECTING:
            factor = -1.0
    return factor


def solve(n: i64) -> Raises[f64, SolverError]:
    if n < 0:
        raise SolverError(n)
    return 1.0


def guarded(n: i64) -> f64:
    try:
        result = solve(n)
    except SolverError:
        result = 0.0
    total = 0.0
    for step in range(n):
        total += result
    return total


def iterate(n: i64) -> i64:
    count = 0
    while True:
        count += 1
        if count > n:
            return count


def fails(n: i64) -> Raises[f64, SolverError]:
    raise SolverError(n)
";
    assert_eq!(check_one(program), "");
}

/// A branch that skips the assignment, a loop that may run zero times, a handler that skips
/// it, a read before any assignment, a declared but unassigned variable, and a function that
/// can reach its end.
#[test]
fn unassigned_reads_and_missing_returns_are_reported() {
    let program = r"
def classify(energy: f64) -> str:
    if energy < 0.0:
        label = 'bound'
    return label


def looped(n: i64) -> i64:
    for step in range(n):
        last = step
    return last


def early(n: i64) -> i64:
    total = count + n
    count = 1
    width: i64
    return total + width


def falls(n: i64) -> i64:
    if n > 0:
        return n
";
    assert_eq!(
        check_one(program),
        "\
error: `label` may not be assigned here
 --> main.byn:4:12
  |
4 |     return label
  |            ^^^^^ read here
 ::: main.byn:2:8
  |
2 |     if energy < 0.0:
  |        ------------ not assigned when this is false
  = note: give it a value before the branch or loop, or assign it on every path that reaches this read

error: `last` may not be assigned here
  --> main.byn:10:12
   |
10 |     return last
   |            ^^^^ read here
  ::: main.byn:8:17
   |
 8 |     for step in range(n):
   |                 -------- not assigned if the loop runs zero times
   = note: give it a value before the branch or loop, or assign it on every path that reaches this read

error: `count` is read before it is assigned
  --> main.byn:14:13
   |
14 |     total = count + n
   |             ^^^^^ read here

error: `width` is read before it is assigned
  --> main.byn:17:20
   |
17 |     return total + width
   |                    ^^^^^ read here

error: `falls` can reach its end without returning a value
  --> main.byn:20:5
   |
20 | def falls(n: i64) -> i64:
   |     ^^^^^ returns `i64`
   = note: add a `return` at the end, or make every branch end in `return` or `raise`"
    );
}

/// A number literal assigned to a field or an element takes the place's type, as it does for
/// an annotated variable: `p.count = 5` on an `i32` field is an `i32`. Before this, the literal
/// kept its default type, and `p.w = 1.5` on an `f32` field was "expected `f32`, found `f64`".
/// A value of the wrong kind is still reported.
#[test]
fn literals_assigned_to_fields_and_elements_take_their_type() {
    let program = "
class P(Struct, Copy):
    count: i32
    w: f32


def fill(p: InOut[P], xs: InOut[Array[i32, 1]], fs: InOut[Array[f32, 1]]):
    p.count = 5
    p.w = 1.5
    xs[1] = -9
    fs[1] = 2
    p.count = 0.5
";
    assert_eq!(
        check_one(program),
        "\
error: expected `i32`, found `f64`
  --> main.byn:11:15
   |
11 |     p.count = 0.5
   |               ^^^ the target holds `i32`"
    );
}
