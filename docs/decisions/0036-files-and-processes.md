# 0036. Files and processes use Python's names, built natively in the runtime

Status: Proposed
Date: 2026-11-10
Roadmap: 1.13.6

## Context

A Baryon program today can compute and `print`, and nothing else: it cannot read an input file,
write a result, take a command-line argument, read an environment variable, set its exit status or
run another program. Every real simulation and analysis does at least the first three. Roadmap
1.13.6 adds them, and it leads release 0.3.0. The interface is user-visible, so this record
proposes it before anything is built.

The forces:

1. **It should read like Python.** A Python programmer reaches for `open`, `with`, `sys.argv`,
   `os.environ`, `pathlib.Path` and `subprocess.run` without thinking. Every name Baryon shares
   with Python must mean what it means there, or be a compile error that says why.
2. **It must be native Baryon** (Collins, 2026-11-10: "true Baryon functionality, not Baryon
   wrapped in Python"). No Python interpreter, no Python library underneath.
3. **It must fit the rules the language already has.** Errors are values declared with
   `Raises[T, E]` ([decision 0005](0005-error-handling.md)), and error types form no hierarchy
   ([decision 0015](0015-no-class-inheritance.md)), so Python's `except OSError` catching
   `FileNotFoundError` cannot work as it does there. A value has one owner and is dropped when
   that owner ends ([Values and references](../reference/values-and-references.md)). A module
   has no variables ([Syntax](../reference/syntax.md#source-files)). `with` exists only as
   `with unsafe:`. There is no `bytes` type.
4. **Data loss must never be silent.** A result file that failed to reach the disk, and a program
   that still exits with status 0, is a wrong answer that looks right.
5. **Large data is the point.** Writing a 10 GB array must cost one pass over its bytes, not a
   loop of formatted writes.
6. **Linux and macOS now, Windows in 0.4.0** (roadmap R.4). Nothing here may depend on a POSIX
   detail that Windows cannot provide, but nothing is built for Windows yet.

Text processing, such as `str.split`, `strip`, slicing and `float(text)`, is a separate 0.3.0
workstream (roadmap 1.13.12). This record relies on it and does not define parsing.

## Decision

Python's modules and names, a subset of each, with one error type. Collins chose parts 1 to 4
on 2026-11-10, each the recommended option: Python's names, one `OSError` with a `kind`,
`with open(...)`, and arrays as the only binary data in 0.3.0. The rest of the record follows
from those choices and the rules above.

### 1. Python's modules and names

| Python | Baryon | Notes |
| --- | --- | --- |
| `open(path, mode)` | the same, built in | Modes `"r"`, `"w"`, `"a"` and `"x"`: text only (part 4) |
| `sys.argv` | the same | A `list[str]`; `sys.argv[0]` is the program as it was started |
| `sys.exit(status)` | the same | Part 5 |
| `sys.stdin`, `sys.stdout`, `sys.stderr` | the same | Text streams with the file methods below |
| `input(prompt)` | the same | Part 6 |
| `print(..., file=, flush=)` | the same | `file=` takes `sys.stdout`, `sys.stderr` or an open file |
| `os.environ`, `os.getenv` | the same, read-only | Part 5 |
| `os.listdir`, `os.remove`, `os.rename`, `os.mkdir`, `os.makedirs`, `os.getcwd` | the same | Each raises `OSError` |
| `pathlib.Path` | the same | Part 7 |
| `subprocess.run` | the same, with an argument list | Part 8 |

The modules are imported as in Python: `import sys`, `from pathlib import Path`. Until the standard
library can be written in Baryon, the compiler builds them in, as it builds in `math` and `random`
today. A program that uses a name Python has and this table does not, such as `os.system` or
`shutil.copy`, gets a compile error that names it as not yet in Baryon, never a different meaning.

**Alternative:** one Baryon module, `io`, with names of our own. It would be smaller and free of
Python's historical splits, but every Python programmer would have to learn it, and every ported
script would need its I/O rewritten. **Chosen: Python's names.**

### 2. One error type, `OSError`, with a `kind`

Every operation in this record that can fail raises one type:

```python
class OSError(Exception):
    kind: ErrorKind   # what went wrong, for a program to test
    errno: i32        # the operating system's error number, 0 when there is none
    strerror: str     # the operating system's message, such as "No such file or directory"
    filename: str     # the path involved, or "" when there is none
```

The field names are Python's `OSError` attributes. `ErrorKind` is an `Enum` whose members are
named after Python's `OSError` subclasses: `FILE_NOT_FOUND`, `FILE_EXISTS`, `PERMISSION`,
`IS_A_DIRECTORY`, `NOT_A_DIRECTORY`, `BROKEN_PIPE`, `INTERRUPTED`, `TIMEOUT`, `INVALID_DATA`
(text that is not UTF-8, an array file whose size is not a whole number of elements) and `OTHER`.

```python
import sys
from os import ErrorKind


def main() -> Raises[None, OSError]:
    try:
        config = open("heat.toml").read()
    except OSError as error:
        if error.kind == ErrorKind.FILE_NOT_FOUND:
            print(f"no {error.filename}; using the defaults", file=sys.stderr)
        else:
            raise
```

**Alternative:** Python's separate types, `FileNotFoundError`, `PermissionError` and the rest.
Without a hierarchy each would be its own type, so `open` would declare
`Raises[File, FileNotFoundError | PermissionError | IsADirectoryError | OSError]`, every caller
would repeat the union, and a function that adds one more failure would change every signature
above it. **Chosen: one `OSError` with a `kind`.** A ported `except FileNotFoundError` becomes
`except OSError` and a test of `kind`; the compile error says so.

### 3. Files close when their owner ends, and `with open(...)` is allowed

`open` returns a `File`. A `File` has one owner, moves and is not `Copy`, like a `list`. It closes
when its owner ends, so a file is never left open by a forgotten `close()`:

```python
def write_energies(path: str, energies: list[f64]) -> Raises[None, OSError]:
    with open(path, "w") as out:
        for energy in energies:
            out.write(f"{energy:.10e}\n")
```

`with open(...) as f:` becomes the second `with` form after `with unsafe:`. The file closes at the
end of the block, as in Python, and a failure to close (a full disk discovered at the last flush)
raises `OSError` from the `with` statement, so the enclosing function must allow it or handle it.
`f.close()` does the same explicitly. Any other `with`, such as a lock, stays a compile error.

A `File` written to and then dropped without `with` or `close()`, whose last flush fails, panics,
naming the path: a drop cannot raise, and force 4 rules out ignoring it. `with` and `close()` are
how to handle that failure as an error.

**Alternative:** no `with`; files close on drop and through `close()` only. One fewer construct,
but the most common Python I/O idiom would not compile. **Chosen: allow `with open(...)`.**

### 4. Text files are UTF-8; binary data is whole arrays

`open` reads and writes text. The encoding is UTF-8 and nothing else; `encoding="utf-8"` is
accepted and any other encoding is a compile error. Reading translates `\r\n` and `\r` to `\n`, as
Python's universal newlines do; writing writes `\n` unchanged, as Python does on Linux and macOS.
Text that is not valid UTF-8 raises `OSError` with kind `INVALID_DATA`, naming the file and the
byte offset.

| Method | Meaning |
| --- | --- |
| `f.read()` | The rest of the file as one `str` |
| `f.readline()` | The next line with its `\n`, or `""` at the end |
| `f.readlines()` | The remaining lines as a `list[str]` |
| `for line in f` | Each remaining line with its `\n`, as in Python; the loop borrows `f` |
| `f.write(text)` | Writes `text` and returns the number of characters, as Python does |
| `f.flush()`, `f.close()` | Flush the buffer; flush and close |

Writes go through a 64 KiB buffer, so a loop of small writes costs one system call per 64 KiB.

Binary data is arrays, with NumPy's spelling and NumPy's bytes:

```python
temperature.tofile("field.bin")                       # raw elements, row-major, native order
field = Array.fromfile("field.bin", f64)              # an Array[f64, 1]
block = Array.fromfile("field.bin", f64, count=1024, offset=8192)
```

`a.tofile(path)` writes the elements as they lie in memory, and `Array.fromfile(path, dtype)` reads
them back, as `numpy.ndarray.tofile` and `numpy.fromfile` do, so the files are shared with NumPy
byte for byte on every platform Baryon supports (all little-endian). Each is one system call per
large block, with no per-element work (force 5). `dtype` may be any number type, `bool`, or a
`Copy` struct of them; `offset` is in bytes, as in NumPy.

**Alternative:** add a `bytes` type now, with `open(path, "rb")`, so any binary format can be
read. That is a new core type with its own literals, methods and printing, and every binary format
scientists use (raw arrays now; `.npy`, HDF5 and the rest under roadmap 3.14) is better served by
reading arrays directly. **Chosen: arrays only in 0.3.0;** a `"b"` mode is a compile error
that points to `Array.fromfile` and `tofile`, and `bytes` waits for a format that needs it.

### 5. Arguments, environment and exit status

- `sys.argv` is a `list[str]`. Each read of `sys.argv` gives the program a new list of the
  arguments, which costs a copy of a few strings, so a program changing its list
  (`sys.argv.pop(0)`) changes only its own copy. This is how a module with no variables can still
  have `sys.argv`.
- `os.environ` reads the environment. `os.environ["HOME"]` panics when the name is not set, as a
  missing `dict` key does ([decision 0035](0035-dict.md)); `os.environ.get(name)` gives
  `str | None`, `os.getenv(name, default)` a `str`, `name in os.environ` a `bool`, and
  `os.environ.copy()` a `dict[str, str]`. Arguments and environment that are not UTF-8 raise
  nothing: they are read with each bad byte replaced by U+FFFD.
- **The environment is read-only.** `os.environ[name] = value` and `os.putenv` are compile errors.
  Changing the environment of a running process is a data race with any thread reading it in the
  C library, and `prange` makes every Baryon program threaded. A program that needs another
  environment passes one to the program it runs, `subprocess.run(..., env=...)`.
- `sys.exit(status)` flushes `print`'s output and every open file, then ends the program with
  `status`, an `int` from 0 to 255; any other value panics. Status 1 is what an error reaching
  the end of `main` uses, and 101 is a panic, as before. Unlike Python's, `sys.exit` cannot be
  caught: it is not an exception, and it does not run the rest of the program on the way out.

### 6. Standard streams

`sys.stdin` reads text as a file opened with `"r"` does; `sys.stdout` and `sys.stderr` write text
as files opened with `"w"` do, with `print` sharing `sys.stdout`'s buffer so their output never
interleaves out of order. `input(prompt)` writes the prompt, flushes, and returns the next line of
standard input without its `\n`; at the end of input it raises `EOFError`, as Python does, an
error type with no fields. A write to a closed pipe, as in `baryon run sim | head`, ends the
program quietly with the signal, as C programs do, rather than raising from `print`.

### 7. Paths

`pathlib.Path` is a struct holding one `str`, moved like a `str`, with Python's operations:
`Path("runs") / name / "out.csv"` joins, `.name`, `.stem`, `.suffix`, `.parent`, `.exists()`,
`.is_file()`, `.is_dir()`, `.read_text()`, `.write_text(text)`, `.iterdir()` (a `list[Path]`, in
the order the operating system gives) and `str(path)`. Every function in this record that takes a
path takes a `str` or a `Path`, as Python's do. A path on Linux may hold bytes that are not UTF-8;
listing a directory that holds such a name raises `OSError` with kind `INVALID_DATA` naming the
directory, rather than handing the program a name it cannot open again.

### 8. Running a program

```python
import subprocess


def git_revision() -> Raises[str, OSError]:
    result = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True)
    if result.returncode != 0:
        return "unknown"
    return result.stdout.strip()
```

`subprocess.run(args, capture_output=False, input=None, cwd=None, env=None, timeout=None)` runs
`args[0]`, found on `PATH`, with the rest as its arguments, waits for it, and returns a
`CompletedProcess` with Python's fields: `args`, `returncode`, `stdout` and `stderr`. Output is
text: `text=True` is accepted and the default, since there is no `bytes`. `returncode` is negative
when a signal ended the program, as in Python. A program that exits with a status other than 0 is
not an error, as in Python without `check=True`; a program that cannot be started raises `OSError`
(`FILE_NOT_FOUND`, `PERMISSION`), and a timeout raises `OSError` with kind `TIMEOUT` after ending
the program.

There is no shell. A `str` as `args`, or `shell=True`, is a compile error that shows the same
command as a list, because a shell parses its command line in ways that break on file names with
spaces and let a value become a command.

### 9. Building it

- The runtime crate does the work, through Rust's standard library: `std::fs`, `std::env` and
  `std::process`. Compiled code calls it through new `baryon_rt_*` functions, as it does for
  strings, lists and `dict` today. No Python and no new dependency; Rust's standard library already
  has a Windows implementation for 0.4.0.
- The open files with unflushed data are the one piece of process-wide state the runtime keeps,
  so that `sys.exit` can flush them, as C's `exit` flushes every open `FILE`.
- Writing to one `File` from every iteration of a `prange` loop changes a value the iterations
  share, so the exclusivity check rejects it, as it rejects any other shared write.
- The work is built in slices, each interface change in its own pull request: arguments, exit
  status and the environment; text files, `with` and the standard streams; array files; paths and
  directories; `subprocess.run`.

### Left for later

Each of these can be added without changing anything above, and idea I.39 tracks them: `bytes`
and binary file modes; NumPy's `.npy` format with shape and type (`save`, `load`); memory-mapped
arrays for files larger than memory; `check=True` and its `CalledProcessError`; `subprocess.Popen`
with pipes to a running program; `shutil`, `glob` and `tempfile`; `os.walk`; seeking and
`tell()`; and encodings other than UTF-8.

## Consequences

- Python programmers write file, argument and process code the way they already do, and a script
  ported from Python changes only where Baryon's rules differ, each with a compile error that says
  how: `except FileNotFoundError`, `"rb"`, `os.environ[...] = ...`, `shell=True`.
- Every operation that touches the system says so with `Raises[..., OSError]`, and a test of one
  `kind` field replaces a hierarchy of exception types.
- Results never vanish silently: a failed final flush raises from `with` or `close()`, and panics
  on a plain drop.
- Raw arrays move between Baryon and NumPy without conversion, at the speed of the disk.
- Programs cannot change their own environment, which Python can. The `env=` argument covers the
  common reason to.
- Without `bytes`, Baryon cannot read an arbitrary binary format until it gets one; formats
  scientists use come as array readers under roadmap 3.14.
- `sys.exit` ending at once, without unwinding, differs from Python's catchable `SystemExit`. No
  Baryon code can observe the difference except a `try` meant to catch it, which does not compile.
- The runtime grows by the parts of Rust's standard library these use; the size goal of roadmap
  1.12.4 already counts on `std`.
