//! Checking `subprocess.run` (decision 0036, part 8), end to end: programs that misuse it go
//! through the parser, the resolver and the checker, and the rendered diagnostics are compared
//! in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `subprocess.run` has no shell: a `str` command is an error, with the list to write when the
/// command is a literal, and so is `shell=True`. A call with no command, the members of
/// `subprocess` that are not built, and an `OSError` nobody handles are each reported too.
#[test]
fn a_command_is_a_list_and_there_is_no_shell() {
    let program = r#"
import subprocess


def main() -> Raises[None, OSError]:
    subprocess.run("git rev-parse HEAD")
    line = "ls -l"
    subprocess.run(line)
    subprocess.run(["ls"], shell=True)
    subprocess.run()
    subprocess.check_output(["ls"])
    pipe = subprocess.PIPE


def quiet():
    subprocess.run(["ls"])
"#;
    assert_eq!(
        check_one(program),
        r#"error: `subprocess.run` takes its command as a `list[str]`, not a `str`
 --> main.byn:5:20
  |
5 |     subprocess.run("git rev-parse HEAD")
  |                    ^^^^^^^^^^^^^^^^^^^^ one `str`
  = note: write `subprocess.run(["git", "rev-parse", "HEAD"])`: there is no shell to split the line (decision 0036)

error: `subprocess.run` takes its command as a `list[str]`, not a `str`
 --> main.byn:7:20
  |
7 |     subprocess.run(line)
  |                    ^^^^ one `str`
  = note: write the program and each argument as one item of a list: there is no shell to split the line (decision 0036)

error: `subprocess.run` has no shell
 --> main.byn:8:28
  |
8 |     subprocess.run(["ls"], shell=True)
  |                            ^^^^^ no shell
  = note: write the program and each argument as one item of the list, such as `["grep", "-c", pattern, path]`; a shell would split a file name with a space and let a value become a command (decision 0036)

error: `subprocess.run` takes one command, a `list[str]`
 --> main.byn:9:5
  |
9 |     subprocess.run()
  |     ^^^^^^^^^^^^^^^^ wrong arguments
  = note: `subprocess.run` takes a `list[str]`, then `capture_output=`, `input=`, `cwd=`, `env=` and `timeout=`

error: `subprocess.check_output` is not in Baryon yet
  --> main.byn:10:5
   |
10 |     subprocess.check_output(["ls"])
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built
   = note: `subprocess` has `run` and `CompletedProcess` (decision 0036)

error: `subprocess.PIPE` is not in Baryon yet
  --> main.byn:11:23
   |
11 |     pipe = subprocess.PIPE
   |                       ^^^^ not built
   = note: `subprocess` has `run` and `CompletedProcess` (decision 0036)

error: `subprocess.run` can fail with `OSError`, which is not handled here
  --> main.byn:15:5
   |
15 |     subprocess.run(["ls"])
   |     ^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}

/// Each keyword explains a wrong value: the ones Python has and Baryon does not, a value that
/// may be `None`, a value of the wrong type, and a keyword given twice.
#[test]
fn wrong_keywords_are_explained() {
    let program = r#"
import subprocess


def maybe() -> str | None:
    return None


def main() -> Raises[None, OSError]:
    subprocess.run(["ls"], check=True)
    subprocess.run(["ls"], text=False)
    subprocess.run(["ls"], stdout=1)
    subprocess.run(["ls"], input=maybe())
    subprocess.run(["ls"], timeout="soon")
    subprocess.run(["ls"], capture_output=None)
    subprocess.run(["ls"], cwd="a", cwd="b")
"#;
    assert_eq!(
        check_one(program),
        r#"error: `check=True` is not in Baryon yet
 --> main.byn:9:28
  |
9 |     subprocess.run(["ls"], check=True)
  |                            ^^^^^ not built
  = note: test `returncode` after the call: `if result.returncode != 0:`

error: `subprocess.run` gives output as text
  --> main.byn:10:28
   |
10 |     subprocess.run(["ls"], text=False)
   |                            ^^^^ always text
   = note: there is no `bytes` type: `stdout` and `stderr` are `str`, read as UTF-8 (decision 0036)

error: `subprocess.run` has no parameter `stdout` in Baryon
  --> main.byn:11:28
   |
11 |     subprocess.run(["ls"], stdout=1)
   |                            ^^^^^^ unknown parameter
   = note: `subprocess.run` takes a `list[str]`, then `capture_output=`, `input=`, `cwd=`, `env=` and `timeout=`

error: passing a value that may be `None` to `input=` is not in Baryon yet
  --> main.byn:12:34
   |
12 |     subprocess.run(["ls"], input=maybe())
   |                                  ^^^^^^^ may be `None`
   = note: test for `None` first, and call `subprocess.run` with the keyword only where there is a value

error: expected `f64`, found `str`
  --> main.byn:13:36
   |
13 |     subprocess.run(["ls"], timeout="soon")
   |                                    ^^^^^^ `timeout=` is `f64`

error: expected `bool`, found `None`
  --> main.byn:14:43
   |
14 |     subprocess.run(["ls"], capture_output=None)
   |                                           ^^^^ `capture_output=` is `bool`

error: `cwd=` is given twice
  --> main.byn:15:37
   |
15 |     subprocess.run(["ls"], cwd="a", cwd="b")
   |                                     ^^^ given already"#
    );
}

/// A correct call has the type `Raises[CompletedProcess, OSError]`, whose fields are Python's.
#[test]
fn a_correct_call_checks() {
    let program = r#"
import subprocess
from subprocess import CompletedProcess


def lines(result: CompletedProcess) -> list[str]:
    return result.stdout.split("\n")


def main() -> Raises[None, OSError]:
    result = subprocess.run(["ls", "-l"], capture_output=True, timeout=10)
    status: int = result.returncode
    listed: list[str] = lines(result)
    again: CompletedProcess = subprocess.run(result.args, env={"PATH": "/bin"}, cwd="/")
    print(status, listed, again.stderr)
"#;
    assert_eq!(check_one(program), "");
}
