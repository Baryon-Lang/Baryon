//! Checking array files (decision 0036, part 4), end to end: programs that misuse
//! `Array.fromfile` and `a.tofile` go through the parser, the syntax check, the resolver and the
//! checker, and the rendered diagnostics are compared in full.

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

/// `Array.fromfile` names an element type whose bytes a file cannot supply: a type parameter,
/// an enum, a struct holding one and a `str`. Its result is rank 1 with the element type asked
/// for, so an annotation that says otherwise is a mismatch.
#[test]
fn unreadable_element_types_are_explained() {
    let program = r#"
class Phase(Enum):
    SOLID = 0
    LIQUID = 1


class Cell(Struct, Copy):
    phase: Phase
    mass: f64


def generic[T](path: str, like: T) -> Raises[None, OSError]:
    values = Array.fromfile(path, T)


def main() -> Raises[None, OSError]:
    phases = Array.fromfile("p.bin", Phase)
    cells = Array.fromfile("c.bin", dtype=Cell)
    names = Array.fromfile("n.bin", str)
    matrix: Array[f64, 2] = Array.fromfile("m.bin")
    small: Array[f32, 1] = Array.fromfile("m.bin", f64)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `Array.fromfile` cannot read elements of type `T`
  --> main.byn:12:35
   |
12 |     values = Array.fromfile(path, T)
   |                                   ^ not plain data
   = note: a file's bytes become the elements as they are, so the element type is a number, a `bool`, a `Fixed` array of them or a `Copy` struct of them, written out

error: `Array.fromfile` cannot read elements of type `Phase`
  --> main.byn:16:38
   |
16 |     phases = Array.fromfile("p.bin", Phase)
   |                                      ^^^^^ not plain data
   = note: a file's bytes become the elements as they are, so the element type is a number, a `bool`, a `Fixed` array of them or a `Copy` struct of them, written out

error: `Array.fromfile` cannot read elements of type `Cell`
  --> main.byn:17:43
   |
17 |     cells = Array.fromfile("c.bin", dtype=Cell)
   |                                           ^^^^ not plain data
   = note: a file's bytes become the elements as they are, so the element type is a number, a `bool`, a `Fixed` array of them or a `Copy` struct of them, written out

error: `Array.fromfile` cannot read elements of type `str`
  --> main.byn:18:37
   |
18 |     names = Array.fromfile("n.bin", str)
   |                                     ^^^ not plain data
   = note: a file's bytes become the elements as they are, so the element type is a number, a `bool`, a `Fixed` array of them or a `Copy` struct of them, written out

error: expected `Array[f64, 2]`, found `Array[f64, 1]`
  --> main.byn:19:29
   |
19 |     matrix: Array[f64, 2] = Array.fromfile("m.bin")
   |                             ^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `Array[f64, 2]`

error: expected `Array[f32, 1]`, found `Array[f64, 1]`
  --> main.byn:20:28
   |
20 |     small: Array[f32, 1] = Array.fromfile("m.bin", f64)
   |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `Array[f32, 1]`"#
    );
}

/// `Array.fromfile` and `tofile` explain wrong arguments: the text mode of `NumPy`'s functions,
/// an argument given twice, of the wrong type or missing. An `OSError` nobody handles is
/// reported as any call's is.
#[test]
fn wrong_arguments_and_unhandled_errors_are_explained() {
    let program = r#"
def main() -> Raises[None, OSError]:
    text = Array.fromfile("t.txt", f64, sep=" ")
    twice = Array.fromfile("t.bin", f64, dtype=f32)
    count = Array.fromfile("t.bin", f64, 2.5)
    nothing = Array.fromfile()
    path = Array.fromfile(3)
    grid = Array.zeros((2, 2))
    grid.tofile()
    grid.tofile("a.bin", sep=",")
    grid.tofile(7)


def quiet():
    grid = Array.fromfile("g.bin")
    grid.tofile("h.bin")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `Array.fromfile` takes a path and an optional `dtype`, `count` and `offset`
 --> main.byn:2:12
  |
2 |     text = Array.fromfile("t.txt", f64, sep=" ")
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: an array file is binary; read text with `open` and the `str` methods

error: `Array.fromfile` takes a path and an optional `dtype`, `count` and `offset`
 --> main.byn:3:13
  |
3 |     twice = Array.fromfile("t.bin", f64, dtype=f32)
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `Array.fromfile("field.bin", f64)` or `Array.fromfile(path, f32, count=1024, offset=8192)`

error: expected `i64`, found `f64`
 --> main.byn:4:42
  |
4 |     count = Array.fromfile("t.bin", f64, 2.5)
  |                                          ^^^ a count is `i64`

error: `Array.fromfile` takes a path and an optional `dtype`, `count` and `offset`
 --> main.byn:5:15
  |
5 |     nothing = Array.fromfile()
  |               ^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `Array.fromfile("field.bin", f64)` or `Array.fromfile(path, f32, count=1024, offset=8192)`

error: expected `str`, found `i64`
 --> main.byn:6:27
  |
6 |     path = Array.fromfile(3)
  |                           ^ a path is `str`

error: `tofile` takes one path
 --> main.byn:8:5
  |
8 |     grid.tofile()
  |     ^^^^^^^^^^^^^ wrong arguments
  = note: it writes the array's elements as binary, `a.tofile("field.bin")`; write text with `open` and f-strings

error: `tofile` takes one path
 --> main.byn:9:5
  |
9 |     grid.tofile("a.bin", sep=",")
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: it writes the array's elements as binary, `a.tofile("field.bin")`; write text with `open` and f-strings

error: expected `str`, found `i64`
  --> main.byn:10:17
   |
10 |     grid.tofile(7)
   |                 ^ a path is `str`

error: `Array.fromfile` can fail with `OSError`, which is not handled here
  --> main.byn:14:12
   |
14 |     grid = Array.fromfile("g.bin")
   |            ^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `grid.tofile` can fail with `OSError`, which is not handled here
  --> main.byn:15:5
   |
15 |     grid.tofile("h.bin")
   |     ^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}
