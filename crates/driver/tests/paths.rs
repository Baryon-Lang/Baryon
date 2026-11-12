//! Checking `pathlib.Path` and the directory functions of `os` (decision 0036, parts 1 and 7),
//! end to end: programs go through the parser, the syntax check, the resolver and the checker,
//! and the rendered diagnostics are compared in full.

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

/// A `Path` is made from one `str` or `Path`, joined with `/`, compared with `==`, copied, and
/// read through its parts and methods, the ones that touch the file system failing with `OSError`;
/// `open` takes one. A method Python's `Path` has and this one does not is reported with what it
/// has.
#[test]
fn paths_are_checked() {
    let program = r#"
from pathlib import Path


def good(root: Path, name: str) -> Raises[str, OSError]:
    out = Path("runs") / name / "out.csv"
    same: bool = out == root / "out.csv"
    kept: Path = out.copy()
    parts: str = out.name + out.stem + out.suffix + str(out.parent) + f"{kept}"
    if out.exists() and not out.is_dir() and out.is_file():
        written: int = out.write_text(parts, encoding="utf-8")
    entries: list[Path] = root.iterdir()
    with open(out) as f:
        return f.read() + out.read_text()


def bad(path: Path):
    Path(1)
    Path("a", "b")
    path.unlink()
    path.exists(1)
    path.write_text()
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `str`, found `i64`
  --> main.byn:17:10
   |
17 |     Path(1)
   |          ^ a path is a `Path` or a `str`

error: `Path` takes one path
  --> main.byn:18:5
   |
18 |     Path("a", "b")
   |     ^^^^^^^^^^^^^^ wrong arguments
   = note: join more with `/`: `Path("runs") / name / "out.csv"`

error: `Path` has no method `unlink`
  --> main.byn:19:5
   |
19 |     path.unlink()
   |     ^^^^^^^^^^^^^ not defined
   = note: a `Path` has `name`, `stem`, `suffix` and `parent`, and the methods `exists`, `is_file`, `is_dir`, `iterdir`, `read_text`, `write_text` and `copy` (decision 0036)

error: `exists` takes no arguments
  --> main.byn:20:5
   |
20 |     path.exists(1)
   |     ^^^^^^^^^^^^^^ wrong arguments

error: `write_text` takes one `str`
  --> main.byn:21:5
   |
21 |     path.write_text()
   |     ^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `path.write_text(f"{energy}\n")`"#
    );
}

