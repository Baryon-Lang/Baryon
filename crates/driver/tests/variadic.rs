//! Variadic generics (the decision 0026 amendment, roadmap 3.19.2), through the type checker:
//! `*Ts` in a function's type parameters, unpacked at the end of a tuple or a `Callable`'s
//! parameters, and `fun(t, *args)` on a tuple. Every rejected form is tested by its full
//! rendered diagnostic.
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

/// `SciPy`'s shape: a callback with extra parameters, given by a tuple or left out, a generic
/// function passing its `args` on to another, and a tuple of known type spread into a
/// `Callable` outside any generic function.
#[test]
fn variadic_callbacks_check_clean() {
    let program = r"
def apply[*Ts](f: Callable[[f64, *Ts], f64], x: f64, args: tuple[*Ts] = ()) -> f64:
    return f(x, *args)


def twice[*Ts](f: Callable[[f64, *Ts], f64], x: f64, args: tuple[*Ts] = ()) -> f64:
    return apply(f, apply(f, x, args), args)


def call(f: Callable[[f64, f64, f64], f64], t: tuple[f64, f64]) -> f64:
    return f(1.0, *t)


def double(x: f64) -> f64:
    return 2.0 * x


def line(x: f64, slope: f64, offset: f64) -> f64:
    return slope * x + offset


def use_them() -> f64:
    return apply(double, 2.0) + twice(line, 1.0, (2.0, 0.5)) + call(line, (2.0, 0.5))
";
    assert_eq!(check_one(program), "");
}

/// Each declaration the amendment leaves out: two packs, a pack on a class, `Ts` without its
/// star, `*Ts` before another type, a star on a type parameter that is not a pack, and a
/// default other than `()`.
#[test]
fn misdeclared_packs_are_errors() {
    let program = r"
def two[*Ts, *Us](x: tuple[*Ts], y: tuple[*Us]) -> f64:
    return 0.0


def bare[*Ts](x: Ts) -> f64:
    return 0.0


def middle[*Ts](f: Callable[[*Ts, f64], f64]) -> f64:
    return 0.0


def not_pack[T](x: tuple[*T]) -> f64:
    return 0.0


def bad_default[*Ts](args: tuple[*Ts] = (1.0,)) -> f64:
    return 0.0


class Box[*Ts](Struct):
    x: f64
";
    assert_eq!(
        check_one(program),
        "\
error: a function has at most one variadic type parameter
 --> main.byn:1:14
  |
1 | def two[*Ts, *Us](x: tuple[*Ts], y: tuple[*Us]) -> f64:
  |              ^^^ a second `*Ts`
  = note: one `*Ts` takes every type left; pass the rest in one tuple

error: `Ts` stands for a list of types, not one type
 --> main.byn:5:18
  |
5 | def bare[*Ts](x: Ts) -> f64:
  |                  ^^ a variadic type parameter
  = note: unpack it at the end of a tuple or a `Callable`'s parameters: `tuple[*Ts]` or `Callable[[f64, *Ts], f64]`

error: `*Ts` comes last
 --> main.byn:9:30
  |
9 | def middle[*Ts](f: Callable[[*Ts, f64], f64]) -> f64:
  |                              ^^^ not the last type
  = note: a variadic type parameter stands for the types left at the end: `Callable[[f64, *Ts], f64]`

error: `*T` unpacks only a variadic type parameter
  --> main.byn:13:26
   |
13 | def not_pack[T](x: tuple[*T]) -> f64:
   |                          ^^ not declared `*Ts`
   = note: declare it among the type parameters, `def solve[*Ts](...)`, then write `tuple[*Ts]` or `Callable[[f64, *Ts], f64]`

error: a `tuple[*Ts]` parameter defaults only to `()`
  --> main.byn:17:41
   |
17 | def bad_default[*Ts](args: tuple[*Ts] = (1.0,)) -> f64:
   |                                         ^^^^^^ not `()`
   = note: leaving the argument out passes no extra arguments: `args: tuple[*Ts] = ()`

error: a class cannot have a variadic type parameter
  --> main.byn:21:11
   |
21 | class Box[*Ts](Struct):
   |           ^^^ `*Ts` on a class
   = note: only a function takes `*Ts`, for a callback's extra arguments"
    );
}

/// A star anywhere but on a tuple passed last to a `Callable`, `args` that do not match the
/// callback's extra parameters, and `args` left out for a callback that wants some: leaving it
/// out makes `Ts` empty.
#[test]
fn misplaced_stars_and_mismatched_args_are_errors() {
    let program = r"
def apply[*Ts](f: Callable[[f64, *Ts], f64], x: f64, args: tuple[*Ts] = ()) -> f64:
    return f(x, *args)


def line(x: f64, slope: f64) -> f64:
    return slope * x


def first(f: Callable[[f64, f64], f64], xs: list[f64], t: tuple[f64]) -> f64:
    return line(*t) + f(*xs) + f(*t, 1.0)


def use_them() -> f64:
    return apply(line, 1.0, (2.0, 3.0)) + apply(line, 1.0)
";
    assert_eq!(
        check_one(program),
        "\
error: `*` unpacking in a call is not supported here
  --> main.byn:10:17
   |
10 |     return line(*t) + f(*xs) + f(*t, 1.0)
   |                 ^^
   = note: only a tuple, as the last argument of a call to a `Callable` value, is unpacked: `fun(t, y, *args)`; pass each other argument by position or by name

error: `*` unpacks a tuple, and `xs` is a `list[f64]`
  --> main.byn:10:26
   |
10 |     return line(*t) + f(*xs) + f(*t, 1.0)
   |                          ^^ not a tuple
   = note: pass a tuple, `args=(10.0, 28.0)`, or each argument by position

error: `*` unpacking in a call is not supported here
  --> main.byn:10:34
   |
10 |     return line(*t) + f(*xs) + f(*t, 1.0)
   |                                  ^^
   = note: only a tuple, as the last argument of a call to a `Callable` value, is unpacked: `fun(t, y, *args)`; pass each other argument by position or by name

error: `args` has type `tuple[f64]` here, found `tuple[f64, f64]`
  --> main.byn:14:29
   |
14 |     return apply(line, 1.0, (2.0, 3.0)) + apply(line, 1.0)
   |                             ^^^^^^^^^^ type `tuple[f64, f64]`

error: `f` has type `Callable[[f64], f64]` here, found `Callable[[f64, f64], f64]`
  --> main.byn:14:49
   |
14 |     return apply(line, 1.0, (2.0, 3.0)) + apply(line, 1.0)
   |                                                 ^^^^ type `Callable[[f64, f64], f64]`"
    );
}
