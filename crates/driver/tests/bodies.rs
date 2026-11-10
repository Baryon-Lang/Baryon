//! Function bodies, end to end: programs go through every stage up to the type checker, and the
//! rendered diagnostics are compared in full.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline, which only lets a test start
/// it on its own line, is dropped.
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

/// Literals that take their type from the context, `Fixed` vectors built and unpacked, a
/// `Callable` argument, a generic call, loops over `range` and `enumerate`, a comprehension, an
/// `InOut` method and an f-string all check clean.
#[test]
fn valid_bodies_check_clean() {
    let program = r#"
from math import sqrt

Vec3 = Fixed[f64, (3,)]
N: Final = 8


class Body(Struct):
    position: Vec3
    mass: f64

    def push(self: InOut[Self], by: f64):
        self.mass = self.mass + by


def norm(v: Vec3) -> f64:
    x, y, z = v
    return sqrt(x * x + y * y + z * z)


def apply(f: Callable[[f64], f64], x: f64) -> f64:
    return f(x)


def half(x: f64) -> f64:
    return x / 2


def largest[T: Number](a: T, b: T) -> T:
    return a if a > b else b


def run(masses: list[f64], limit: f64 | None) -> f32:
    scale: f32 = 0
    scale = scale * 2 + 1
    body = Body(Vec3(1.0, 2.0, 3.0), 1.0)
    body.push(0.5)
    total = 0.0
    for i, mass in enumerate(masses):
        total += mass * i / N
    for j in range(1, N + 1):
        total += apply(half, f64(j))
    pairs = [k * k for k in range(4) if k % 2 == 0]
    biggest = largest(1.0, 2.0)
    print(f"total {total:.3f}", norm(body.position), len(pairs), biggest, limit)
    return scale
"#;
    assert_eq!(check_one(program), "");
}

/// Floats, integers or signed and unsigned integers of different types, `bool` in arithmetic,
/// and a literal too large for its type.
#[test]
fn numbers_that_do_not_mix_are_reported() {
    let program = r"
def mix(a: f32, b: f64, n: i32, m: i64, u: u32) -> f64:
    c = a + b
    d = n * m
    e = m + u
    f = 1 + True
    g = a < b
    small: u8 = 300
    return b
";
    assert_eq!(
        check_one(program),
        r"error: `+` cannot mix `f32` and `f64`
 --> main.byn:2:9
  |
2 |     c = a + b
  |         ^^^^^ different number types
  = note: convert one side so both are the same type, such as `f64(x)`

error: `*` cannot mix `i32` and `i64`
 --> main.byn:3:9
  |
3 |     d = n * m
  |         ^^^^^ different number types
  = note: convert one side so both are the same type, such as `i64(x)`

error: `+` cannot mix `i64` and `u32`
 --> main.byn:4:9
  |
4 |     e = m + u
  |         ^^^^^ different number types
  = note: convert one side so both are the same type, such as `i64(x)`

error: `+` cannot mix `i64` and `bool`
 --> main.byn:5:9
  |
5 |     f = 1 + True
  |         ^^^^^^^^ `bool` is not a number
  = note: convert one side so both are the same type, such as `i64(x)`

error: `<` cannot mix `f32` and `f64`
 --> main.byn:6:10
  |
6 |     g = a < b
  |          ^^^^ different number types
  = note: convert one side so both are the same type, such as `f64(x)`

error: `300` does not fit in `u8`
 --> main.byn:7:17
  |
7 |     small: u8 = 300
  |                 ^^^ out of range"
    );
}

/// A variable given a value of another type, an in-place operator that would change it, a
/// number unpacked into names, an empty list without a type, and a list of mixed items.
#[test]
fn a_variable_keeps_one_type() {
    let program = r#"
def walk(steps: int) -> f64:
    total = 0.0
    total = "done"
    count = 0
    count += 0.5
    x, y = 1.0
    empty = []
    items = [1, "a"]
    return total
"#;
    assert_eq!(
        check_one(program),
        r#"error: `total` has type `f64`, so it cannot hold `str`
 --> main.byn:3:13
  |
3 |     total = "done"
  |             ^^^^^^ type `str`
  = note: a variable keeps one type for its whole scope; give this value a new name

error: `+=` gives `f64`, but the target has type `i64`
 --> main.byn:5:5
  |
5 |     count += 0.5
  |     ^^^^^^^^^^^^ would change the target's type
  = note: a variable keeps one type for its whole scope; convert the value, or use a new name

error: a value of type `f64` cannot be unpacked into 2 names
 --> main.byn:6:5
  |
6 |     x, y = 1.0
  |     ^^^^ does not match the value

error: an empty list needs its type written
 --> main.byn:7:13
  |
7 |     empty = []
  |             ^^ the item type is unknown
  = note: for example `values: list[f64] = []`

error: list items must share one type, found `i64` and `str`
 --> main.byn:8:17
  |
8 |     items = [1, "a"]
  |                 ^^^ a different type"#
    );
}

