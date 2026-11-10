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
