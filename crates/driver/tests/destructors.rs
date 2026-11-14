//! Checking `__del__` (decision 0063), end to end: programs that give a class a `__del__` it
//! cannot have, call one by name, or move a field out of a value whose class has one go through
//! the parser, the syntax check, the resolver and the checker, and the rendered diagnostics
//! are compared in full.

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

/// A `__del__` with another shape, one on a `Copy` class and one on a generic class are
/// reported where each is defined.
#[test]
fn only_one_shape_of_del_is_allowed() {
    let program = r#"
class Returns(Struct):
    name: str

    def __del__(self: InOut[Self]) -> int:
        return 1


class Borrows(Struct):
    name: str

    def __del__(self):
        print(self.name)


class Point(Struct, Copy):
    x: f64

    def __del__(self: InOut[Self]):
        print(self.x)


class Box[T](Struct):
    item: T

    def __del__(self: InOut[Self]):
        print("box")


def main():
    print("main")
"#;
    assert_eq!(
        check_one(program),
        r"error: `Returns.__del__` must take only `self: InOut[Self]` and return `None`
 --> main.byn:4:9
  |
4 |     def __del__(self: InOut[Self]) -> int:
  |         ^^^^^^^ runs when the value is dropped
  = note: write `def __del__(self: InOut[Self]):`, or `-> Raises[None, E]` when it can fail; an error it raises panics, since a drop cannot raise (decision 0063)

error: `Borrows.__del__` must take only `self: InOut[Self]` and return `None`
  --> main.byn:11:9
   |
11 |     def __del__(self):
   |         ^^^^^^^ runs when the value is dropped
   = note: write `def __del__(self: InOut[Self]):`, or `-> Raises[None, E]` when it can fail; an error it raises panics, since a drop cannot raise (decision 0063)

error: `Point` is declared `Copy`, so it cannot have `__del__`
  --> main.byn:18:9
   |
18 |     def __del__(self: InOut[Self]):
   |         ^^^^^^^ never runs
   = note: a copy type is copied, never dropped; remove `Copy` to give the class a `__del__` (decision 0063)

error: `__del__` on a generic class, `Box`, is not built yet
  --> main.byn:25:9
   |
25 |     def __del__(self: InOut[Self]):
   |         ^^^^^^^ not built yet
   = note: give the class a method such as `close()` and call it, or use `with` (decision 0063)"
    );
}

/// Calling `__del__` by name and moving a field out of a value whose class has one are errors;
/// copying a field of a copy type and moving with `.copy()` are not.
#[test]
fn del_is_never_called_and_its_fields_stay() {
    let program = r#"
class Log(Struct):
    name: str
    count: int

    def __del__(self: InOut[Self]):
        print(self.name)


def main():
    log = Log("a", 1)
    log.__del__()
    kept = log.name
    copied = log.name.copy()
    count = log.count
    print(copied, count)
"#;
    assert_eq!(
        check_one(program),
        r"error: `__del__` is called when the value is dropped, never by name
  --> main.byn:11:5
   |
11 |     log.__del__()
   |     ^^^^^^^^^^^ called here
   = note: calling it here would run it twice; put the work in a method such as `close()`, call that, and call it from `__del__` too (decision 0063)

error: a field cannot be moved out of a value whose class has `__del__`
  --> main.byn:12:12
   |
12 |     kept = log.name
   |            ^^^^^^^^ moved out here
   = note: `__del__` runs on the whole value when it is dropped (decision 0063); move a copy instead: `log.name.copy()`"
    );
}
