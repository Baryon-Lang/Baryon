//! Checking the standard streams (decision 0036, part 6), end to end: programs that misuse
//! `sys.stdin`, `sys.stdout`, `sys.stderr`, `print(..., file=, flush=)` or `input` go through
//! the parser, the syntax check, the resolver and the checker, and the rendered diagnostics are
//! compared in full.

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

/// A program that uses the streams as they are meant to be used has no diagnostics: printing to
/// a named stream cannot fail, so `report` declares no error, and a stream is a `File` like any
/// other, to hold, pass on and loop over.
#[test]
fn streams_check_clean() {
    let program = r#"
import sys
from sys import stderr


def report(message: str):
    print("warning:", message, file=sys.stderr)
    print(message, file=stderr, flush=True)
    print(message, file=sys.stdout)


def copy(source: InOut[File], out: InOut[File]) -> Raises[None, OSError]:
    for line in source:
        print(line, end="", file=out)


def main() -> Raises[None, OSError]:
    report("starting")
    source = sys.stdin
    out = sys.stdout
    copy(source, out)
    try:
        name = input("name: ")
        print(name)
    except EOFError:
        report("no name given")
"#;
    assert_eq!(check_one(program), "");
}

/// `print` explains a `file=` that is `sys.stdin`, is not a file, or is a file the function may
/// not change, a `flush=` that is not a `bool`, and the `OSError` of a print to a file that
/// nobody handles. `sys.stdout` cannot be assigned, as no member of a module can.
#[test]
fn print_targets_are_checked() {
    let program = r#"
import sys


def main() -> Raises[None, OSError]:
    print("x", file=sys.stdin)
    print("x", file="log.txt")
    print("x", flush=1)
    sys.stdout = open("log.txt", "w")
    sys.argv = ["a"]


def borrowed(log: File) -> Raises[None, OSError]:
    print("x", file=log)


def quiet(log: InOut[File]):
    print("x", file=log)
    out = sys.stderr
    print("x", file=out)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `print` cannot write to `sys.stdin`
 --> main.byn:5:21
  |
5 |     print("x", file=sys.stdin)
  |                     ^^^^^^^^^ read only
  = note: write to `sys.stdout`, `sys.stderr` or a file opened to write

error: `file=` takes a `File`, found `str`
 --> main.byn:6:21
  |
6 |     print("x", file="log.txt")
  |                     ^^^^^^^^^ not a file
  = note: for example `file=sys.stderr`, or a file from `open(path, "w")`

error: expected `bool`, found `i64`
 --> main.byn:7:22
  |
7 |     print("x", flush=1)
  |                      ^ `flush=` takes a `bool`

error: `sys.stdout` cannot be assigned
 --> main.byn:8:5
  |
8 |     sys.stdout = open("log.txt", "w")
  |     ^^^^^^^^^^ a module has no variables
  = note: to write somewhere else, give `print` the file: `print(..., file=log)`

error: `sys.argv` cannot be assigned
 --> main.byn:9:5
  |
9 |     sys.argv = ["a"]
  |     ^^^^^^^^ a module has no variables

error: `log` is a read-only parameter, so it cannot be changed
  --> main.byn:13:21
   |
13 |     print("x", file=log)
   |                     ^^^ changed here
   = note: take it as `InOut[File]` to use the caller's

error: `print` can fail with `OSError`, which is not handled here
  --> main.byn:17:5
   |
17 |     print("x", file=log)
   |     ^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `print` can fail with `OSError`, which is not handled here
  --> main.byn:19:5
   |
19 |     print("x", file=out)
   |     ^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}

/// `input` takes at most one prompt, a `str`, and its `EOFError` must be handled or declared.
#[test]
fn input_is_checked() {
    let program = r#"
def main() -> Raises[None, EOFError]:
    name = input("first: ", "last: ")
    size = input(3)
    count: int = input()


def quiet() -> str:
    return input()
"#;
    assert_eq!(
        check_one(program),
        r#"error: `input` takes one prompt
 --> main.byn:2:12
  |
2 |     name = input("first: ", "last: ")
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `input()` or `input("grid size: ")`

error: expected `str`, found `i64`
 --> main.byn:3:18
  |
3 |     size = input(3)
  |                  ^ a prompt is `str`

error: expected `i64`, found `str`
 --> main.byn:4:18
  |
4 |     count: int = input()
  |                  ^^^^^^^ the annotation says `i64`

error: `input` can fail with `EOFError`, which is not handled here
 --> main.byn:8:12
  |
8 |     return input()
  |            ^^^^^^^ can fail with `EOFError`
  = note: catch it with `try:` and `except EOFError:`, or let it pass on by declaring `-> Raises[str, EOFError]`"#
    );
}
