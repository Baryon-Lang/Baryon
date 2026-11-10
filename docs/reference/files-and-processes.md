# Files and processes

[What builds today](status.md#files-and-processes) says which parts of this chapter the compiler
builds into programs yet.

A program talks to the system that runs it through Python's modules and names: `sys` for its
command-line arguments and exit status, and `os` for its environment
([decision 0036](../decisions/0036-files-and-processes.md)). They are this language's own
modules, built into the compiler and the runtime, not CPython's: no Python runs underneath.
Reading and writing files, paths, the standard streams and running another program come next,
with the names that decision gives them.

## Command-line arguments

`sys.argv` is a `list[str]` of the arguments the program was started with. `sys.argv[0]` is the
program as it was started, and the rest are its arguments, as in Python:

```python
import sys


def main():
    if len(sys.argv) != 3:
        print("usage: heat <grid size> <steps>")
        sys.exit(2)
    for argument in sys.argv[1:]:
        print("argument:", argument)
```

`baryon build . heat` followed by `./heat 64 1000` prints `argument: 64` and `argument: 1000`.
Turning the text into numbers is `int(text)` and `float(text)`, which roadmap 1.13.12 builds.

Each read of `sys.argv` gives a new list, a copy of a few strings, so a program that changes its
list, such as `args = sys.argv` and then `args.pop()`, changes only its own copy. An argument
that is not valid UTF-8 reads with U+FFFD, the replacement character, in place of each bad
sequence, so reading the arguments never fails.

## Exit status

`sys.exit(status)` ends the program with `status`, an `int` from 0 to 255; `sys.exit()` is
`sys.exit(0)`. It first writes out anything `print` still holds, such as a line `end=""` left
open, then ends the program at once. Other statuses already have a meaning: 1 is an error that
reached the end of `main`, and 101 is a [panic](errors.md#panics).

- A status outside 0 to 255 panics: `` `sys.exit` status 256 is outside 0 to 255 ``.
- Unlike Python's, `sys.exit` is not an exception. A `try` cannot catch it, and nothing after it
  runs.

## Environment variables

`os.getenv(name)` gives the value of the environment variable `name` as a `str | None`: `None`
when it is not set. `os.getenv(name, default)` gives a `str`, `default` when it is not set.
`os.environ.get(name)` and `os.environ.get(name, default)` mean the same.

```python
import os


def thread_count() -> str:
    return os.getenv("OMP_NUM_THREADS", "1")


def main():
    home = os.environ.get("HOME")
    if home is None:
        print("HOME is not set")
    else:
        print("home:", home)
```

A value that is not valid UTF-8 reads with U+FFFD in place of each bad sequence, as an argument
does.

The environment is read-only. Setting a variable, `os.environ["OMP_NUM_THREADS"] = "4"` in
Python, is not part of the language: changing the environment while another thread reads it is a
data race in the C library, and every `prange` loop runs on several threads. A program that
needs another environment gives one to the program it runs, which `subprocess.run(..., env=...)`
will do (decision 0036).

## Errors: `OSError`

Every operation on the operating system that can fail raises one error type, `OSError`, which
every module can use without an import:

```python
class OSError(Exception):
    kind: ErrorKind   # what went wrong, for a program to test
    errno: i32        # the operating system's error number, 0 when there is none
    strerror: str     # the operating system's message, such as "No such file or directory"
    filename: str     # the path involved, or "" when there is none
```

`ErrorKind` is an `Enum` in `os`, with the members `FILE_NOT_FOUND`, `FILE_EXISTS`,
`PERMISSION`, `IS_A_DIRECTORY`, `NOT_A_DIRECTORY`, `BROKEN_PIPE`, `INTERRUPTED`, `TIMEOUT`,
`INVALID_DATA` and `OTHER`, named after Python's subclasses of `OSError`. Baryon has no class
inheritance, so where Python code catches `FileNotFoundError`, a Baryon program catches `OSError`
and tests `kind`:

```python
from os import ErrorKind


def settings(error: OSError) -> str:
    if error.kind == ErrorKind.FILE_NOT_FOUND:
        return "no " + error.filename + "; using the defaults"
    return error.strerror.copy()
```

Both classes are written in Baryon, in source files the compiler carries
([decision 0038](../decisions/0038-standard-library-in-baryon.md)), so they behave exactly like a
program's own classes: `OSError(...)` builds one, `raise` and `except` work as for any exception,
and one that leaves `main` is printed in Python's form, such as
`error: OSError(kind=ErrorKind.FILE_NOT_FOUND, errno=2, strerror='No such file or directory', filename='heat.toml')`.

## What is not built yet

Decision 0036 settles the rest of this chapter's surface, and the compiler reports each part by
name until it is built: `` `sys.stdin` is not in Baryon yet ``. In order, the next parts are
`os.environ[name]`, `name in os.environ` and `os.environ.copy()`; `open`, `with open(...) as f`
and the file methods, which raise `OSError`;
`sys.stdin`, `sys.stdout`, `sys.stderr`, `input` and `print(..., file=)`; `Array.fromfile` and
`a.tofile`; `pathlib.Path` and the directory functions of `os`; and `subprocess.run`.