/// Conditions, `and` operands and `not` operands that are not `bool`, and the branches of a
/// conditional expression that differ.
#[test]
fn conditions_are_bool() {
    let program = r#"
def decide(dt: f64, steps: int) -> bool:
    if steps:
        pass
    while dt and steps > 0:
        pass
    flag = not steps
    return 2.0 if dt > 0.0 else "no"
"#;
    assert_eq!(
        check_one(program),
        r#"error: a condition must be a `bool`, found `i64`
 --> main.byn:2:8
  |
2 |     if steps:
  |        ^^^^^ not a `bool`
  = note: compare explicitly, such as `n != 0` or `x is not None`

error: `and` needs `bool` operands, found `f64`
 --> main.byn:4:11
  |
4 |     while dt and steps > 0:
  |           ^^ not a `bool`
  = note: compare explicitly, such as `n != 0` or `x is not None`

error: `not` does not apply to `i64`
 --> main.byn:6:12
  |
6 |     flag = not steps
  |            ^^^^^^^^^ no such operation

error: both branches must have one type, found `f64` and `str`
 --> main.byn:7:12
  |
7 |     return 2.0 if dt > 0.0 else "no"
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^ branches differ"#
    );
}

/// Constructor and method calls with missing, repeated, extra or mistyped arguments, a missing
/// field, a missing enum member, an unknown keyword, and indexing a number.
#[test]
fn calls_fields_and_members_are_checked() {
    let program = r#"
class Particle(Struct):
    mass: f64
    speed: f64

    def energy(self, scale: f64) -> f64:
        return 0.5 * self.mass * self.speed**2 * scale


class Boundary(Enum):
    PERIODIC = 1


def step(dt: f64, steps: int = 10) -> f64:
    p = Particle(1.0)
    q = Particle(1.0, 2.0, mass=3.0)
    q.energy(1.0, 2.0)
    q.energy(scale="x")
    charge = q.charge
    side = Boundary.CLOSED
    step(1.0, steps=5, extra=1)
    return dt[0]
"#;
    assert_eq!(
        check_one(program),
        r#"error: `Particle` is missing the argument `speed`
  --> main.byn:14:9
   |
14 |     p = Particle(1.0)
   |         ^^^^^^^^^^^^^ missing arguments

error: `mass` is given twice
  --> main.byn:15:28
   |
15 |     q = Particle(1.0, 2.0, mass=3.0)
   |                            ^^^^ already given by position

error: `energy` takes 1 argument, found 2
  --> main.byn:16:5
   |
16 |     q.energy(1.0, 2.0)
   |     ^^^^^^^^^^^^^^^^^^ too many arguments

error: `scale` has type `f64`, found `str`
  --> main.byn:17:20
   |
17 |     q.energy(scale="x")
   |                    ^^^ type `str`

error: `Particle` has no field or method `charge`
  --> main.byn:18:14
   |
18 |     charge = q.charge
   |              ^^^^^^^^ not defined on this struct

error: `Boundary` has no member `CLOSED`
  --> main.byn:19:12
   |
19 |     side = Boundary.CLOSED
   |            ^^^^^^^^^^^^^^^ not a member

error: `step` has no parameter `extra`
  --> main.byn:20:24
   |
20 |     step(1.0, steps=5, extra=1)
   |                        ^^^^^ unknown parameter

error: a value of type `f64` cannot be indexed
  --> main.byn:21:12
   |
21 |     return dt[0]
   |            ^^^^^ not a sequence"#
    );
}

