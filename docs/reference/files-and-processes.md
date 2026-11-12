# Files and processes

[What builds today](status.md#files-and-processes) says which parts of this chapter the compiler
builds into programs yet.

A program talks to the system that runs it through Python's modules and names: `sys` for its
command-line arguments, exit status and standard streams, `os` for its environment and
directories, `pathlib` for paths, `open` for text files, `subprocess` for running another
program, and `Array.fromfile` and `a.tofile` for binary files of arrays
([decision 0036](../decisions/0036-files-and-processes.md)). They are this language's own
modules, built into the compiler and the runtime, not CPython's: no Python runs underneath.

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
- `sys.exit` cannot run inside a `prange` loop, where other iterations may be writing the files
  it would write out. Written in the body it is a compile error; reached through a call it
  panics: `` `sys.exit` cannot end the program inside a parallel loop ``. See
  [Parallel loops](parallel-loops.md).
- Unlike Python's, `sys.exit` is not an exception. A `try` cannot catch it, and nothing after it
  runs.

## Environment variables

`os.getenv(name)` gives the value of the environment variable `name` as a `str | None`: `None`
when it is not set. `os.getenv(name, default)` gives a `str`, `default` when it is not set.
`os.environ.get(name)` and `os.environ.get(name, default)` mean the same.

`os.environ[name]` gives the value as a `str` and panics when the variable is not set, as reading
a missing key of a `dict` does; Python raises `KeyError` there. `name in os.environ` and
`name not in os.environ` test whether it is set. `os.environ.copy()` gives every variable as a new
`dict[str, str]` of the program's own, in the order the operating system keeps them: changing the
copy changes nothing else, so it is the way to build the environment for another program.
`os.environ` on its own is not a value, since these are the only ways to read it.

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
    if "CI" in os.environ:
        print("running in CI as", os.environ["USER"])
    variables = os.environ.copy()
    print(len(variables), "variables")
```

A value that is not valid UTF-8 reads with U+FFFD in place of each bad sequence, as an argument
does.

The environment is read-only. Setting a variable, `os.environ["OMP_NUM_THREADS"] = "4"` in
Python, is a compile error, and so are `del os.environ[name]`, `os.putenv` and `os.unsetenv`: changing the environment while another thread reads it is a
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

## Standard streams

Every program starts with three streams: standard input, which it reads, and standard output and
standard error, which it writes. They are `sys.stdin`, `sys.stdout` and `sys.stderr`, each a
`File` like one from `open`, with the same methods.

```python
import sys


def main() -> Raises[None, OSError | ValueError]:
    total = 0.0
    count = 0
    for line in sys.stdin:
        total += float(line)
        count += 1
    if count == 0:
        print("no numbers given", file=sys.stderr)
        sys.exit(1)
    print(total / count)
```

Run as `baryon run < energies.txt`, with `energies.txt` holding the lines `1.5` and `2.5`, this
prints `2.0`. With an empty file it writes `no numbers given` to standard error and exits with
status 1, so a shell that collects the program's output with `>` still shows the message.

- **`print(..., file=f)` writes to a file.** `f` is any `File` open to write. The values, `sep=`
  and `end=` are as without `file=`. Writing to a file can fail, so this `print` raises `OSError`
  as `f.write` does, and it changes the file, so a function that prints to a parameter declares
  it `InOut[File]`.
- **`print(..., file=sys.stderr)` never fails.** Where `file=` names `sys.stderr` or `sys.stdout`
  itself, `print` raises nothing, as `print` without `file=` raises nothing, so a function that
  reports a warning needs no `Raises`. A stream held in a variable is a file like any other:
  after `out = sys.stderr`, `print("x", file=out)` raises `OSError`.
- **`flush=True` writes the output out now.** Standard output holds what is printed and writes
  it when its buffer fills, when the program ends, at `sys.exit` and before a panic's message.
  `print(..., flush=True)` writes it at once, for a progress line that must appear while the
  program still runs. `flush=` takes any `bool`. Standard error holds nothing back, so it needs
  no flush.
- **`input()` reads one line.** `input(prompt)` writes the prompt to standard output with no
  line end, writes standard output out, and returns the next line of standard input without its
  line end. At the end of the input it raises `EOFError`, an error type with no fields that
  needs no import: `Raises[str, EOFError]`. A byte that is not UTF-8 is read as `�` (U+FFFD),
  where `sys.stdin.readline()` raises `OSError` with the kind `INVALID_DATA`.
- **Each read of `sys.stdout` gives a new handle on the one stream.** `out = sys.stdout` then
  `out.write("x")` and `print("y")` write `xy` in that order: there is one buffer, whichever
  handle writes. `close()` on a handle writes the stream out and leaves it open, and dropping a
  handle does nothing, so the stream outlives every handle.
- **The streams cannot be replaced.** `sys.stdout = log` is a compile error, since a module has
  no variables. To send output somewhere else, pass the file: `print(..., file=log)`.
- **Writing to `sys.stdin` or reading `sys.stdout` raises `OSError`**, as it does for a file
  opened the other way. `print(..., file=sys.stdin)` is a compile error.
- **A closed pipe ends the program.** When the reader of standard output goes away, as in
  `baryon run | head -1`, the next write ends the program quietly, as it ends a C program.
  Python instead raises `BrokenPipeError` and prints a traceback.

## Paths and directories

`pathlib.Path` holds a path, and `/` joins one to a name or another path. Its parts read the text
and never touch the file system: `name` is the last component, `stem` the name without its suffix,
`suffix` the name's last `.` and what follows, such as `.csv`, and `parent` the path without its
last component. A `Path` keeps its text in the form Python's `pathlib` keeps on Linux and macOS:
`Path("runs//out/")` is `runs/out`, `./a` is `a`, the empty text is `.`, and `..` stays, since
removing it would change what a path through a symbolic link names. Each part gives what
CPython 3.13 gives for the same text.

```python
import os
from pathlib import Path


def save(root: Path, step: int, energy: float) -> Raises[None, OSError]:
    os.makedirs(root, exist_ok=True)
    out = root / f"step_{step}.csv"
    out.write_text(f"{energy}\n")
    print(out, out.name, out.stem, out.suffix, out.parent)


def main() -> Raises[None, OSError]:
    save(Path("runs") / "2026", 3, -1.5)
    for entry in Path("runs/2026").iterdir():
        if entry.is_file():
            print(entry.read_text(), end="")
```

prints `runs/2026/step_3.csv step_3.csv step_3 .csv runs/2026`, then `-1.5`.

| Python | What it gives |
| --- | --- |
| `Path(text)` | the path, from one `str` or `Path` |
| `path / other` | the two joined; `other` is a `Path` or a `str`, and one that starts with `/` replaces `path` |
| `path.name`, `path.stem`, `path.suffix` | a `str` |
| `path.parent` | a `Path` |
| `path.exists()`, `path.is_file()`, `path.is_dir()` | a `bool`, following symbolic links; `False` when the system cannot tell |
| `path.iterdir()` | a `list[Path]` of the directory's entries, in the order the system gives |
| `path.read_text()` | the file's text, a `str` |
| `path.write_text(text)` | the number of characters written; the file is made or emptied first |
| `str(path)`, `print(path)`, `f"{path}"` | the text |
| `path == other`, `path != other` | whether the two texts are equal |
| `path.copy()` | a new `Path` with the same text |

The directory functions of `os` take a path as a `str` or a `Path`, as everything that takes a
path does, `open` included:

| Python | What it does |
| --- | --- |
| `os.listdir(path)`, `os.listdir()` | the names in the directory, `.` when left out, as a `list[str]` in the order the system gives |
| `os.mkdir(path)` | makes one directory |
| `os.makedirs(path)`, `os.makedirs(path, exist_ok=True)` | makes the directory and every missing one above it |
| `os.remove(path)` | removes a file |
| `os.rename(source, destination)` | moves `source`, replacing a file at `destination` |
| `os.getcwd()` | the directory the program runs in, a `str` |

Everything that reads or changes the file system but `exists`, `is_file` and `is_dir` can fail,
with an `OSError` that carries Python's `errno`, text and file name:
`os.mkdir("runs")` on a directory that exists raises `ErrorKind.FILE_EXISTS`, errno 17,
`File exists`, `runs`. A directory entry whose name is not UTF-8 is an `OSError` of the kind
`INVALID_DATA` naming the directory, since a program could not open that name again.

Three things differ from Python. `iterdir()` gives a list rather than an iterator, so `len` works
on it. A `Path` inside a list, a tuple, a `dict` or a struct cannot be printed yet, where Python
shows `PosixPath('runs')`: print each path on its own, or keep `str(path)`. Paths compare with
`==` and `!=` only; Python orders them part by part, which is not built. A part cannot be
assigned, `path.name = "x"`, as in Python: make a new path, `path.parent / "x"`.

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

Every part of decision 0036 now builds. A name the decision leaves for later is reported by
name: `` `sys.path` is not in Baryon yet ``. Idea I.39 lists what comes later,
`subprocess.run(check=True)` and a keyword of `subprocess.run` given a value that may be `None`
among them. The rest of `os` and `pathlib`, such as `os.chdir`, `Path.unlink` and `Path.glob`,
is not part of decision 0036 and is reported the same way.
