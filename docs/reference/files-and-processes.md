# Files and processes

[What builds today](status.md#files-and-processes) says which parts of this chapter the compiler
builds into programs yet.

A program talks to the system that runs it through Python's modules and names: `sys` for its
command-line arguments and exit status, `os` for its environment, and `open` for text files
([decision 0036](../decisions/0036-files-and-processes.md)). They are this language's own
modules, built into the compiler and the runtime, not CPython's: no Python runs underneath.
Paths, the standard streams, array files and running another program come next,
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
Turning the text into numbers is `int(text)` and `float(text)`, which raise `ValueError` on text
that is not a number ([Types](types.md#indexing-slicing-and-methods)).

Each read of `sys.argv` gives a new list, a copy of a few strings, so a program that changes its
list, such as `args = sys.argv` and then `args.pop()`, changes only its own copy. An argument
that is not valid UTF-8 reads with U+FFFD, the replacement character, in place of each bad
sequence, so reading the arguments never fails.

## Exit status

`sys.exit(status)` ends the program with `status`, an `int` from 0 to 255; `sys.exit()` is
`sys.exit(0)`. It first writes out anything `print` still holds, such as a line `end=""` left
open, and everything written to a [file](#text-files) still open, then ends the program at once. Other statuses already have a meaning: 1 is an error that
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

## Text files

`open(path, mode)` opens a text file and returns a `File`. The mode is written out as a literal:

| Mode | Opens the file to | When the file exists | When it does not |
| --- | --- | --- | --- |
| `"r"`, the default | read | reads it from the start | `FILE_NOT_FOUND` |
| `"w"` | write | empties it first | creates it |
| `"a"` | write | writes after its end | creates it |
| `"x"` | write | `FILE_EXISTS` | creates it |

A `File` has six methods, and each can fail with `OSError`:

| Method | Gives | Does |
| --- | --- | --- |
| `f.read()` | `str` | the rest of the file |
| `f.readline()` | `str` | the next line with its line end, or `""` at the end of the file |
| `f.readlines()` | `list[str]` | the remaining lines, each with its line end |
| `f.write(text)` | `int` | writes `text` and gives the number of characters written |
| `f.flush()` | `None` | sends what the file holds to the operating system |
| `f.close()` | `None` | writes out what the file holds and closes it |

```python
def save(path: str, energies: list[float]) -> Raises[None, OSError]:
    with open(path, "w") as out:
        for energy in energies:
            out.write(f"{energy}\n")


def count(path: str) -> Raises[int, OSError]:
    with open(path) as source:
        return len(source.readlines())
```

`save("e.txt", [1.5, -0.25])` writes the two lines `1.5` and `-0.25`, and `count("e.txt")` then
gives `2`. `count` returns from inside `with`, which drops the file and so closes it.

- **Text is UTF-8.** `encoding="utf-8"` may be written and changes nothing; any other encoding is
  an error. Reading text that is not UTF-8 raises `OSError` with the kind `INVALID_DATA`.
- **Line ends are universal.** Reading turns `"\r\n"` and `"\r"` into `"\n"`, as Python does.
  Writing writes text as it is.
- **Writes are buffered.** A file holds what is written, 64 KiB at a time, and writes it out when
  it fills, at `flush()`, at `close()`, and when the program calls `sys.exit`.
- **A file has one owner.** Reading or writing changes the file, as `append` changes a list, so a
  function that takes one declares the parameter `InOut[File]`. `close()` moves the file, so it
  cannot be used after.
- **`with` closes the file.** `with open(path, mode) as f:` gives the body the file and closes it
  where the body ends. A failure to write it out then is an `OSError` raised by the `with`
  statement. A body left by `return`, `break` or an error drops the file instead. `with` takes
  only a `File` and one `as` name; other context managers are not in Baryon.
- **A dropped file is closed.** A file that reaches the end of its owner's scope without
  `close()` or `with` is closed then. A drop cannot raise, so a failure to write the file out
  there panics: `` could not write out `out.txt` when it was dropped: No space left on device ``.
  Use `with` or `close()` where that failure must be handled.

The modes Python has and Baryon does not are errors that say what to write instead. A `"b"` mode
points to `Array.fromfile` and `a.tofile`, since binary data is whole arrays
([decision 0036](../decisions/0036-files-and-processes.md)). A `"+"` mode says a file is opened to
read or to write, not both. Looping over a file, `for line in f:`, is not in Baryon yet; loop over
`f.readlines()`.

## What is not built yet

Decision 0036 settles the rest of this chapter's surface, and the compiler reports each part by
name until it is built: `` `sys.stdin` is not in Baryon yet ``. In order, the next parts are
`os.environ[name]`, `name in os.environ` and `os.environ.copy()`; `for line in f`;
`sys.stdin`, `sys.stdout`, `sys.stderr`, `input` and `print(..., file=)`; `Array.fromfile` and
`a.tofile`; `pathlib.Path` and the directory functions of `os`; and `subprocess.run`.
