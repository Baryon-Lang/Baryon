//! Text files (decision 0036, parts 3 and 4), built and run: `open`, the `File` methods and
//! `with`, the `OSError` a failure raises, the panic when a dropped file cannot be written out,
//! and `sys.exit` writing out what an open file still holds.

use super::{baryon_build, package};
use std::fs;
use std::process::Command;

/// A program writes a file with `with`, appends to it, reads it back three ways, catches the
/// `OSError` of a missing file and of `"x"` on a file that exists, and reads a temporary file
/// in one expression.
#[test]
fn files_are_written_read_and_closed() {
    let program = "\
from os import ErrorKind


def save(path: str) -> Raises[None, OSError]:
    with open(path, \"w\") as out:
        out.write(\"x 1.5\\n\")
        out.write(\"y 2.5\\r\\n\")
    log = open(path, mode=\"a\", encoding=\"utf-8\")
    count = log.write(\"zéta 3.5\\n\")
    log.close()
    print(count)


def main() -> Raises[None, OSError]:
    save(\"data.txt\")
    f = open(\"data.txt\")
    print(f.readline(), end=\"\")
    rest = f.readlines()
    print(rest)
    f.close()
    print(open(\"data.txt\").read(), end=\"\")
    try:
        open(\"missing.txt\")
    except OSError as error:
        print(error.kind == ErrorKind.FILE_NOT_FOUND, error.errno, error.filename)
    try:
        open(\"data.txt\", \"x\")
    except OSError as error:
        print(error.kind == ErrorKind.FILE_EXISTS, error.strerror)
";
    let directory = package("files", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "9\nx 1.5\n['y 2.5\\n', 'zéta 3.5\\n']\nx 1.5\ny 2.5\nzéta 3.5\n\
         True 2 missing.txt\nTrue File exists\n"
    );
    assert_eq!(
        fs::read_to_string(directory.join("data.txt")).unwrap(),
        "x 1.5\ny 2.5\r\nzéta 3.5\n"
    );
}

/// A file that cannot be written out is an `OSError` where `with` closes it, and a panic where
/// it is only dropped. `sys.exit` writes out a file the program still holds open. Linux only,
/// for `/dev/full`, the device every write to fails with "No space left on device".
#[cfg(target_os = "linux")]
#[test]
fn closing_fails_loudly_and_exit_writes_out() {
    let program = "\
import sys


def with_full() -> Raises[None, OSError]:
    with open(\"/dev/full\", \"w\") as out:
        out.write(\"lost\")


def dropped_full() -> Raises[None, OSError]:
    out = open(\"/dev/full\", \"w\")
    out.write(\"lost\")


def main() -> Raises[None, OSError]:
    held = open(\"held.txt\", \"w\")
    held.write(\"kept\")
    if len(sys.argv) > 1:
        dropped_full()
    try:
        with_full()
    except OSError as error:
        print(error.strerror, error.filename)
    sys.exit(4)
";
    let directory = package("files_failing", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = |arguments: &[&str]| {
        Command::new(&executable)
            .args(arguments)
            .current_dir(&directory)
            .output()
            .unwrap()
    };
    let exited = run(&[]);
    assert_eq!(exited.status.code(), Some(4), "{exited:?}");
    assert_eq!(
        String::from_utf8_lossy(&exited.stdout),
        "No space left on device /dev/full\n"
    );
    assert_eq!(
        fs::read_to_string(directory.join("held.txt")).unwrap(),
        "kept"
    );
    let panicked = run(&["drop"]);
    assert_eq!(panicked.status.code(), Some(101), "{panicked:?}");
    assert_eq!(
        super::panic_report(&panicked),
        "panic: could not write out `/dev/full` when it was dropped: No space left on device\n"
    );
}

/// A loop over a file reads it line by line, as Python does: each line keeps its line end, an
/// empty line is `"\n"`, `continue` and `break` work, a `readline()` after the loop carries on
/// where it stopped, and a file at its end gives no passes. A comprehension loops the same way,
/// a file opened in the loop's header is closed after it, and a failed read is an `OSError`.
#[test]
fn a_loop_over_a_file_reads_each_line() {
    let program = "\
def main() -> Raises[None, OSError]:
    with open(\"data.txt\", \"w\") as out:
        out.write(\"x 1.5\\n\\ny 2.5\\r\\nz 3\")
    f = open(\"data.txt\")
    kept: list[str] = []
    for line in f:
        if line == \"\\n\":
            continue
        stop = line.startswith(\"y\")
        kept.append(line)
        if stop:
            break
    print(kept)
    print(f.readline())
    for line in f:
        print(\"never\", line)
    print([line.strip() for line in open(\"data.txt\") if line != \"\\n\"])
    for line in open(\"data.txt\"):
        print(line, end=\"\")
    print()
    try:
        for line in open(\"missing.txt\"):
            print(line)
    except OSError as error:
        print(error.filename)
    log = open(\"data.txt\", \"a\")
    try:
        for line in log:
            print(line)
    except OSError as error:
        print(error.strerror)
";
    let directory = package("file_loop", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "['x 1.5\\n', 'y 2.5\\n']\nz 3\n['x 1.5', 'y 2.5', 'z 3']\n\
         x 1.5\n\ny 2.5\nz 3\nmissing.txt\nnot readable\n"
    );
    fs::remove_dir_all(directory).unwrap();
}
