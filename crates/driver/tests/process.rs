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
   = note: so far `sys` has `argv` and `exit`, and `os` has `environ`, `getenv`, `listdir`, `remove`, `rename`, `mkdir`, `makedirs`, `getcwd` and `ErrorKind` (decision 0036)

error: `os.environ.setdefault` is not in Baryon yet
  --> main.byn:15:5
   |
15 |     os.environ.setdefault("A", "B")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built
   = note: so far `sys` has `argv` and `exit`, and `os` has `environ`, `getenv`, `listdir`, `remove`, `rename`, `mkdir`, `makedirs`, `getcwd` and `ErrorKind` (decision 0036)"#
    );
}

/// `os.environ[name]`, `name in os.environ` and `os.environ.copy()` read the environment;
/// anything that would change it is a compile error, and so is `os.environ` on its own, which has
/// no type (decision 0036, part 5).
#[test]
fn the_environment_is_read_four_ways_and_never_changed() {
    let program = r#"
import os


def f(name: str) -> str:
    home = os.environ["HOME"]
    env: dict[str, str] = os.environ.copy()
    if name in os.environ and "CI" not in os.environ:
        return os.environ[name]
    os.environ["A"] = "b"
    del os.environ["A"]
    os.putenv("A", "b")
    os.unsetenv("A")
    table = os.environ
    count: int = os.environ[3]
    found: bool = 3 in os.environ
    os.environ.copy(1)
    return home
"#;
    assert_eq!(
        check_one(program),
        r#"error: the environment is read-only
 --> main.byn:9:5
  |
9 |     os.environ["A"] = "b"
  |     ^^^^^^^^^^^^^^^ cannot be changed
  = note: changing it would race with every thread that reads it; to run a program with another environment, pass it one: `subprocess.run(..., env=...)` (decision 0036)

error: the environment is read-only
  --> main.byn:10:9
   |
10 |     del os.environ["A"]
   |         ^^^^^^^^^^^^^^^ cannot be changed
   = note: changing it would race with every thread that reads it; to run a program with another environment, pass it one: `subprocess.run(..., env=...)` (decision 0036)

error: the environment is read-only
  --> main.byn:11:5
   |
11 |     os.putenv("A", "b")
   |     ^^^^^^^^^^^^^^^^^^^ cannot be changed
   = note: changing it would race with every thread that reads it; to run a program with another environment, pass it one: `subprocess.run(..., env=...)` (decision 0036)

error: the environment is read-only
  --> main.byn:12:5
   |
12 |     os.unsetenv("A")
   |     ^^^^^^^^^^^^^^^^ cannot be changed
   = note: changing it would race with every thread that reads it; to run a program with another environment, pass it one: `subprocess.run(..., env=...)` (decision 0036)

error: `os.environ` is not a value on its own
  --> main.byn:13:16
   |
13 |     table = os.environ
   |                ^^^^^^^ read one variable, or copy them all
   = note: the environment is read with `os.environ[name]`, `os.environ.get(name)`, `name in os.environ` and `os.environ.copy()`, which gives a `dict[str, str]` (decision 0036)

error: expected `str`, found `i64`
  --> main.byn:14:29
   |
14 |     count: int = os.environ[3]
   |                             ^ a variable's name is `str`

error: expected `i64`, found `str`
  --> main.byn:14:18
   |
14 |     count: int = os.environ[3]
   |                  ^^^^^^^^^^^^^ the annotation says `i64`

error: expected `str`, found `i64`
  --> main.byn:15:19
   |
15 |     found: bool = 3 in os.environ
   |                   ^ a variable's name is `str`

error: `copy` takes no arguments
  --> main.byn:16:5
   |
16 |     os.environ.copy(1)
   |     ^^^^^^^^^^^^^^^^^^ wrong arguments"#
    );
}

/// `OSError` and `os.ErrorKind` are classes of the standard library's own source (decision
/// 0038): their fields and members are checked like a program's, and a name the class does not
/// have is an error.
#[test]
fn os_error_and_error_kind_are_library_classes() {
    let program = r#"
import os
from os import ErrorKind


def describe(error: OSError) -> str:
    if error.kind == ErrorKind.FILE_NOT_FOUND:
        return "no " + error.filename
    code: i32 = error.errno
    return error.message


def fail() -> Raises[None, OSError]:
    raise OSError(os.ErrorKind.PERMISSION, 13, "Permission denied", "data.txt")
"#;
    assert_eq!(
        check_one(program),
        r"error: `OSError` has no field or method `message`
 --> main.byn:9:12
  |
9 |     return error.message
  |            ^^^^^^^^^^^^^ not defined on this struct"
    );
}
