//! End-to-end tests of lowering errors (roadmap 1.9.3; decisions 0005 and 0018): error edges
//! on calls that can fail, `raise`, and `try` with its `except` clauses. Each program is parsed,
//! resolved and checked, then each named function is lowered and compared by its full IR text.

use driver::Session;

/// The IR text of each function in `names`, from `program` after its first newline, joined by
/// blank lines. When the program has errors, or lowering gives some, it returns them instead,
/// rendered, so a failing test shows what went wrong.
fn lower(program: &str, names: &[&str]) -> String {
    let mut session = Session::new();
    let program = program.strip_prefix('\n').unwrap_or(program);
    if let Err(error) = session.add_file("main.byn", program) {
        return error.to_string();
    }
    let mut errors = session.check();
    if errors.is_empty() {
        let mut bodies = Vec::new();
        for name in names {
            match session.mir(name) {
                None => return format!("no function `{name}`"),
                Some(Ok(text)) => bodies.push(text),
                Some(Err(found)) => {
                    errors = found;
                    break;
                }
            }
        }
        if errors.is_empty() {
            return bodies.join("\n");
        }
    }
    errors
        .iter()
        .map(|error| diagnostics::render(error, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A call that can fail gets an error edge. With no `try` around it, the edge raises the
/// error again to the caller.
#[test]
fn a_call_that_can_fail_passes_its_error_on() {
    let program = r#"
class ParseError(Exception):
    position: i64


class RangeError(Exception):
    value: f64


class NamedError(Exception):
    name: str


def parse(text: str) -> Raises[f64, ParseError]:
    raise ParseError(0)


def checked(x: f64) -> Raises[f64, ParseError | RangeError]:
    if x < 0.0:
        raise RangeError(x)
    return x


def named(text: str) -> Raises[f64, NamedError]:
    raise NamedError("bad")


def passes_on(text: str) -> Raises[f64, ParseError]:
    return parse(text) + 1.0
"#;
    assert_eq!(
        lower(program, &["passes_on"]),
        r"fn(_1: str) -> f64 {
    let _0: f64; // return
    let _1: str; // parameter text
    let _2: f64;
    let _3: Raises[None, ParseError];

    bb0: {
        _2 = call parse(_1) -> [return: bb2, error _3: bb1];
    }

    bb1: {
        raise move _3.error[0];
    }

    bb2: {
        _0 = Add(copy _2, 1.0_f64);
        return;
    }
}
"
    );
}

/// An `except` that names the error stores it in the variable `as` binds and jumps to its
/// body; both paths join after the `try`.
#[test]
fn an_except_catches_and_binds_the_error() {
    let program = r#"
class ParseError(Exception):
    position: i64


class RangeError(Exception):
    value: f64


class NamedError(Exception):
    name: str


def parse(text: str) -> Raises[f64, ParseError]:
    raise ParseError(0)


def checked(x: f64) -> Raises[f64, ParseError | RangeError]:
    if x < 0.0:
        raise RangeError(x)
    return x


def named(text: str) -> Raises[f64, NamedError]:
    raise NamedError("bad")


def handled(text: str) -> f64:
    try:
        value = parse(text)
    except ParseError as error:
        value = f64(error.position)
    return value
"#;
    assert_eq!(
        lower(program, &["handled"]),
        r"fn(_1: str) -> f64 {
    let _0: f64; // return
    let _1: str; // parameter text
    let _2: f64; // value
    let _3: ParseError; // error
    let _4: Raises[None, ParseError];
    let _5: f64;

    bb0: {
        _2 = call parse(_1) -> [return: bb3, error _4: bb2];
    }

    bb1: {
        _5 = f64(copy _3.0);
        _2 = copy _5;
        goto -> bb4;
    }

    bb2: {
        drop(_3);
        _3 = move _4.error[0];
        goto -> bb1;
    }

    bb3: {
        goto -> bb4;
    }

    bb4: {
        _0 = copy _2;
        drop(_3);
        return;
    }
}
"
    );
}

/// A callee with several error types: a switch on which one arrived catches `ParseError` and
/// passes `RangeError` on.
#[test]
fn several_errors_switch_on_which_arrived() {
    let program = r#"
class ParseError(Exception):
    position: i64


class RangeError(Exception):
    value: f64


class NamedError(Exception):
    name: str


def parse(text: str) -> Raises[f64, ParseError]:
    raise ParseError(0)


def checked(x: f64) -> Raises[f64, ParseError | RangeError]:
    if x < 0.0:
        raise RangeError(x)
    return x


def named(text: str) -> Raises[f64, NamedError]:
    raise NamedError("bad")


def both(x: f64) -> Raises[f64, RangeError]:
    try:
        return checked(x)
    except ParseError:
        return 0.0
"#;
    assert_eq!(
        lower(program, &["both"]),
        r"fn(_1: f64) -> f64 {
    let _0: f64; // return
    let _1: f64; // parameter x
    let _2: ParseError;
    let _3: Raises[None, ParseError | RangeError];
    let _4: i64;

    bb0: {
        _0 = call checked(_1) -> [return: bb3, error _3: bb2];
    }

    bb1: {
        _0 = 0.0_f64;
        drop(_2);
        return;
    }

    bb2: {
        _4 = error_index(_3);
        switch copy _4 -> [0: bb4, 1: bb5, otherwise: bb6];
    }

    bb3: {
        return;
    }

    bb4: {
        drop(_2);
        _2 = move _3.error[0];
        goto -> bb1;
    }

    bb5: {
        raise move _3.error[1];
    }

    bb6: {
        unreachable;
    }
}
"
    );
}