/// A value returned from a function without a return type, and a bare `return` in a function
/// that returns a value.
#[test]
fn returns_match_the_signature() {
    let program = r"
def nothing():
    return 1


def count() -> int:
    return
";
    assert_eq!(
        check_one(program),
        r"error: this function has no return type, so it cannot return `i64`
 --> main.byn:2:12
  |
2 |     return 1
  |            ^ returns a value
  = note: declare what it returns: `-> i64`

error: this function returns `i64`, so `return` needs a value
 --> main.byn:6:5
  |
6 |     return
  |     ^^^^^^ no value"
    );
}

/// A `math` function given the wrong number of arguments, a keyword argument, a value that is
/// not a real number, or two floats of different widths.
#[test]
fn math_functions_are_checked() {
    let program = r"
import math
from math import sqrt


def f(a: f32, b: f64, s: str) -> f64:
    c = sqrt(a, b)
    d = math.exp(x=b)
    e = math.sin(s)
    g = math.pow(a, b)
    return b
";
    assert_eq!(
        check_one(program),
        r"error: `sqrt` takes one number
 --> main.byn:6:9
  |
6 |     c = sqrt(a, b)
  |         ^^^^^^^^^^ found 2 arguments

error: `exp` takes one number
 --> main.byn:7:9
  |
7 |     d = math.exp(x=b)
  |         ^^^^^^^^^^^^^ found a keyword argument

error: `sin` takes a real number, not `str`
 --> main.byn:8:18
  |
8 |     e = math.sin(s)
  |                  ^ not an integer or a float

error: `pow` cannot mix `f32` and `f64`
 --> main.byn:9:9
  |
9 |     g = math.pow(a, b)
  |         ^^^^^^^^^^^^^^ different number types
  = note: convert one side so both are the same type, such as `f64(x)`"
    );
}

/// The rest of `math`: `fma` takes three numbers and `log` one or two, `floor` gives an `int`
/// and `isnan` a `bool`, and the three numbers of `fma` mix as arithmetic does, a literal taking
/// the type of the first.
#[test]
fn more_math_functions_are_checked() {
    let program = r"
import math


def f(a: f32, b: f64, s: str) -> f64:
    c = math.fma(a, a)
    d = math.log(b, 2.0, 3.0)
    e = math.isnan(s)
    g: f64 = math.floor(b)
    h: int = math.isinf(b)
    k = math.fma(a, a, b)
    m: f32 = math.fma(a, a, 1.0)
    return b
";
    assert_eq!(
        check_one(program),
        r"error: `fma` takes three numbers
 --> main.byn:5:9
  |
5 |     c = math.fma(a, a)
  |         ^^^^^^^^^^^^^^ found 2 arguments

error: `log` takes one or two numbers
 --> main.byn:6:9
  |
6 |     d = math.log(b, 2.0, 3.0)
  |         ^^^^^^^^^^^^^^^^^^^^^ found 3 arguments

error: `isnan` takes a real number, not `str`
 --> main.byn:7:20
  |
7 |     e = math.isnan(s)
  |                    ^ not an integer or a float

error: expected `f64`, found `i64`
 --> main.byn:8:14
  |
8 |     g: f64 = math.floor(b)
  |              ^^^^^^^^^^^^^ the annotation says `f64`

error: expected `i64`, found `bool`
 --> main.byn:9:14
  |
9 |     h: int = math.isinf(b)
  |              ^^^^^^^^^^^^^ the annotation says `i64`

error: `fma` cannot mix `f32` and `f64`
  --> main.byn:10:9
   |
10 |     k = math.fma(a, a, b)
   |         ^^^^^^^^^^^^^^^^^ different number types
   = note: convert one side so both are the same type, such as `f64(x)`"
    );
}

