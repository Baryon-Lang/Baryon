//! The standard streams (decision 0036, part 6), built and run: `sys.stdin`, `sys.stdout` and
//! `sys.stderr` as files, `print(..., file=, flush=)` and `input(prompt)`, with the output
//! CPython 3.12 gives for the same program and the same input.

use super::{baryon_build, package, stderr};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Runs `executable` in `directory` with `input` as its standard input, and waits for it.
///
/// # Errors
///
/// The error of starting the program, of writing its input or of waiting for it.
fn run_with_input(executable: &Path, directory: &Path, input: &[u8]) -> io::Result<Output> {
    let mut child = Command::new(executable)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(input)?;
    }
    child.wait_with_output()
}

/// A program asks for a name with a prompt, sums the lines of standard input up to `end`,
/// prints to standard error, to standard output through a handle, and to a file it then reads
/// back, and catches the `EOFError` of `input` at the end of the input. The prompt goes to
/// standard output without a newline, as Python writes it when no terminal is attached.
#[test]
fn streams_are_read_written_and_printed_to() {
    let program = "\
import sys


def main() -> Raises[None, OSError | ValueError | EOFError]:
    name = input(\"name: \")
    print(\"hello\", name, file=sys.stderr)
    total = 0.0
    for line in sys.stdin:
        if line.strip() == \"end\":
            break
        total += float(line)
    print(\"total\", total, sep=\"=\", flush=True)
    out = sys.stdout
    out.write(\"written\\n\")
    out.flush()
    with open(\"log.txt\", \"w\") as log:
        print(\"total\", total, file=log, flush=True)
        print(open(\"log.txt\").read(), end=\"\")
        print(\"second\", end=\"\", file=log)
    try:
        input()
    except EOFError:
        print(\"no more input\", file=sys.stderr, flush=True)
    print(sys.stdin.readline() == \"\")
";
    let directory = package("streams", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = run_with_input(&executable, &directory, b"Ada\n1.5\r\n2.5\nend\n").unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "name: total=4.0\nwritten\ntotal 4.0\nTrue\n"
    );
    assert_eq!(stderr(&run), "hello Ada\nno more input\n");
    assert_eq!(
        fs::read_to_string(directory.join("log.txt")).unwrap(),
        "total 4.0\nsecond"
    );
}

/// An `EOFError` that `main` lets out is reported as any error is, with exit status 1. The last
/// line needs no newline, and a byte that is not UTF-8 is read as U+FFFD.
#[test]
fn input_ends_with_eof_error() {
    let program = "\
def main() -> Raises[None, EOFError]:
    while True:
        print(input())
";
    let directory = package("streams_end", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = run_with_input(&executable, &directory, b"one\nt\xffo").unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "one\nt\u{fffd}o\n");
    assert_eq!(stderr(&run), "error: EOFError()\n");
}
