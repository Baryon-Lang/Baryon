//! End-to-end tests of lowering `match` (roadmap 1.9.3): enum members, literals, guards,
//! `None`, tuples and `|`. Each program is parsed, resolved and checked, then each named
//! function is lowered and compared by its full IR text.

use driver::Session;

/// The IR text of each function in `names`, from `program` after its first newline, joined by
/// blank lines. When the program has errors, or lowering gives some, it returns them instead,
/// rendered, so a failing test shows what went wrong.
fn lower(program: &str, names: &[&str]) -> String {
    let mut session = Session::new();
    let program = program.strip_prefix('\n').unwrap_or(program);
    if let Err(error) = session.add_file("main.npl", program) {
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

/// Each enum member in a pattern is a switch on that member; `a | b` tries each in turn and
/// joins where the case's body starts.
#[test]
fn enum_members_switch_on_the_member() {
    let program = r"
class Boundary(Enum):
    PERIODIC = 0
    REFLECTING = 1
    ABSORBING = 2


def weight(boundary: Boundary) -> f64:
    match boundary:
        case Boundary.PERIODIC:
            return 1.0
        case Boundary.REFLECTING | Boundary.ABSORBING:
            return 0.5
";
    assert_eq!(
        lower(program, &["weight"]),
        r"fn(_1: Boundary) -> f64 {
    let _0: f64; // return
    let _1: Boundary; // parameter boundary

    bb0: {
        switch copy _1 -> [0: bb2, otherwise: bb1];
    }

    bb1: {
        switch copy _1 -> [1: bb6, otherwise: bb5];
    }

    bb2: {
        _0 = 1.0_f64;
        return;
    }

    bb3: {
        unreachable;
    }

    bb4: {
        _0 = 0.5_f64;
        return;
    }

    bb5: {
        switch copy _1 -> [2: bb7, otherwise: bb3];
    }

    bb6: {
        goto -> bb4;
    }

    bb7: {
        goto -> bb4;
    }
}
"
    );
}

/// A literal pattern compares for equality; a capture is assigned before its guard is tested,
/// and a failed guard goes on to the next case.
#[test]
fn literals_compare_and_guards_branch() {
    let program = r"
def classify(n: i64) -> i64:
    match n:
        case 0:
            return 0
        case k if k < 0:
            return -1
        case _:
            return 1
";
    assert_eq!(
        lower(program, &["classify"]),
        r"fn(_1: i64) -> i64 {
    let _0: i64; // return
    let _1: i64; // parameter n
    let _2: i64; // k
    let _3: bool;
    let _4: bool;

    bb0: {
        _3 = Equal(copy _1, 0_i64);
        branch copy _3 -> [true: bb2, false: bb1];
    }

    bb1: {
        _2 = copy _1;
        _4 = Less(copy _2, 0_i64);
        branch copy _4 -> [true: bb4, false: bb3];
    }

    bb2: {
        _0 = 0_i64;
        return;
    }

    bb3: {
        _0 = 1_i64;
        return;
    }

    bb4: {
        _0 = -1_i64;
        return;
    }

    bb5: {
        unreachable;
    }
}
"
    );
}

/// `case None:` tests whether the optional holds a value, and a later capture reads the value
/// inside; a tuple pattern tests each item in order.
#[test]
fn none_and_tuple_patterns_test_each_part() {
    let program = r"
def maybe(x: f64 | None) -> f64:
    match x:
        case None:
            return 0.0
        case value:
            return value


def pair(p: tuple[i64, bool]) -> i64:
    match p:
        case (0, True):
            return 1
        case (n, _):
            return n
";
    assert_eq!(
        lower(program, &["maybe", "pair"]),
        r"fn(_1: f64 | None) -> f64 {
    let _0: f64; // return
    let _1: f64 | None; // parameter x
    let _2: f64; // value
    let _3: bool;

    bb0: {
        _3 = is_present(_1);
        branch copy _3 -> [true: bb1, false: bb2];
    }

    bb1: {
        _2 = copy _1.value;
        _0 = copy _2;
        return;
    }

    bb2: {
        _0 = 0.0_f64;
        return;
    }

    bb3: {
        unreachable;
    }
}

fn(_1: tuple[i64, bool]) -> i64 {
    let _0: i64; // return
    let _1: tuple[i64, bool]; // parameter p
    let _2: i64; // n
    let _3: bool;

    bb0: {
        _3 = Equal(copy _1.0, 0_i64);
        branch copy _3 -> [true: bb2, false: bb1];
    }

    bb1: {
        _2 = copy _1.0;
        _0 = copy _2;
        return;
    }

    bb2: {
        branch copy _1.1 -> [true: bb3, false: bb1];
    }

    bb3: {
        _0 = 1_i64;
        return;
    }

    bb4: {
        unreachable;
    }
}
"
    );
}

/// Capturing a value that owns memory would move part of the subject, which waits on the move
/// rules for patterns.
#[test]
fn captures_of_owning_values_are_not_lowered_yet() {
    let program = r"
def first(names: tuple[str, i64]) -> i64:
    match names:
        case (name, count):
            return count
";
    assert_eq!(
        lower(program, &["first"]),
        r"error: lowering does not handle capturing a value that is not a copy type yet
 --> main.npl:3:15
  |
3 |         case (name, count):
  |               ^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}
