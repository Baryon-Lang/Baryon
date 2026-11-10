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
