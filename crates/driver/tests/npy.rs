//! Checking `.npy` files (decision 0043), end to end: programs that misuse `Array.load` and
//! `Array.save` go through the parser, the syntax check, the resolver and the checker, and the
//! rendered diagnostics are compared in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
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

/// Arrays of structs and of `str`, missing and unknown arguments, `NumPy`'s keywords that do
/// not apply, and an unhandled `OSError`.
#[test]
fn what_cannot_be_a_npy_file_is_explained() {
    let program = r#"
class Point(Struct, Copy):
    x: f64
    y: f64


def main() -> Raises[None, OSError]:
    points: Array[Point, 1] = Array.load("p.npy")
    Array.save("p.npy", points)
    Array.save("p.npy")
    grid = Array.load("g.npy", allow_pickle=True)
    Array.save("g.npy", [1.0, 2.0])


def careless() -> None:
    grid = Array.load("g.npy")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `Array.load` cannot read a `.npy` file into an `Array[Point, 1]`
 --> main.byn:7:31
  |
7 |     points: Array[Point, 1] = Array.load("p.npy")
  |                               ^^^^^^^^^^^^^^^^^^^ not a number type
  = note: a `.npy` file holds numbers, `bool`s or complex numbers; records are roadmap 3.25's

error: `Array.save` writes an `Array` of numbers, `bool`s or complex numbers, found `Array[Point, 1]`
 --> main.byn:8:25
  |
8 |     Array.save("p.npy", points)
  |                         ^^^^^^ cannot be saved
  = note: a `.npy` file names its element type as `NumPy` does; records are roadmap 3.25's

error: `Array.save` takes `file` and `arr`
 --> main.byn:9:5
  |
9 |     Array.save("p.npy")
  |     ^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `Array.save("field.npy", field)`

error: `Array.load` does not take `allow_pickle`
  --> main.byn:10:32
   |
10 |     grid = Array.load("g.npy", allow_pickle=True)
   |                                ^^^^^^^^^^^^ not taken
   = note: a `.npy` file holds numbers, read straight into the array

error: `Array.save` writes an `Array` of numbers, `bool`s or complex numbers, found `list[f64]`
  --> main.byn:11:25
   |
11 |     Array.save("g.npy", [1.0, 2.0])
   |                         ^^^^^^^^^^ cannot be saved
   = note: make an array first, such as `Array(values)` of a list

error: `Array.load` can fail with `OSError`, which is not handled here
  --> main.byn:15:12
   |
15 |     grid = Array.load("g.npy")
   |            ^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}
