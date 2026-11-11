//! `subprocess.run` (decision 0036, part 8), built and run: the exit status, captured output,
//! `input`, `cwd`, `env` and `timeout`, the `OSError` a failure raises, and the panic for an
//! empty command. Every program run is one Linux and macOS both have: `sh`, `cat` and `sleep`.
//! What the first test prints was compared with CPython 3.12 running the same calls, apart from
//! the two differences the reference lists: `""` where Python gives `None`, and `OSError` with
//! the kind `TIMEOUT` where Python raises `TimeoutExpired`.

use super::{baryon_build, package};
use std::process::Command;

/// A program runs other programs every way `subprocess.run` allows, and its own output stays in
/// order around a child's that shares its standard output.
#[test]
fn programs_are_run_and_their_output_read() {
    let program = r#"
import subprocess
from os import ErrorKind
from subprocess import CompletedProcess, run


def show(result: CompletedProcess):
    print(result.args, result.returncode, result.stdout, result.stderr)


def revision(directory: str) -> Raises[str, OSError]:
    result = subprocess.run(["sh", "-c", "echo ' abc123 '"], capture_output=True, cwd=directory)
    if result.returncode != 0:
        return "unknown"
    return result.stdout.strip()


def main() -> Raises[None, OSError]:
    print("before", end="")
    plain = subprocess.run(["sh", "-c", "echo ' child'; exit 3"])
    print(plain.returncode, plain.stdout == "", plain.stderr == "")
    show(run(["sh", "-c", "echo out; echo err >&2"], capture_output=True))
    words = ["cat"]
    text = "piped\n"
    fed = subprocess.run(
        words, capture_output=True, input=text, text=True, shell=False, check=False
    )
    print(fed.stdout.strip(), words, text, end="")
    environment = {"PATH": "/usr/bin:/bin", "GREETING": "hi there"}
    where = "/"
    there = subprocess.run(
        ["sh", "-c", "pwd; echo $GREETING; echo ${HOME:-no home}"],
        capture_output=True,
        cwd=where,
        env=environment,
        timeout=5,
    )
    print(there.stdout, end="")
    print(len(environment), where, revision("/"))
    spaced = subprocess.run(
        ["sh", "-c", "printf '%s|' \"$@\"", "sh", "two words", "$HOME;ls"],
        capture_output=True,
    )
    print(spaced.stdout)
    try:
        subprocess.run(["sleep", "30"], timeout=0.05)
    except OSError as error:
        print(error.kind == ErrorKind.TIMEOUT, error.errno, error.strerror, error.filename)
    try:
        subprocess.run(["baryon-no-such-program"], input=None, cwd=None, env=None, timeout=None)
    except OSError as error:
        print(error.kind == ErrorKind.FILE_NOT_FOUND, error.errno)
    print(subprocess.run(["sh", "-c", "kill -9 $$"]))
    subprocess.run(["baryon-no-such-program"])
"#;
    let directory = package("subprocess", &[("main.byn", program.trim_start())]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "before child\n\
         3 True True\n\
         ['sh', '-c', 'echo out; echo err >&2'] 0 out\n err\n\n\
         piped ['cat'] piped\n\
         /\nhi there\nno home\n\
         2 / abc123\n\
         two words|$HOME;ls|\n\
         True 0 timed out after 0.05 seconds sleep\n\
         True 2\n\
         CompletedProcess(args=['sh', '-c', 'kill -9 $$'], returncode=-9, stdout='', stderr='')\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "error: OSError(kind=ErrorKind.FILE_NOT_FOUND, errno=2, \
         strerror='No such file or directory', filename='baryon-no-such-program')\n"
    );
}

/// An empty command panics, where Python raises `IndexError`.
#[test]
fn an_empty_command_panics() {
    let program = "\
import subprocess


def main() -> Raises[None, OSError]:
    nothing: list[str] = []
    subprocess.run(nothing)
";
    let directory = package("subprocess_empty", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(run.status.code(), Some(101), "{run:?}");
    assert_eq!(
        super::panic_report(&run),
        "panic: `subprocess.run` needs a program to run: its argument list is empty\n  \
         --> main.byn:6:5\n"
    );
}
