//! Complex numbers in the checker, end to end: `real`, `imag`, `conjugate()`, `abs` and
//! `complex(re, im)` type as Python's `complex` has them, and what Python rejects is reported,
//! with the rendered diagnostics compared in full.
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

/// The parts are floats of the parts' width, `abs` is a float, `conjugate()` keeps the type,
/// and `complex(re, im)` converts real numbers of any type.
#[test]
fn complex_numbers_check_clean() {
    let program = r"
def modulus(z: complex64) -> f32:
    return abs(z)


def main() -> None:
    z = 1.5 + 2j
    w: complex64 = complex64(1, 0.5)
    x: f64 = z.real + z.imag + abs(z)
    y: f32 = w.real * w.imag + modulus(w)
    c: complex = z.conjugate() ** 2 / (1 - 1j)
    d: complex = complex(3, x)
    print(x, y, c, d, z == d, (z * d).real)
";
    assert_eq!(check_one(program), "");
}

/// Python's `TypeError` and `AttributeError` cases are compile errors here: assigning a part,
/// `//` and `%`, `round`, a conversion to a real number, unknown methods, a complex part, and
/// ordering.
#[test]
fn what_python_rejects_on_complex_numbers_is_reported() {
    let program = r"
def main() -> None:
    z = 1.5 + 2j
    z.real = 1.0
    f = z // 2
    g = round(z)
    h = f64(z)
    i = z.phase()
    m = complex(z, 1.0)
    n = max(z, 1j)
    o = z < 1j
";
    let expected = "\
error: the parts of a complex number cannot be assigned
 --> main.byn:3:5
  |
3 |     z.real = 1.0
  |     ^^^^^^ assigned here
  = note: build a new number instead: `z = complex(x, z.imag)`

error: `//` is not defined on complex numbers
 --> main.byn:4:9
  |
4 |     f = z // 2
  |         ^^^^^^ a complex number
  = note: Python has no `//` or `%` of complex numbers either

error: `round` is not defined on complex numbers
 --> main.byn:5:15
  |
5 |     g = round(z)
  |               ^ a complex number
  = note: round each part: `round(z.real)`, `round(z.imag)`

error: `f64(...)` cannot convert a complex number to a real one
 --> main.byn:6:9
  |
6 |     h = f64(z)
  |         ^^^^^^ a complex number
  = note: take a part with `z.real` or `z.imag`, or the modulus with `abs(z)`

error: `complex128` has no method `phase`
 --> main.byn:7:9
  |
7 |     i = z.phase()
  |         ^^^^^^^ not defined on this type
  = note: a complex number has `real`, `imag` and `conjugate()`

error: each part of a complex number is a real number, found `complex128`
 --> main.byn:8:17
  |
8 |     m = complex(z, 1.0)
  |                 ^ not a real number

error: `max` is not defined on complex numbers
 --> main.byn:9:13
  |
9 |     n = max(z, 1j)
  |             ^ a complex number
  = note: complex numbers have no order; compare their moduli with `abs(z)`

error: `<` cannot order `complex128` values
  --> main.byn:10:10
   |
10 |     o = z < 1j
   |          ^^^^^ no order";
    assert_eq!(check_one(program), expected);
}