/// `Philox` takes a seed and an optional stream, both `int`; it has `uniform()` and nothing
/// else; and it moves, so a generator given to another name cannot draw again. `math.pi` and
/// the other constants are `f64`s.
#[test]
fn philox_and_math_constants_are_checked() {
    let program = r"
import math
from random import Philox


def f(seed: int) -> f64:
    a = Philox(seed, stream=2)
    b = Philox(stream=2)
    c = Philox(seed, 1.5)
    x = a.normal()
    y = a.uniform(1.0)
    d = a
    z = a.uniform()
    n: i64 = math.pi
    return d.uniform() + math.tau + math.e
";
    assert_eq!(
        check_one(program),
        r"error: `Philox` takes a seed and an optional stream
 --> main.byn:7:9
  |
7 |     b = Philox(stream=2)
  |         ^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `Philox(2026)` or `Philox(2026, stream=chunk)`

error: expected `i64`, found `f64`
 --> main.byn:8:22
  |
8 |     c = Philox(seed, 1.5)
  |                      ^^^ a seed or stream is `i64`

error: `Philox` has no method `normal`
 --> main.byn:9:9
  |
9 |     x = a.normal()
  |         ^^^^^^^^^^ not defined
  = note: a `Philox` has `uniform()` so far

error: `uniform` takes no arguments
  --> main.byn:10:9
   |
10 |     y = a.uniform(1.0)
   |         ^^^^^^^^^^^^^^ wrong arguments
   = note: it draws a float in `[0, 1)`; scale it for another range

error: `a` was moved
  --> main.byn:12:9
   |
12 |     z = a.uniform()
   |         ^ read after the move
  ::: main.byn:11:9
   |
11 |     d = a
   |         - `a` moved here
   = note: a generator cannot be copied, so no two draw the same numbers; make another with its own `stream`

error: expected `i64`, found `f64`
  --> main.byn:13:14
   |
13 |     n: i64 = math.pi
   |              ^^^^^^^ the annotation says `i64`"
    );
}

/// A method or attribute on a value whose type has none in version 0, such as `"abc".upper()`,
/// is reported by the checker. Before, it was left unchecked and failed at build time as "a
/// call lowering does not handle". `copy` on a `str` still checks.
#[test]
fn methods_of_strings_and_numbers_are_reported() {
    let program = "
def f(x: f64, s: str) -> str:
    print(x.real)
    s.copy()
    return s.upper()
";
    assert_eq!(
        check_one(program),
        r"error: `f64` has no attribute `real`
 --> main.byn:2:11
  |
2 |     print(x.real)
  |           ^^^^^^ not defined on this type
  = note: version 0 defines no methods or attributes on numbers, `bool`, enums or tuples

error: `str` has no method `upper`
 --> main.byn:4:12
  |
4 |     return s.upper()
  |            ^^^^^^^ not defined on this type
  = note: version 0 defines one method on `str`: `copy`"
    );
}

/// `y = None`, then `y = 3`: the note says how to declare a variable filled in later, rather
/// than to use a new name.
#[test]
fn a_none_variable_given_a_value_suggests_an_optional() {
    let program = "
def f():
    y = None
    y = 3
";
    assert_eq!(
        check_one(program),
        "error: `y` has type `None`, so it cannot hold `i64`
 --> main.byn:3:9
  |
3 |     y = 3
  |         ^ type `i64`
  = note: a variable keeps one type for its whole scope; to fill it in later, declare it optional: `y: i64 | None = None`"
    );
}

/// `print` takes the keywords `sep=` and `end=`, each a `str` or `None`; any other keyword, or
/// another type, is an error.
#[test]
fn print_takes_sep_and_end_as_text() {
    let program = r#"
def show(x: f64):
    print(x, sep=", ", end=None)
    print(x, file=None)
    print(x, end=1)
"#;
    assert_eq!(
        check_one(program),
        r"error: `print` has no parameter `file`
 --> main.byn:3:14
  |
3 |     print(x, file=None)
  |              ^^^^ unknown parameter
  = note: `print` takes the values to write, then `sep=` and `end=`

error: `end=` takes a `str` or `None`, found `i64`
 --> main.byn:4:18
  |
4 |     print(x, end=1)
  |                  ^ not a `str`"
    );
}

/// `enumerate` takes one sequence and an integer start, given second or as `start=`, and nothing
/// else. `reversed` takes one sequence whose length is known, so not `zip` or `enumerate`, which
/// only count forward (roadmap 1.13.13).
#[test]
fn enumerate_and_reversed_check_their_arguments() {
    let program = "
def main():
    xs = [1, 2]
    for p in reversed(zip(xs, xs)):
        print(p)
    for i, x in enumerate(xs, step=2):
        print(i)
    for i, x in enumerate(xs, 1.5):
        print(i)
    for x in reversed(xs, xs):
        print(x)
";
    assert_eq!(
        check_one(program),
        "\
error: `reversed` needs a sequence, and this counts forward only
 --> main.byn:3:23
  |
3 |     for p in reversed(zip(xs, xs)):
  |                       ^^^^^^^^^^^ not reversible
  = note: reverse the sequences inside: `zip(reversed(a), reversed(b))`

error: `enumerate` has no parameter `step`
 --> main.byn:5:31
  |
5 |     for i, x in enumerate(xs, step=2):
  |                               ^^^^ unknown parameter
  = note: `enumerate` takes the sequence, then `start=`

error: `enumerate` counts in integers, found `f64`
 --> main.byn:7:31
  |
7 |     for i, x in enumerate(xs, 1.5):
  |                               ^^^ not an integer

error: `reversed` takes one sequence
 --> main.byn:9:14
  |
9 |     for x in reversed(xs, xs):
  |              ^^^^^^^^^^^^^^^^ wrong number of arguments"
    );
}