/// A tuple `except` remembers which error it caught, and a bare `raise` raises that one again.
/// Each error moves out before the clause's temporaries are dropped.
#[test]
fn a_bare_raise_raises_the_caught_error_again() {
    let program = r#"
class ParseError(Exception):
    position: i64


class RangeError(Exception):
    value: f64


class NamedError(Exception):
    name: str


def parse(text: str) -> Raises[f64, ParseError]:
    raise ParseError(0)


def checked(x: f64) -> Raises[f64, ParseError | RangeError]:
    if x < 0.0:
        raise RangeError(x)
    return x


def named(text: str) -> Raises[f64, NamedError]:
    raise NamedError("bad")


def again(x: f64) -> Raises[f64, ParseError | RangeError]:
    try:
        return checked(x)
    except (ParseError, RangeError):
        raise
"#;
    assert_eq!(
        lower(program, &["again"]),
        r"fn(_1: f64) -> f64 {
    let _0: f64; // return
    let _1: f64; // parameter x
    let _2: ParseError;
    let _3: RangeError;
    let _4: i64;
    let _5: Raises[None, ParseError | RangeError];
    let _6: i64;
    let _7: ParseError;
    let _8: RangeError;

    bb0: {
        _0 = call checked(_1) -> [return: bb3, error _5: bb2];
    }

    bb1: {
        switch copy _4 -> [0: bb7, 1: bb8, otherwise: bb9];
    }

    bb2: {
        _6 = error_index(_5);
        switch copy _6 -> [0: bb4, 1: bb5, otherwise: bb6];
    }

    bb3: {
        return;
    }

    bb4: {
        drop(_2);
        _2 = move _5.error[0];
        _4 = 0_i64;
        goto -> bb1;
    }

    bb5: {
        drop(_3);
        _3 = move _5.error[1];
        _4 = 1_i64;
        goto -> bb1;
    }

    bb6: {
        unreachable;
    }

    bb7: {
        _7 = move _2;
        drop(_3);
        drop(_2);
        raise move _7;
    }

    bb8: {
        _8 = move _3;
        drop(_3);
        drop(_2);
        raise move _8;
    }

    bb9: {
        unreachable;
    }
}
"
    );
}

/// `raise error` moves the error out of its variable before the variable is dropped, and a
/// `raise` inside a `try` that catches it jumps straight to the `except`.
#[test]
fn raised_errors_move_out_before_drops() {
    let program = r#"
class ParseError(Exception):
    position: i64


class RangeError(Exception):
    value: f64


class NamedError(Exception):
    name: str


def parse(text: str) -> Raises[f64, ParseError]:
    raise ParseError(0)


def checked(x: f64) -> Raises[f64, ParseError | RangeError]:
    if x < 0.0:
        raise RangeError(x)
    return x


def named(text: str) -> Raises[f64, NamedError]:
    raise NamedError("bad")


def rethrow(text: str) -> Raises[f64, NamedError]:
    try:
        return named(text)
    except NamedError as error:
        raise error


def caught_here() -> f64:
    try:
        raise NamedError("odd")
    except NamedError:
        return 1.0
"#;
    assert_eq!(
        lower(program, &["rethrow", "caught_here"]),
        r#"fn(_1: str) -> f64 {
    let _0: f64; // return
    let _1: str; // parameter text
    let _2: NamedError; // error
    let _3: Raises[None, NamedError];
    let _4: NamedError;

    bb0: {
        _0 = call named(_1) -> [return: bb3, error _3: bb2];
    }

    bb1: {
        _4 = move _2;
        drop(_2);
        raise move _4;
    }

    bb2: {
        drop(_2);
        _2 = move _3.error[0];
        goto -> bb1;
    }

    bb3: {
        drop(_2);
        return;
    }
}

fn() -> f64 {
    let _0: f64; // return
    let _1: NamedError;
    let _2: NamedError;

    bb0: {
        _2 = NamedError("odd");
        drop(_1);
        _1 = move _2;
        drop(_2);
        goto -> bb1;
    }

    bb1: {
        _0 = 1.0_f64;
        drop(_1);
        return;
    }
}
"#
    );
}