/// Joining a number, an encoding other than UTF-8, assigning a part, showing a `Path` inside
/// another value, reading a method as a value and reading the struct's own field are errors, and
/// so is a failure no `try` or `Raises` handles.
#[test]
fn path_misuse_is_reported() {
    let program = r#"
from pathlib import Path


class Run(Struct):
    out: Path


def bad(path: Path, run: Run) -> Raises[str, OSError]:
    joined = path / 3
    path.read_text(encoding="latin-1")
    path.name = "x"
    print([path.copy()], run)
    text: str = path.exists
    return path._text


def unhandled(path: Path):
    path.read_text()
"#;
    assert_eq!(
        check_one(program),
        r#"error: `/` joins a `Path` with a `Path` or a `str`, found `i64`
 --> main.byn:9:14
  |
9 |     joined = path / 3
  |              ^^^^^^^^ not a path
  = note: write the name as text, such as `path / f"step_{i}.csv"`

error: text files are UTF-8
  --> main.byn:10:29
   |
10 |     path.read_text(encoding="latin-1")
   |                             ^^^^^^^^^ not UTF-8
   = note: leave `encoding` out, or write `encoding="utf-8"`

error: a part of a `Path` cannot be assigned
  --> main.byn:11:5
   |
11 |     path.name = "x"
   |     ^^^^^^^^^ computed from the path
   = note: make a new path instead, such as `path.parent / "out.csv"`

error: a `Path` inside `list[Path]` cannot be shown yet
  --> main.byn:12:11
   |
12 |     print([path.copy()], run)
   |           ^^^^^^^^^^^^^ holds a `Path`
   = note: show each path on its own, `print(path)`, or keep its text: `str(path)`

error: a `Path` inside `Run` cannot be shown yet
  --> main.byn:12:26
   |
12 |     print([path.copy()], run)
   |                          ^^^ holds a `Path`
   = note: show each path on its own, `print(path)`, or keep its text: `str(path)`

error: `Path.exists` is a method
  --> main.byn:13:17
   |
13 |     text: str = path.exists
   |                 ^^^^^^^^^^^ not called
   = note: call it: `path.exists()`

error: `Path` has no attribute `_text`
  --> main.byn:14:12
   |
14 |     return path._text
   |            ^^^^^^^^^^ not defined
   = note: a `Path` has `name`, `stem`, `suffix` and `parent`, and the methods `exists`, `is_file`, `is_dir`, `iterdir`, `read_text`, `write_text` and `copy` (decision 0036)

error: `path.read_text` can fail with `OSError`, which is not handled here
  --> main.byn:18:5
   |
18 |     path.read_text()
   |     ^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}

/// The directory functions of `os` take a `str` or a `Path` and fail with `OSError`; wrong
/// arguments are reported with an example, and a function this slice does not build by name.
#[test]
fn directory_functions_are_checked() {
    let program = r#"
import os
from pathlib import Path


def good(root: Path) -> Raises[list[str], OSError]:
    os.mkdir(root)
    os.makedirs(root / "a" / "b", exist_ok=True)
    os.rename("old.txt", root / "new.txt")
    os.remove(root / "new.txt")
    here: str = os.getcwd()
    return os.listdir() + os.listdir(root)


def bad():
    os.listdir(3)
    os.makedirs("a", exist_ok=1)
    os.makedirs("a", mode=7)
    os.getcwd(1)
    os.rename("a")
    os.mkdir("x")
    os.chdir("x")
"#;
    assert_eq!(
        check_one(program),
        r#"error: expected `str`, found `i64`
  --> main.byn:15:16
   |
15 |     os.listdir(3)
   |                ^ a path is a `Path` or a `str`

error: `os.listdir` can fail with `OSError`, which is not handled here
  --> main.byn:15:5
   |
15 |     os.listdir(3)
   |     ^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: expected `bool`, found `i64`
  --> main.byn:16:31
   |
16 |     os.makedirs("a", exist_ok=1)
   |                               ^ `exist_ok` is `bool`

error: `os.makedirs` can fail with `OSError`, which is not handled here
  --> main.byn:16:5
   |
16 |     os.makedirs("a", exist_ok=1)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `os.makedirs` does not take these arguments
  --> main.byn:17:5
   |
17 |     os.makedirs("a", mode=7)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `os.makedirs("runs/2026", exist_ok=True)`

error: `os.getcwd` does not take these arguments
  --> main.byn:18:5
   |
18 |     os.getcwd(1)
   |     ^^^^^^^^^^^^ wrong arguments
   = note: for example `os.getcwd()`

error: `os.rename` does not take these arguments
  --> main.byn:19:5
   |
19 |     os.rename("a")
   |     ^^^^^^^^^^^^^^ wrong arguments
   = note: for example `os.rename("out.tmp", "out.csv")`

error: `os.mkdir` can fail with `OSError`, which is not handled here
  --> main.byn:20:5
   |
20 |     os.mkdir("x")
   |     ^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `os.chdir` is not in Baryon yet
  --> main.byn:21:5
   |
21 |     os.chdir("x")
   |     ^^^^^^^^^^^^^ not built
   = note: so far `sys` has `argv`, `exit`, `stdin`, `stdout` and `stderr`, and `os` has `environ`, `getenv`, `listdir`, `remove`, `rename`, `mkdir`, `makedirs`, `getcwd` and `ErrorKind` (decision 0036)"#
    );
}
