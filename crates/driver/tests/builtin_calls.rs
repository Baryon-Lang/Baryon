//! Calls of built-in functions, end to end: programs go through every stage up to the type
//! checker, and the diagnostics for each built-in's arguments are compared in full. The other
//! body checks are in `bodies.rs`.
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

/// Inside a function generic over `T: Float`, a `math` function takes `T` and returns `T`, so one
/// function serves `f32` and `f64`; an argument of another type does not mix with `T`, since
/// the instance may be `f32`.
#[test]
fn math_functions_take_a_float_type_parameter() {
    let program = r"
from math import hypot, isfinite, sqrt


def norm[T: Float](x: T, y: T) -> T:
    if not isfinite(x):
        return x
    return sqrt(x * x + y * y) + hypot(x, y)


def mix[T: Float](x: T, y: f64) -> T:
    return hypot(x, y)
";
    assert_eq!(
        check_one(program),
        r"error: `hypot` cannot mix `T` and `f64`
  --> main.byn:11:21
   |
11 |     return hypot(x, y)
   |                     ^ not a `T`"
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

/// `print` takes the keywords `sep=` and `end=`, each a `str` or `None`; an unknown keyword, or
/// another type, is an error. `file=` and `flush=` are tested with the standard streams.
#[test]
fn print_takes_sep_and_end_as_text() {
    let program = r#"
def show(x: f64):
    print(x, sep=", ", end=None)
    print(x, color=None)
    print(x, end=1)
"#;
    assert_eq!(
        check_one(program),
        r"error: `print` has no parameter `color`
 --> main.byn:3:14
  |
3 |     print(x, color=None)
  |              ^^^^^ unknown parameter
  = note: `print` takes the values to write, then `sep=`, `end=`, `file=` and `flush=`

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

/// `math.fsum` takes one list or rank-1 array of `f64`s by position (roadmap 1.13.16): integer
/// elements, a table, a value that is not a sequence, a second argument and a keyword are each
/// an error that says what to pass instead.
#[test]
fn fsum_checks_its_values() {
    let program = "
import math
from math import fsum


def f(counts: list[int], grid: Array[f64, 2], text: str, xs: list[f64]) -> f64:
    a = fsum(counts)
    b = math.fsum(grid)
    c = fsum(text)
    d = fsum(xs, 0.0)
    e = fsum(values=xs)
    return fsum(xs) + fsum(Array([1.0, 2.0]))
";
    assert_eq!(
        check_one(program),
        "\
error: `fsum` adds a list or rank-1 array of `f64`s, found `list[i64]`
 --> main.byn:6:14
  |
6 |     a = fsum(counts)
  |              ^^^^^^ cannot be added exactly
  = note: convert the elements to `f64` first, such as `[f64(n) for n in counts]`

error: `fsum` adds a list or rank-1 array of `f64`s, found `Array[f64, 2]`
 --> main.byn:7:19
  |
7 |     b = math.fsum(grid)
  |                   ^^^^ cannot be added exactly
  = note: add one row at a time, such as `fsum(grid[i, :])`

error: `fsum` adds a list or rank-1 array of `f64`s, found `str`
 --> main.byn:8:14
  |
8 |     c = fsum(text)
  |              ^^^^ cannot be added exactly
  = note: pass a `list[f64]` or an `Array[f64, 1]`

error: `fsum` takes one list or array, found 2 arguments
 --> main.byn:9:9
  |
9 |     d = fsum(xs, 0.0)
  |         ^^^^^^^^^^^^^ wrong number of arguments
  = note: pass the values to add, such as `fsum(energies)`

error: `fsum` takes its values by position, as Python's does
  --> main.byn:10:9
   |
10 |     e = fsum(values=xs)
   |         ^^^^^^^^^^^^^^^ a keyword argument
   = note: pass the values to add by position, such as `fsum(energies)`"
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
