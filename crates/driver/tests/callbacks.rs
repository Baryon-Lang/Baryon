//! Callbacks that version 0 leaves out (decision 0026, roadmap 1.9.41), end to end through the
//! type checker: each is a checker error at the expression or annotation that names the
//! record, where before it passed the checker and failed in lowering or the backend.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
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

/// A function that takes a callback, and a struct with a method, a function of the class and
/// `__call__`, shared by the tests below.
const SHARED: &str = r"
def apply(f: Callable[[f64], f64], x: f64) -> f64:
    return f(x)


class Scale(Struct, Copy):
    k: f64

    def at(self, x: f64) -> f64:
        return self.k * x

    def double(x: f64) -> f64:
        return 2.0 * x

    def __call__(self, x: f64) -> f64:
        return self.k * x


def twice[T: Float](x: T) -> T:
    return x + x
";

/// A bound method, a function of a class and a generic function read as a value are reported.
/// A struct value called through `__call__` (decision 0058), the method called by name, and a
/// module-level function passed check clean.
#[test]
fn methods_and_generic_functions_as_values_are_reported() {
    let program = format!(
        "{SHARED}{}",
        r"

def half(x: f64) -> f64:
    return x / 2.0


def main():
    s = Scale(2.0)
    print(apply(s.at, 1.0))
    print(apply(Scale.double, 1.0))
    print(apply(twice, 1.0))
    print(s(3.0))
    print(s.__call__(3.0), apply(half, 1.0), twice(1.0))
"
    );
    assert_eq!(
        check_one(&program),
        "\
error: the method `at` cannot be used as a value in version 0
  --> main.byn:28:17
   |
28 |     print(apply(s.at, 1.0))
   |                 ^^^^ a method, not called
   = note: only a module-level function can be passed as a `Callable` (decision 0026); call the method inside one, or make the type implement a trait and pass the value

error: the method `double` cannot be used as a value in version 0
  --> main.byn:29:17
   |
29 |     print(apply(Scale.double, 1.0))
   |                 ^^^^^^^^^^^^ a method, not called
   = note: only a module-level function can be passed as a `Callable` (decision 0026); call the method inside one, or make the type implement a trait and pass the value

error: the generic function `twice` cannot be used as a value in version 0
  --> main.byn:30:17
   |
30 |     print(apply(twice, 1.0))
   |                 ^^^^^ a generic function, not called
   = note: its type arguments cannot be worked out from a `Callable` yet (decision 0026); pass a function that is not generic, which may call this one"
    );
}

/// A struct with `__call__` passed to a `Callable` parameter is an error whose note names
/// decisions 0026 and 0058, and calling a struct without `__call__` is an error that says what
/// a struct needs to be called.
#[test]
fn callable_structs_are_not_function_values() {
    let program = format!(
        "{SHARED}{}",
        r"

class Plain(Struct, Copy):
    k: f64


def main():
    s = Scale(2.0)
    print(apply(s, 1.0))
    p = Plain(1.0)
    print(p(2.0))
"
    );
    assert_eq!(
        check_one(&program),
        "\
error: `f` has type `Callable[[f64], f64]`, found `Scale`
  --> main.byn:28:17
   |
28 |     print(apply(s, 1.0))
   |                 ^ type `Scale`
   = note: a struct with `__call__` can be called, but a `Callable` takes only a module-level function (decisions 0026 and 0058); pass a function that calls it

error: `p` has type `Plain`, which cannot be called
  --> main.byn:30:11
   |
30 |     print(p(2.0))
   |           ^^^^^^ not a function
   = note: a struct is called through a `__call__` method, which this one does not define"
    );
}

/// A `Callable` in a struct field, inside a parameter's type, and as a return type.
#[test]
fn stored_callables_are_reported() {
    let program = r"
class Problem(Struct):
    f: Callable[[f64], f64]


def newton(f: Callable[[f64], f64], jac: Callable[[f64], f64] | None = None) -> f64:
    return f(1.0)


def many(fs: list[Callable[[f64], f64]]) -> f64:
    return 0.0


def pick(x: f64) -> Callable[[f64], f64]:
    return pick
";
    assert_eq!(
        check_one(program),
        "\
error: a struct field cannot hold a `Callable` in version 0
 --> main.byn:2:8
  |
2 |     f: Callable[[f64], f64]
  |        ^^^^^^^^^^^^^^^^^^^^ a `Callable` kept as a value
  = note: a `Callable` is a parameter type only (decision 0026); pass the function to each function that calls it, or give the struct a method

error: a parameter of type `Callable[[f64], f64] | None` is not supported in version 0
 --> main.byn:5:42
  |
5 | def newton(f: Callable[[f64], f64], jac: Callable[[f64], f64] | None = None) -> f64:
  |                                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^ a `Callable` kept as a value
  = note: a `Callable` must be a parameter's whole type, not optional or inside another type (decision 0026); write a second function for the case without it

error: a parameter of type `list[Callable[[f64], f64]]` is not supported in version 0
 --> main.byn:9:14
  |
9 | def many(fs: list[Callable[[f64], f64]]) -> f64:
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^ a `Callable` kept as a value
  = note: a `Callable` must be a parameter's whole type, not optional or inside another type (decision 0026); write a second function for the case without it

error: a function cannot return a `Callable` in version 0
  --> main.byn:13:21
   |
13 | def pick(x: f64) -> Callable[[f64], f64]:
   |                     ^^^^^^^^^^^^^^^^^^^^ a `Callable` kept as a value
   = note: a `Callable` is a parameter type only (decision 0026); return the value the function computes, or pass the function on as an argument"
    );
}
