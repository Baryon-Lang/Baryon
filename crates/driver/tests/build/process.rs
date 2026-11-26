//! `sys` and `os` (decision 0036), built and run: the program gets its arguments and its
//! environment, and ends with the status `sys.exit` gives, after writing out what `print` held.
//! Also the clocks of `time` (roadmap 1.13.19).

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

/// `time.perf_counter()` and `time.monotonic()` never go back and count the seconds a busy loop
/// takes, and `time.time()` is the date the test reads from the system too, within a minute.
/// Each is imported both ways, as a module and by name.
#[test]
fn the_clocks_of_time_read_seconds() {
    let program = "\
import time
from time import monotonic, perf_counter


def main():
    wall = time.time()
    start = perf_counter()
    first = time.monotonic()
    total = 0.0
    for i in range(20000000):
        total += f64(i) * 0.5
    elapsed = time.perf_counter() - start
    print(elapsed > 0.0, elapsed < 60.0, monotonic() >= first, total > 0.0)
    print(int(wall))
";
    let directory = package("clocks", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("True True True True"));
    let wall: u64 = lines.next().unwrap().parse().unwrap();
    assert!(
        wall >= before && wall < before + 60,
        "{wall} against {before}"
    );
}

/// A clock with an argument, and a member of `time` that is not built, are compile errors.
#[test]
fn the_clocks_take_no_arguments() {
    let program = "\
import time


def main():
    print(time.perf_counter(1))
    time.sleep(1.0)
";
    let directory = package("clock_errors", &[("main.byn", program)]).unwrap();
    let (output, _) = baryon_build(&directory).unwrap();
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "error: `time.perf_counter` takes no arguments\n --> main.byn:5:11\n  |\n\
         5 |     print(time.perf_counter(1))\n  |           ^^^^^^^^^^^^^^^^^^^^ wrong arguments\n\
         \x20 = note: call it as `time.perf_counter()`\n\n\
         error: `time.sleep` is not in Baryon yet\n --> main.byn:6:5\n  |\n\
         6 |     time.sleep(1.0)\n  |     ^^^^^^^^^^^^^^^ not built\n\
         \x20 = note: the language reference lists what `time` has; the roadmap's Ideas table \
         is the place to ask for more\n\n"
    );
}

/// The `time` page's example, read from the page itself, prints the sum the page shows and a
/// time.
#[test]
fn the_reference_s_clock_example_runs() {
    let page = include_str!("../../../../docs/reference/time.md");
    let block = page.split("```python\n").nth(1).unwrap_or_default();
    let program = &block[..block.find("```").unwrap_or_default()];
    let directory = package("clock_example", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let seconds = stdout
        .strip_prefix("sum 1.644934 in ")
        .and_then(|rest| rest.strip_suffix(" s\n"))
        .unwrap_or_else(|| panic!("{stdout}"));
    assert!(
        seconds.parse::<f64>().is_ok_and(|seconds| seconds >= 0.0),
        "{stdout}"
    );
}
