//! Checking text tables (decision 0043), end to end: programs that misuse `Array.loadtxt` and
//! `Array.savetxt` go through the parser, the syntax check, the resolver and the checker, and
//! the rendered diagnostics are compared in full.

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

/// Calls that are right check with no diagnostic: both ranks of each function, `dtype` by
/// position and keyword, and every keyword the first version takes.
#[test]
fn every_built_form_checks() {
    let program = r##"
def main() -> Raises[None, OSError | ValueError]:
    table = Array.loadtxt("t.csv", f64, comments="%", delimiter=",", skiprows=1, max_rows=9)
    column: Array[f64, 1] = Array.loadtxt("c.txt", dtype=f64)
    Array.savetxt("t.txt", table, delimiter=",", newline="\n", header="x", footer="y", comments="# ")
    Array.savetxt(fname="c.txt", X=column)
"##;
    assert_eq!(check_one(program), "");
}

/// Keywords `NumPy` has that the first version does not take, an element type other than `f64`,
/// an array of the wrong rank or type, too many positional arguments, and an unhandled error.
#[test]
fn what_is_not_built_is_named() {
    let program = r##"
def main() -> Raises[None, OSError | ValueError]:
    a = Array.loadtxt("t.txt", usecols=1)
    b = Array.loadtxt("t.txt", i64)
    c = Array.loadtxt("t.txt", f64, "#")
    Array.savetxt("t.txt", a, fmt="%d")
    cube = Array.zeros((2, 2, 2))
    Array.savetxt("t.txt", cube)
    counts = Array.zeros((2,), i64)
    Array.savetxt("t.txt", counts)
    Array.savetxt("t.txt")


def careless() -> None:
    table = Array.loadtxt("t.txt")
"##;
    assert_eq!(
        check_one(program),
        r##"error: `Array.loadtxt` does not take `usecols` yet
 --> main.byn:2:32
  |
2 |     a = Array.loadtxt("t.txt", usecols=1)
  |                                ^^^^^^^ not built yet
  = note: decision 0043 lists what the first version takes; until then, `open`, `str` methods and f-strings do it by hand

error: `Array.loadtxt` reads `f64` numbers only in this version
 --> main.byn:3:32
  |
3 |     b = Array.loadtxt("t.txt", i64)
  |                                ^^^ not built yet
  = note: leave out `dtype`, or write `dtype=f64`, and convert the array after

error: `Array.loadtxt` takes `fname` and `dtype` by position
 --> main.byn:4:9
  |
4 |     c = Array.loadtxt("t.txt", f64, "#")
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ too many by position
  = note: pass the rest by keyword, such as `delimiter=","`

error: `Array.savetxt` does not take `fmt` yet
 --> main.byn:5:31
  |
5 |     Array.savetxt("t.txt", a, fmt="%d")
  |                               ^^^ not built yet
  = note: decision 0043 lists what the first version takes; until then, `open`, `str` methods and f-strings do it by hand

error: `Array.savetxt` writes an `Array[f64, 1]` or `Array[f64, 2]`, found `Array[f64, 3]`
 --> main.byn:7:28
  |
7 |     Array.savetxt("t.txt", cube)
  |                            ^^^^ cannot be written as a table
  = note: a text table has rows and columns; write one slice at a time

error: `Array.savetxt` writes an `Array[f64, 1]` or `Array[f64, 2]`, found `Array[i64, 1]`
 --> main.byn:9:28
  |
9 |     Array.savetxt("t.txt", counts)
  |                            ^^^^^^ cannot be written as a table
  = note: convert the elements to `f64` first, such as `f64(n)` of each one

error: `Array.savetxt` needs the array to write
  --> main.byn:10:5
   |
10 |     Array.savetxt("t.txt")
   |     ^^^^^^^^^^^^^^^^^^^^^^ no array
   = note: for example `Array.savetxt("out.txt", table)`

error: `Array.loadtxt` can fail with `OSError`, which is not handled here
  --> main.byn:14:13
   |
14 |     table = Array.loadtxt("t.txt")
   |             ^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `Array.loadtxt` can fail with `ValueError`, which is not handled here
  --> main.byn:14:13
   |
14 |     table = Array.loadtxt("t.txt")
   |             ^^^^^^^^^^^^^^^^^^^^^^ can fail with `ValueError`
   = note: catch it with `try:` and `except ValueError:`, or let it pass on by declaring `-> Raises[None, ValueError]`"##
    );
}
