//! Errors and panics in built programs: errors raised, passed on and caught, an error that ends
//! `main`, and the backtrace a panic prints.

use super::{BACKTRACE, baryon_build, package, panic_report, stderr};
use std::process::Command;

/// Errors end to end (decision 0005): functions that raise, a call whose error passes on to its
/// caller, and `except` clauses that catch each error type and read its field. The last
/// `assert` fails on purpose, so the test sees every earlier one pass.
#[test]
fn errors_are_raised_passed_on_and_caught() {
    let program = "\
class ParseError(Exception):
    position: i64


class RangeError(Exception):
    value: f64


def parse(x: i64) -> Raises[f64, ParseError]:
    if x < 0:
        raise ParseError(x)
    return f64(x) / 2.0


def checked(x: f64) -> Raises[f64, ParseError | RangeError]:
    if x > 100.0:
        raise RangeError(x)
    return x


def chain(x: i64) -> Raises[f64, ParseError | RangeError]:
    return checked(parse(x))


def handled(x: i64) -> f64:
    try:
        value = chain(x)
    except ParseError as error:
        value = f64(error.position)
    except RangeError as problem:
        value = -problem.value
    return value


def main():
    assert handled(10) == 5.0
    assert handled(-3) == -3.0
    assert handled(400) == -200.0
    assert handled(4) == 3.0, \"last\"
";
    let directory = package("errors", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: handled(4) == 3.0, \"last\"\n  --> main.byn:39:5\n"
    );
}

/// An error that reaches the end of `main` is printed to standard error with its type and fields,
/// and the program exits with status 1 (the reference, "Errors"). The second error of the list
/// is raised, so the report picks it by its position.
#[test]
fn errors_that_end_main_are_reported() {
    let program = "\
class ConfigError(Exception):
    line: i64
    strict: bool


class LimitError(Exception):
    code: u8
    limit: f64


def load(n: i64) -> Raises[f64, ConfigError | LimitError]:
    if n < 0:
        raise ConfigError(n, True)
    if n > 10:
        raise LimitError(u8(n), 10.0)
    return f64(n)


def main() -> Raises[None, ConfigError | LimitError]:
    print(\"loaded\", load(3))
    print(\"loaded\", load(12))
    print(\"unreachable\")
";
    let directory = package("main_raises", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "loaded 3.0\n");
    assert_eq!(stderr(&ran), "error: LimitError(code=12, limit=10.0)\n");
    assert_eq!(ran.status.code(), Some(1));
}

/// A panic ends its report with the `.byn` frames that led to it, most recent last, each with
/// its function and source position, read from the line tables (roadmap 1.12.3). Inlined calls
/// keep their frames, and the runtime's own frames are left out.
#[test]
fn panics_print_a_backtrace_of_baryon_frames() {
    let program = "\
def pick(values: Array[f64, 1], i: i64) -> f64:
    return values[i]


def total(n: i64) -> f64:
    values: Array[f64, 1] = Array.zeros(n)
    return pick(values, n)


def main():
    print(total(4))
";
    let directory = package("backtrace", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    let root = directory.join("package").canonicalize().unwrap();
    let file = root.join("main.byn").display().to_string();
    assert_eq!(
        stderr(&ran),
        format!(
            "panic: index 4 is out of range for dimension 0 of length 4\n  \
             --> main.byn:2:19\n\
             {BACKTRACE}  \
             main.main at {file}:11:11\n  \
             main.total at {file}:7:12\n  \
             main.pick at {file}:2:19\n"
        )
    );
}