/// `sum` adds a list or a rank-1 array of numbers, from a number `start` given second or as
/// `start=` (roadmap 1.13.13). `min` and `max` of a list of `str`s are accepted, as Python orders
/// them, and are left to lowering.
#[test]
fn sum_checks_its_sequence_and_start() {
    let program = "
def main():
    names = [\"a\", \"b\"]
    grid = Array.zeros((2, 2))
    print(sum(names), min(names), sum(grid), sum([1, 2], step=1), sum([1, 2], \"x\"), sum(3))
";
    let line = "4 |     print(sum(names), min(names), sum(grid), sum([1, 2], step=1), sum([1, 2], \"x\"), sum(3))";
    assert_eq!(
        check_one(program),
        format!(
            "\
error: `sum` of `list[str]`, whose elements are not numbers
 --> main.byn:4:15
  |
{line}
  |               ^^^^^ not numbers

error: `sum` of an array of rank 2 or more
 --> main.byn:4:39
  |
{line}
  |                                       ^^^^ not rank 1
  = note: reduce every element with `a.sum()`

error: `sum` has no parameter `step`
 --> main.byn:4:58
  |
{line}
  |                                                          ^^^^ unknown parameter
  = note: `sum` takes the values, then `start=`

error: `sum` starts from a number, found `str`
 --> main.byn:4:79
  |
{line}
  |                                                                               ^^^ not a number

error: `sum` adds a list or an array of numbers, found `i64`
 --> main.byn:4:89
  |
{line}
  |                                                                                         ^ not a sequence"
        )
    );
}

/// `sorted` takes one list and a `bool` `reverse=` by keyword, as Python does, and complex
/// numbers have no order; `round` takes a whole number of digits (roadmap 1.13.13).
#[test]
fn sorted_and_round_check_their_arguments() {
    let program = "
def main():
    xs = [1, 2]
    zs = [1j]
    print(sorted(xs, True), sorted(xs, reverse=1), sorted(xs, order=1))
    print(sorted(3), sorted(zs), round(2.5, 1.0))
";
    assert_eq!(
        check_one(program),
        "\
error: `sorted` takes one sequence
 --> main.byn:4:11
  |
4 |     print(sorted(xs, True), sorted(xs, reverse=1), sorted(xs, order=1))
  |           ^^^^^^^^^^^^^^^^ wrong number of arguments
  = note: write `sorted(values)` or `sorted(values, reverse=True)`

error: `reverse=` takes a `bool`, found `i64`
 --> main.byn:4:48
  |
4 |     print(sorted(xs, True), sorted(xs, reverse=1), sorted(xs, order=1))
  |                                                ^ not a `bool`

error: `sorted` has no parameter `order`
 --> main.byn:4:63
  |
4 |     print(sorted(xs, True), sorted(xs, reverse=1), sorted(xs, order=1))
  |                                                               ^^^^^ unknown parameter
  = note: `sorted` takes the values, then `key=` and `reverse=`

error: `sorted` sorts a list, found `i64`
 --> main.byn:5:18
  |
5 |     print(sorted(3), sorted(zs), round(2.5, 1.0))
  |                  ^ not a list

error: `sorted` is not defined on complex numbers
 --> main.byn:5:29
  |
5 |     print(sorted(3), sorted(zs), round(2.5, 1.0))
  |                             ^^ complex numbers
  = note: complex numbers have no order; sort by their moduli, `abs(z)`

error: `round` takes a whole number of digits, found `f64`
 --> main.byn:5:45
  |
5 |     print(sorted(3), sorted(zs), round(2.5, 1.0))
  |                                             ^^^ not an integer"
    );
}
