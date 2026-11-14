//! Checking context managers (decision 0053), end to end: programs that misuse a `with` block
//! of a class with `__enter__` and `__exit__` go through the parser, the syntax check, the
//! resolver and the checker, and the rendered diagnostics are compared in full.

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

/// The name after `as` stays put in the block: moving it, a field of it or assigning it is an
/// error, and so is reading it after an `__exit__` that takes it as `Owned`.
#[test]
fn held_names_stay_put() {
    let program = r#"
class Section(Struct):
    name: str

    def __enter__(self: Owned[Self]) -> Self:
        return self

    def __exit__(self: Owned[Self]):
        print(self.name)


def main():
    with Section("a") as log:
        keep = log
        log = Section("b")
        name = log.name
    print(log.name)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `log` is held by a `with` block, so it cannot be moved
  --> main.byn:13:16
   |
13 |         keep = log
   |                ^^^ moved here
  ::: main.byn:12:26
   |
12 |     with Section("a") as log:
   |                          --- `__exit__` is called on `log` when the block ends
   = note: keep a copy instead: `log.copy()`

error: `log` is held by a `with` block, so it cannot be assigned
  --> main.byn:14:9
   |
14 |         log = Section("b")
   |         ^^^ assigned here
  ::: main.byn:12:26
   |
12 |     with Section("a") as log:
   |                          --- `__exit__` is called on `log` when the block ends
   = note: give the new value another name

error: `log` is held by a `with` block, so it cannot be moved
  --> main.byn:15:16
   |
15 |         name = log.name
   |                ^^^^^^^^ moved here
  ::: main.byn:12:26
   |
12 |     with Section("a") as log:
   |                          --- `__exit__` is called on `log` when the block ends
   = note: keep a copy instead: `log.name.copy()`

error: `log` is read after its `with` block ended
  --> main.byn:16:11
   |
16 |     print(log.name)
   |           ^^^ read after the block
  ::: main.byn:12:26
   |
12 |     with Section("a") as log:
   |                          --- `__exit__` takes `log` as `Owned` when the block ends
   = note: read what you need from it inside the block (decision 0053)"#
    );
}

/// A class with one dunder, an `__enter__` that borrows, Python's three-argument `__exit__`, a
/// value that is neither a file nor a manager, and a tuple target are each explained.
#[test]
fn misused_managers_are_explained() {
    let program = r#"
class Half(Struct):
    name: str

    def __enter__(self: Owned[Self]) -> Self:
        return self


class Borrowing(Struct):
    name: str

    def __enter__(self) -> Self:
        return self.copy()

    def __exit__(self):
        pass


class Pythonic(Struct):
    name: str

    def __enter__(self: Owned[Self]) -> Self:
        return self

    def __exit__(self, kind: int, value: int, trace: int):
        pass


def main():
    with Half("h") as h:
        pass
    with Borrowing("b") as b:
        pass
    with Pythonic("p") as p:
        pass
    with 3:
        pass
    with Half("c") as (a, b):
        pass
"#;
    assert_eq!(
        check_one(program),
        r#"error: the target of `with ... as` must be a single name
  --> main.byn:37:23
   |
37 |     with Half("c") as (a, b):
   |                       ^^^^^^ not a name
   = note: unpack the value on the next line: `with m as pair:` then `a, b = pair`

error: `Half` needs both `__enter__` and `__exit__` for `with`
  --> main.byn:29:10
   |
29 |     with Half("h") as h:
   |          ^^^^^^^^^ not a context manager
   = note: define `def __enter__(self: Owned[Self]) -> Self` and `def __exit__(self)` (decision 0053)

error: `Borrowing.__enter__` must take `self: Owned[Self]` and return `Self` for `with`
  --> main.byn:31:10
   |
31 |     with Borrowing("b") as b:
   |          ^^^^^^^^^^^^^^ used by this `with`
   = note: the value moves into the block, and `__exit__` is called on the one `__enter__` returns: `return self` (decision 0053)

error: `Pythonic.__exit__` must take only `self` and return `None` for `with`
  --> main.byn:33:10
   |
33 |     with Pythonic("p") as p:
   |          ^^^^^^^^^^^^^ used by this `with`
   = note: `__exit__` cannot see or suppress an error; handle errors with `try` around the `with` (decision 0053)

error: `with` needs a `File`, found `i64`
  --> main.byn:35:10
   |
35 |     with 3:
   |          ^ not a file
   = note: `with` is for `with open(...) as f:`, `with unsafe:`, and a class that defines `__enter__` and `__exit__` (decisions 0036 and 0053)

error: `Half` needs both `__enter__` and `__exit__` for `with`
  --> main.byn:37:10
   |
37 |     with Half("c") as (a, b):
   |          ^^^^^^^^^ not a context manager
   = note: define `def __enter__(self: Owned[Self]) -> Self` and `def __exit__(self)` (decision 0053)"#
    );
}
