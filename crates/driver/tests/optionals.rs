//! Optional values, end to end: a `T | None` is used as a `T` only after a `None` check
//! narrows it, through every stage up to the type checker.
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
  --> main.byn:10:13
   |
10 |     total = distance + 1.0
   |             ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`

error: `distance` may be `None`
  --> main.byn:11:10
   |
11 |     half(distance)
   |          ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`

error: `particle` may be `None`
  --> main.byn:12:14
   |
12 |     weight = particle.mass
   |              ^^^^^^^^ has type `Particle | None`
   = note: check it first: `if particle is not None:`

error: `distance` may be `None`
  --> main.byn:18:16
   |
18 |         return distance
   |                ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`

error: `distance` may be `None`
  --> main.byn:19:12
   |
19 |     return distance
   |            ^^^^^^^^ has type `f64 | None`
   = note: check it first: `if distance is not None:`"
    );
}

/// The value of an in-place operator is checked like either side of the operator: `total += x`
/// with `x` unchecked is the error `total = total + x` is, where it once reached the backend
/// (roadmap 1.9.54). A field is not narrowed by a check, so `row.rain` stays optional.
#[test]
fn in_place_operators_check_their_value() {
    let program = "\
class Reading(Struct):
    rain: f64 | None


def add(x: f64 | None, n: i64 | None, s: str | None, row: Reading):
    total = 0.0
    total += x
    k = 1
    k *= n
    t = \"a\"
    t += s
    if row.rain is not None:
        total += row.rain
    print(total, k, t)
";
    assert_eq!(
        check_one(program),
        "\
error: `x` may be `None`
 --> main.byn:7:14
  |
7 |     total += x
  |              ^ has type `f64 | None`
  = note: check it first: `if x is not None:`

error: `n` may be `None`
 --> main.byn:9:10
  |
9 |     k *= n
  |          ^ has type `i64 | None`
  = note: check it first: `if n is not None:`

error: `s` may be `None`
  --> main.byn:11:10
   |
11 |     t += s
   |          ^ has type `str | None`
   = note: check it first: `if s is not None:`

error: this value may be `None`
  --> main.byn:13:18
   |
13 |         total += row.rain
   |                  ^^^^^^^^ has type `f64 | None`
   = note: give it a name and check it first: `if value is not None:`"
    );
}

/// A tuple or dict whose part is only ever `None`, with no type written, is reported (roadmap
/// 1.9.72): the backend stores no value of the type `None` inside another value yet. Each
/// note shows the written type that makes room for a value; with it, each builds.
#[test]
fn a_value_holding_only_none_is_reported() {
    let program = r#"
def main():
    print((1.5, None))
    names = {"a": None}
    t: tuple[f64, f64 | None] = (1.5, None)
"#;
    assert_eq!(
        check_one(program),
        r#"error: a tuple cannot hold the type `None` on its own yet
 --> main.byn:2:17
  |
2 |     print((1.5, None))
  |                 ^^^^ only ever `None`
  = note: write the type it holds, with `| None`: `t: tuple[f64, f64 | None] = (1.5, None)`

error: a dict cannot hold the type `None` on its own yet
 --> main.byn:3:13
  |
3 |     names = {"a": None}
  |             ^^^^^^^^^^^ only ever `None`
  = note: write the type it holds, with `| None`: `d: dict[str, f64 | None] = {"a": None}`"#
    );
}
