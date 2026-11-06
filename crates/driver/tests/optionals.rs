//! Optional values, end to end: a `T | None` is used as a `T` only after a `None` check
//! narrows it, through every stage up to the type checker.
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

/// Each way of checking: inside `if x is not None`, after an early `return`, after giving a
/// default, in `and`, in a conditional expression, after `assert`, and by assigning a value.
#[test]
fn checked_optionals_check_clean() {
    let program = r"
class Particle(Struct):
    mass: f64


def inside(distance: f64 | None) -> f64:
    if distance is not None:
        return distance * 2.0
    return 0.0


def early(distance: f64 | None) -> f64:
    if distance is None:
        return 0.0
    return distance + 1.0


def defaulted(distance: f64 | None, particle: Particle | None) -> f64:
    if distance is None:
        distance = 1.0
    assert particle is not None, 'needs a particle'
    return distance * particle.mass


def combined(distance: f64 | None, limit: f64) -> f64:
    if distance is not None and distance > limit:
        return limit
    nearest = distance if distance is not None else limit
    best: f64 | None = None
    for step in range(3):
        best = f64(step)
    found: f64 | None = None
    found = 2.0
    return nearest + found
";
    assert_eq!(check_one(program), "");
}

/// Using a `T | None` unchecked: in arithmetic, as an argument, as a return value, reading a
/// field, after an `if` whose other branch does not narrow, and after a loop that reassigns it.
#[test]
fn unchecked_optionals_are_reported() {
    let program = r"
class Particle(Struct):
    mass: f64


def half(x: f64) -> f64:
    return x / 2.0


def careless(distance: f64 | None, particle: Particle | None, flag: bool) -> f64:
    total = distance + 1.0
    half(distance)
    weight = particle.mass
    if flag:
        distance = 1.0
    if distance is not None:
        for step in range(3):
            distance = None
        return distance
    return distance
";
    assert_eq!(
        check_one(program),
        "\
error: `distance` may be `None`
  --> main.npl:10:13
   |
10 |     total = distance + 1.0
   |             ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`

error: `distance` may be `None`
  --> main.npl:11:10
   |
11 |     half(distance)
   |          ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`

error: `particle` may be `None`
  --> main.npl:12:14
   |
12 |     weight = particle.mass
   |              ^^^^^^^^ has type `Particle | None`
   = note: check it first: `if particle is not None:`

error: `distance` may be `None`
  --> main.npl:18:16
   |
18 |         return distance
   |                ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`

error: `distance` may be `None`
  --> main.npl:19:12
   |
19 |     return distance
   |            ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`"
    );
}
