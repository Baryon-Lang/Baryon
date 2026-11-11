# Files and processes

[What builds today](status.md#files-and-processes) says which parts of this chapter the compiler
builds into programs yet.

A program talks to the system that runs it through Python's modules and names: `sys` for its
command-line arguments and exit status, `os` for its environment, `open` for text files, and
`subprocess` for running another program, and `Array.fromfile` and `a.tofile` for binary files of
arrays ([decision 0036](../decisions/0036-files-and-processes.md)). They are this language's own
modules, built into the compiler and the runtime, not CPython's: no Python runs underneath.
Paths and the standard streams come next, with the names that decision gives them.

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
needs another environment gives one to the program it runs, with
[`subprocess.run(..., env=...)`](#running-a-program).

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

A `for` loop over a file reads it one line at a time, so a file of any size costs the memory of
its longest line:

```python
def total(path: str) -> Raises[float, OSError | ValueError]:
    energy = 0.0
    with open(path) as source:
        for line in source:
            if line.startswith("#"):
                continue
            energy += float(line.split()[1])
    return energy
```

With `e.txt` holding the lines `# step energy`, `0 1.5` and `1 -0.25`, `total("e.txt")` gives
`1.25`.

- **Each pass is one `readline()`.** The loop variable gets the next line with its line end, and
  the loop ends at the end of the file. A `break` leaves the rest unread, so a `readline()` or
  another loop after it carries on from there. A comprehension loops the same way:
  `[line.strip() for line in source]`.
- **The loop variable owns its line.** Each line is a new `str`, so it can be kept without a
  copy: `kept.append(line)`.
- **The loop can fail.** A read that fails raises `OSError` from the `for` statement, as
  `readline()` would.
- **The loop borrows the file.** The body cannot read, write or close the file the loop goes
  over; to do that, read the lines first with `readlines()`.

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
read or to write, not both.

## Running a program

`subprocess.run(args)` runs another program, waits for it to end, and returns a
`CompletedProcess`. `args` is a `list[str]`: the program, found on `PATH`, then each of its
arguments.

```python
import subprocess


def git_revision() -> Raises[str, OSError]:
    result = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True)
    if result.returncode != 0:
        return "unknown"
    return result.stdout.strip()
```

In a `git` checkout `git_revision()` gives the commit, such as
`"ed289d3c0a7b..."`, and anywhere else it gives `"unknown"`. On a machine without `git` the call
raises `OSError` with the kind `FILE_NOT_FOUND`.

A `CompletedProcess` is a struct in `subprocess` with Python's fields:

| Field | Type | Holds |
| --- | --- | --- |
| `args` | `list[str]` | a copy of the list the program was run with |
| `returncode` | `int` | the exit status, 0 to 255; minus the signal's number when a signal ended the program, such as `-9` |
| `stdout` | `str` | what the program wrote to standard output, when `capture_output=True`; `""` otherwise |
| `stderr` | `str` | what the program wrote to standard error, when `capture_output=True`; `""` otherwise |

Five keywords change how the program runs. Each is written with a value or left out:

| Keyword | Type | Without it | With it |
| --- | --- | --- | --- |
| `capture_output` | `bool` | the program writes to this program's standard output and standard error | `True` collects both into `stdout` and `stderr` |
| `input` | `str` | the program reads this program's standard input | the program reads the text as its standard input |
| `cwd` | `str` | the program runs in this program's directory | the program runs in that directory |
| `env` | `dict[str, str]` | the program gets a copy of this program's environment | the `dict` is the program's whole environment |
| `timeout` | `float` or `int` | the program runs as long as it takes | after that many seconds the program is ended and the call raises `OSError` with the kind `TIMEOUT` |

```python
import subprocess


def solve(mesh: str, threads: int) -> Raises[str, OSError]:
    environment = {"PATH": "/usr/bin:/bin", "OMP_NUM_THREADS": f"{threads}"}
    result = subprocess.run(
        ["sh", "-c", "wc -l; echo $OMP_NUM_THREADS threads"],
        capture_output=True,
        input=mesh,
        cwd="/tmp",
        env=environment,
        timeout=60,
    )
    return result.stdout
```

`solve("0 0\n1 0\n", 4)` runs `wc -l` on the two lines and gives `"2\n4 threads\n"`, with
spaces before the `2` on macOS.

- **There is no shell.** The arguments reach the program exactly as the list holds them, so a
  file name with a space is one argument and nothing in a value is ever run as a command. A `str`
  command and `shell=True` are errors that show the list to write. A pipeline or a redirection
  is written by running `["sh", "-c", line]`, which makes the shell visible.
- **A status other than 0 is not an error.** The call returns, and the program tests
  `returncode`. `check=True` is not in Baryon yet.
- **Failures raise `OSError`.** A program that is not found has the kind `FILE_NOT_FOUND`, one
  that may not be run `PERMISSION`, a `cwd` that is missing or not a directory `FILE_NOT_FOUND` or
  `NOT_A_DIRECTORY`, and a timeout `TIMEOUT`. `filename` is the program, or `cwd` when that is
  what was wrong. An empty list panics:
  `` `subprocess.run` needs a program to run: its argument list is empty ``.
- **Output is text.** Captured output is read as a [text file](#text-files) is: it must be
  UTF-8, or the call raises `OSError` with the kind `INVALID_DATA`, and `"\r\n"` and `"\r"`
  read as `"\n"`. `text=True` may be written and changes nothing. Captured output is held in
  memory whole.
- **Output stays in order.** Anything `print` still holds is written out before the program
  starts, so this program's output so far comes before the other program's.
- **The call borrows its arguments.** The list, the `dict` and the text are read, not moved, and
  can be used after the call.
- **A keyword is written or left out.** `input=None` means what leaving `input` out means.
  Passing a value that may be `None`, such as a `str | None`, is not built yet: test it first and
  call with the keyword only where there is a value.
- **A timeout ends the program, not what it started.** Programs the ended program started
  itself keep running, and a `timeout` call does not wait for them or read what they write.
- **`env` replaces the environment.** With `env`, the program is looked up on the `PATH` the
  `dict` gives, and on the system's default when it gives none.

Three things differ from Python. `stdout` and `stderr` are `""` when output is not captured,
where Python gives `None`, so `result.stdout.strip()` needs no test first. A timeout raises
`OSError`, where Python raises `subprocess.TimeoutExpired`. And only the keywords above exist:
`stdin`, `stdout`, `stderr`, `encoding` and the rest of Python's are errors that say so.

## Array files

Binary data is whole arrays, with NumPy's names and NumPy's bytes. `a.tofile(path)` writes an
array's elements to a file, and `Array.fromfile(path, dtype)` reads a file's elements into a new
array. An array file holds the elements and nothing else: each element's bytes as they lie in
memory, in row-major order, with no header, shape or type. It is the file `numpy.ndarray.tofile`
writes and `numpy.fromfile` reads, so Baryon and NumPy share array files byte for byte. Every
platform Baryon supports is little-endian, so a file written on one reads on any other.

| Call | Gives | Does |
| --- | --- | --- |
| `a.tofile(path)` | `None` | makes the file, or empties the one there, and writes every element of `a` |
| `Array.fromfile(path, dtype)` | `Array[dtype, 1]` | reads every element in the file |
| `Array.fromfile(path, dtype, count=n, offset=k)` | `Array[dtype, 1]` | skips `k` bytes, then reads `n` elements |

Each can fail with [`OSError`](#errors-oserror). A simulation saves its state and picks it up
again in a later run:

```python
from os import ErrorKind


def save(path: str, temperature: Array[f64, 2]) -> Raises[None, OSError]:
    temperature.tofile(path)


def restore(path: str, temperature: InOut[Array[f64, 2]]) -> Raises[bool, OSError]:
    rows, columns = temperature.shape
    try:
        saved = Array.fromfile(path, f64, count=rows * columns)
    except OSError as error:
        if error.kind == ErrorKind.FILE_NOT_FOUND:
            return False
        raise
    for i in range(rows):
        for j in range(columns):
            temperature[i, j] = saved[i * columns + j]
    return True
```

`save("heat.bin", grid)` of a 64 by 64 grid writes a file of 32768 bytes, 4096 elements of 8
bytes. `restore` gives `False` when there is no file yet, fills the grid when there is one, and
raises `OSError` with the kind `INVALID_DATA` when the file holds fewer than `rows * columns`
elements, so a file cut short is never taken for a whole grid. In Python,
`numpy.fromfile("heat.bin").reshape(64, 64)` reads the same file.

- **Reading gives rank 1.** The file does not record a shape, so `fromfile` gives an
  `Array[dtype, 1]` in the file's order, which is row-major for an array `tofile` wrote: element
  `[i, j]` of a grid with `columns` columns is element `i * columns + j`.
- **The element type** is the `dtype` argument, given second or as `dtype=`; without it, the type
  the context needs, as in `masses: Array[f32, 1] = Array.fromfile(path)`; otherwise `f64`, as
  NumPy's default is `float64`.
- **What can be read.** The file's bytes become the elements as they are, so `dtype` is a type
  every pattern of bytes is a value of: a number, a `bool`, a `Fixed` array of them, or a struct
  declared `Copy` whose fields are all such types. A struct is read with the padding C gives it,
  which NumPy calls `align=True`. A byte read as a `bool` is `True` unless it is 0, as in NumPy.
  An enum cannot be read, since a byte might name no member; read the integers and convert them.
  A type parameter cannot be read either: write the element type out.
- **What can be written.** `tofile` writes an array of any element type. A view writes the
  elements it sees, in row-major order, so `grid[:, 0].tofile(path)` writes one column.
- **`count` and `offset`.** `offset` is in bytes, as in NumPy, and skips a header or the blocks
  before the one wanted. `count` is in elements; left out, or negative, it reads to the end of
  the file. An offset at or past the end of the file reads an empty array.
- **A file that does not fit is an error.** With no `count`, the bytes after the offset must be
  a whole number of elements. With a `count`, the file must hold at least that many elements.
  Otherwise the call raises `OSError` with the kind `INVALID_DATA`, such as
  `the file holds 4 elements of 8 bytes after the offset, fewer than the 100 asked for`. NumPy
  returns a shorter array and no error in both cases; Baryon never loses data silently.
- **Cost.** One pass over the bytes, a system call for each large block and no work per
  element: a read goes straight into the new array's memory, and a write comes straight from
  the array's. A view that skips elements is written a run at a time through a 64 KiB buffer.
  Reading into `bool`s visits each one once more.
- **Other files.** A path that is not a regular file, such as a pipe, is read to its end before
  the array is made, since it has no length to ask for.

NumPy's text mode, `sep=`, is not in Baryon: an array file is binary, and text is read with
[`open`](#text-files) and the `str` methods. `tofile` does not write to an open `File`, and the
`.npy` format, which records the shape and the type, is not built yet (roadmap idea I.39).

## What is not built yet

Decision 0036 settles the rest of this chapter's surface, and the compiler reports each part by
name until it is built: `` `sys.stdin` is not in Baryon yet ``. In order, the next parts are
`os.environ[name]`, `name in os.environ` and `os.environ.copy()`;
`sys.stdin`, `sys.stdout`, `sys.stderr`, `input` and `print(..., file=)`; and `pathlib.Path`
and the directory functions of `os`. Past that, idea I.39 lists what comes later,
`subprocess.run(check=True)` and a keyword of `subprocess.run` given a value that may be `None`
among them.
