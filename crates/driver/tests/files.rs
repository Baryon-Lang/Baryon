//! Checking text files (decision 0036, parts 3 and 4), end to end: programs that misuse `open`,
//! a `File` or `with` go through the parser, the syntax check, the resolver and the checker, and
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

/// `open` explains each wrong argument: the modes Python has and Baryon does not, a mode that
/// is not a literal, an encoding other than UTF-8, no path at all, and an `OSError` nobody
/// handles.
#[test]
fn open_arguments_are_explained() {
    let program = r#"
def main() -> Raises[None, OSError]:
    raw = open("data.bin", "rb")
    both = open("data.txt", "r+")
    mode = "w"
    named = open("out.txt", mode)
    latin = open("data.txt", encoding="latin-1")
    open()


def quiet():
    open("data.txt")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `open` has no mode `"rb"`
 --> main.byn:2:28
  |
2 |     raw = open("data.bin", "rb")
  |                            ^^^^ unknown mode
  = note: binary data is whole arrays: `Array.fromfile(path, f64)` and `a.tofile(path)` (decision 0036)

error: `open` has no mode `"r+"`
 --> main.byn:3:29
  |
3 |     both = open("data.txt", "r+")
  |                             ^^^^ unknown mode
  = note: a file is opened to read or to write, not both

error: `open`'s mode must be written out
 --> main.byn:5:29
  |
5 |     named = open("out.txt", mode)
  |                             ^^^^ not a literal
  = note: write `"r"`, `"w"`, `"a"` or `"x"`

error: text files are UTF-8
 --> main.byn:6:39
  |
6 |     latin = open("data.txt", encoding="latin-1")
  |                                       ^^^^^^^^^ not UTF-8
  = note: leave `encoding` out, or write `encoding="utf-8"`

error: `open` takes a path and an optional mode
 --> main.byn:7:5
  |
7 |     open()
  |     ^^^^^^ wrong arguments
  = note: for example `open("data.txt")` or `open("out.txt", "w")`

error: `open` can fail with `OSError`, which is not handled here
  --> main.byn:11:5
   |
11 |     open("data.txt")
   |     ^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}

/// Using a `File` wrongly is explained: changing it through a read-only parameter, looping over
/// it, using it after `close()`, `with` on something that is not a file, and wrong arguments and
/// methods.
#[test]
fn file_mistakes_are_explained() {
    let program = r#"
def log(out: File) -> Raises[None, OSError]:
    out.write("x")


def main() -> Raises[None, OSError]:
    f = open("data.txt")
    for line in f:
        print(line)
    f.close()
    f.read()
    with 3 as h:
        pass
    g = open("out.txt", "w")
    g.write(3)
    g.seek(0)
    g.read(10)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `out` is a read-only parameter, so it cannot be changed
 --> main.byn:2:5
  |
2 |     out.write("x")
  |     ^^^ changed here
  = note: take it as `InOut[File]` to use the caller's

error: looping over a `File` is not in Baryon yet
 --> main.byn:7:17
  |
7 |     for line in f:
  |                 ^ not built yet
  = note: loop over its lines as a list: `for line in f.readlines():`

error: `f` was moved
  --> main.byn:10:5
   |
10 |     f.read()
   |     ^ read after the move
  ::: main.byn:9:5
   |
 9 |     f.close()
   |     - `f` moved here
   = note: a file has one owner, and `close()` or `with` ends it; open it again to read it again

error: `with` needs a `File`, found `i64`
  --> main.byn:11:10
   |
11 |     with 3 as h:
   |          ^ not a file
   = note: `with` is for `with open(...) as f:` and `with unsafe:` (decision 0036)

error: expected `str`, found `i64`
  --> main.byn:14:13
   |
14 |     g.write(3)
   |             ^ `write` takes a `str`

error: `File` has no method `seek`
  --> main.byn:15:5
   |
15 |     g.seek(0)
   |     ^^^^^^^^^ not defined
   = note: a `File` has `read`, `readline`, `readlines`, `write`, `flush` and `close`

error: `read` takes no arguments
  --> main.byn:16:5
   |
16 |     g.read(10)
   |     ^^^^^^^^^^ wrong arguments"#
    );
}
