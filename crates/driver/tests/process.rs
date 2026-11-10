//! Checking `sys` and `os` (decision 0036), end to end: programs go through the parser, the
//! syntax check, the resolver and the checker, and the rendered diagnostics are compared in
//! full.

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

/// `sys.argv` is a `list[str]`, `sys.exit` takes one `int`, `os.getenv` and `os.environ.get`
/// take a `str` name and an optional `str` default, and a member decision 0036 has but this
/// slice does not build is reported by name (decision 0036).
#[test]
fn sys_and_os_are_checked() {
    let program = r#"
import os
import sys


def f() -> str:
    args: list[str] = sys.argv
    n: int = sys.argv
    sys.exit("failed")
    sys.exit(1, 2)
    home: str | None = os.getenv("HOME")
    threads: str = os.environ.get("OMP_NUM_THREADS", "1")
    bad: str = os.getenv("HOME")
    os.getenv(3)
    line = sys.stdin.readline()
    os.environ.setdefault("A", "B")
    return threads
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `i64`, found `list[str]`
 --> main.byn:7:14
  |
7 |     n: int = sys.argv
  |              ^^^^^^^^ the annotation says `i64`

error: expected `i64`, found `str`
 --> main.byn:8:14
  |
8 |     sys.exit("failed")
  |              ^^^^^^^^ an exit status is `i64`

error: `sys.exit` takes one status
 --> main.byn:9:5
  |
9 |     sys.exit(1, 2)
  |     ^^^^^^^^^^^^^^ wrong arguments
  = note: for example `sys.exit(2)`, an `int` from 0 to 255

error: this value may be `None`
  --> main.byn:12:16
   |
12 |     bad: str = os.getenv("HOME")
   |                ^^^^^^^^^^^^^^^^^ has type `str | None`
   = note: give it a name and check it first: `if value is not None:`

error: expected `str`, found `i64`
  --> main.byn:13:15
   |
13 |     os.getenv(3)
   |               ^ a variable's name and default are `str`

error: `sys.stdin` is not in Baryon yet
  --> main.byn:14:16
   |
14 |     line = sys.stdin.readline()
   |                ^^^^^ not built
   = note: so far `sys` has `argv` and `exit`, and `os` has `getenv` and `environ.get` (decision 0036)

error: `os.environ.setdefault` is not in Baryon yet
  --> main.byn:15:5
   |
15 |     os.environ.setdefault("A", "B")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built
   = note: so far `sys` has `argv` and `exit`, and `os` has `getenv` and `environ.get` (decision 0036)"#
    );
}
