//! `sys` and `os` (decision 0036), built and run: the program gets its arguments and its
//! environment, and ends with the status `sys.exit` gives, after writing out what `print` held.

use super::{baryon_build, package, panic_report};
use std::process::Command;

/// The program reads its arguments and two variables, one set and one not, and exits with 3;
/// a status outside 0 to 255 panics instead.
#[test]
fn arguments_environment_and_exit_status() {
    let program = "\
import os
import sys


def main():
    args = sys.argv
    print(len(args), args[1:])
    print(os.getenv(\"BARYON_TEST_SET\"), os.getenv(\"BARYON_TEST_UNSET\"))
    print(os.environ.get(\"BARYON_TEST_UNSET\", \"default\"))
    if len(args) > 2:
        sys.exit(256)
    print(\"held\", end=\"\")
    sys.exit(3)
";
    let directory = package("process", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = |arguments: &[&str]| {
        Command::new(&executable)
            .args(arguments)
            .env("BARYON_TEST_SET", "café")
            .env_remove("BARYON_TEST_UNSET")
            .output()
            .unwrap()
    };
    let exited = run(&["grid size"]);
    assert_eq!(exited.status.code(), Some(3));
    assert_eq!(
        String::from_utf8_lossy(&exited.stdout),
        "2 ['grid size']\ncafé None\ndefault\nheld"
    );
    let panicked = run(&["a", "b"]);
    assert_eq!(panicked.status.code(), Some(101));
    assert_eq!(
        panic_report(&panicked),
        "panic: `sys.exit` status 256 is outside 0 to 255\n  --> main.byn:11:9\n"
    );
}

/// `os.environ[name]`, `in` and `copy()` read the environment as Python's do: the copy is a
/// `dict` of the program's own, so adding to it changes nothing else, and a variable that is
/// not set panics as a missing dict key does, where Python raises `KeyError`.
#[test]
fn the_environment_is_indexed_tested_and_copied() {
    let program = "\
import os


def main():
    name = \"BARYON_TEST_SET\"
    value = os.environ[name]
    print(value, name in os.environ, \"BARYON_TEST_UNSET\" in os.environ)
    print(\"BARYON_TEST_UNSET\" not in os.environ)
    copy = os.environ.copy()
    copy[\"BARYON_TEST_NEW\"] = \"new\"
    print(copy[name], len(copy) > 1, \"BARYON_TEST_NEW\" in os.environ)
    print(os.environ[\"BARYON_TEST_UNSET\"])
";
    let directory = package("environment", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .env("BARYON_TEST_SET", "café")
        .env_remove("BARYON_TEST_UNSET")
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(101));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "café True False\nTrue\ncafé True False\n"
    );
    assert_eq!(
        panic_report(&run),
        "panic: key 'BARYON_TEST_UNSET' is not in the dict\n  --> main.byn:12:11\n"
    );
}

/// `OSError` and `ErrorKind`, the standard library's own classes (decision 0038), compile like a
/// program's: caught, tested by `kind`, and printed in Python's form when `main` lets one out.
#[test]
fn os_error_is_caught_tested_and_reported() {
    let program = "\
import os
from os import ErrorKind


def fail(kind: ErrorKind) -> Raises[None, OSError]:
    raise OSError(kind, 13, \"Permission denied\", \"data.txt\")


def main() -> Raises[None, OSError]:
    try:
        fail(ErrorKind.PERMISSION)
    except OSError as error:
        print(error.kind == ErrorKind.PERMISSION, error.errno, error.filename)
    fail(os.ErrorKind.OTHER)
";
    let directory = package("os_error", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "True 13 data.txt\n");
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "error: OSError(kind=ErrorKind.OTHER, errno=13, strerror='Permission denied', \
         filename='data.txt')\n"
    );
}
