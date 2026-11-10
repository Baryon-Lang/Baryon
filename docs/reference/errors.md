# Errors and panics

[What builds today](status.md#errors-and-panics) says which parts of this chapter the compiler
builds into programs yet.

A program meets two kinds of failure, and the language keeps them apart:

- An **error** is a failure the program expects and can handle: a solver that does not converge, a
  file that does not exist. Errors use Python's `raise` and `try`, but compile to ordinary return
  values ([decision 0005](../decisions/0005-error-handling.md)).
- A **panic** is a bug: an index out of range, a failed `assert`. It stops the program and cannot
  be caught.

## Declaring an error type

An error type is a class with the base `Exception`. Its fields are declared like a struct's:

```python
class ConvergenceError(Exception):
    """Conjugate gradient did not reach the tolerance within the allowed iterations."""

    iterations: int
    residual: f64
```

`ConvergenceError(1000, 3.2e-6)` builds one, with arguments in field order. An error type is not
part of a hierarchy ([decision 0015](../decisions/0015-no-class-inheritance.md)): `Exception`
is a marker, as `Struct` is, and an error type cannot have another error type as its base. Where
Python code would catch a base class, list the error types instead: in `Raises` as a union, and
in `except` as a tuple.

### Error types the language provides

Two error types need no declaration or import, as in Python. Each is an ordinary class of the
standard library's own Baryon source ([decision 0038](../decisions/0038-standard-library-in-baryon.md)),
so it is raised, caught and read like one of the program's own.

| Type | Fields | Raised by |
| --- | --- | --- |
| `ValueError` | `message: str` | `int(text)` and `float(text)` of text that is not a number ([decision 0037](../decisions/0037-reading-numbers-from-text.md)), and any program that raises it |
| `OSError` | `kind`, `errno`, `strerror`, `filename` | Files, the environment and processes ([Files and processes](files-and-processes.md)) |

```python
def positive(mass: f64) -> Raises[f64, ValueError]:
    if mass <= 0.0:
        raise ValueError("mass must be positive")
    return mass
```

A module that declares its own class of the same name uses its own, since the library never
hides a program's names.

## Functions that can fail

A function that can fail says so in its return type with `Raises[T, E]`: it returns a `T`, or fails
with an `E`. Several error types are written as a union, `Raises[T, ParseError | IOError]`, or as
an alias of one, `ReadErrors = ParseError | IOError`, and a function that returns nothing on
success uses `Raises[None, E]`.

```python
def solve(
    matrix: CsrMatrix, b: Array[f64, 1], tolerance: f64, max_iterations: int
) -> Raises[Array[f64, 1], ConvergenceError]:
    ...
    raise ConvergenceError(max_iterations, sqrt(residual_squared))
```

- `raise E(...)` inside the function returns the error to its caller. Nothing unwinds: the error
  travels back as a return value, and every local value is destroyed on the way out, as on any
  return.
- `raise` is allowed only in a function whose `Raises` lists that error type.

## Calling a function that can fail

Inside a function that can fail, calling another fallible function passes its errors on
automatically, as an exception would propagate in Python. Every error type the callee can raise
must be in the caller's `Raises`, or be handled by a `try` around the call; the compiler checks
this and names the error type that is not covered.

A function that cannot fail must handle every error of every fallible call it makes:

```python
def main():
    try:
        x = solve(matrix, b, 1e-10, 1000)
        print(f"converged; centre value {x[centre]:.6f}")
    except ConvergenceError as error:
        print(f"no convergence after {error.iterations} iterations")
```

- Each `except` names one error type, or a parenthesized tuple of them, and may bind the error to a
  name with `as`. A name bound by a tuple `except` has no single type, so its fields cannot be
  read; give each error type its own `except` to read them.
- A bare `raise` inside an `except` block raises the caught error again, so the function must
  declare it in its `Raises` or a `try` around the block must handle it.
- Errors do not chain: `raise ... from` is an error. An error that needs the one that led to it
  keeps it in a field, `raise LoadError(path, cause=error)`, where a handler can read it
  ([decision 0034](../decisions/0034-no-error-chaining.md)).
- A `try` handles the errors of the calls in its body; an error type no `except` names passes on to
  the caller, which must allow it.
- `try` and `except` compile to a branch on the returned value. When no error occurs, the cost is
  that one branch per fallible call, the same as checking a `Result` in Rust.

The editor marks every call that can fail, because the source alone does not show which calls may
return early.

## `main`

`main` may fail: `def main() -> Raises[None, ConfigError]`. An error that reaches the end of `main`
prints to standard error as [`print`](syntax.md#what-print-writes-for-each-value) writes a struct,
its type and each field by name, a `str` quoted as Python's `repr` writes it, and ends the program
with exit status 1:

```text
error: ConfigError(line=3, strict=True, path='heat.toml')
```

## Panics

These panic:

| Failure | Message names |
| --- | --- |
| Index out of range | The index, the dimension and its length |
| Slice out of range | Its start and stop, the dimension and its length |
| A slice step of zero | Nothing more: `slice step cannot be zero`, as Python says |
| Array shapes that do not match in whole-array arithmetic | Both shapes |
| `assert` with a false condition | The condition's source and the message, if given |
| Integer division or `%` by zero | The operation |
| A conversion that cannot hold the value, such as `u8(300)` | The value and the target type |
| Integer overflow, in debug builds ([Types](types.md#integer-overflow)) | The operation and its operands |
| A negative exponent in integer `**` | The exponent |
| A `range` whose step is zero, where Python raises `ValueError` | Nothing more: the step is zero |

A panic prints its message and the source location, with a backtrace in debug builds, and ends the
program with exit status 101, so a script can tell a panic from an error that reached the end of
`main` (status 1). A panic in any iteration of a parallel loop ends the whole
program.

The backtrace lists the calls that led to the panic, most recent last as Python's traceback does,
each as the function and its file, line and column. Calls the optimizer inlined keep their own
line:

```text
panic: index 4 is out of range for dimension 0 of length 4
  --> main.byn:2:19
backtrace, most recent call last:
  main.main at /home/ada/heat/main.byn:11:11
  main.total at /home/ada/heat/main.byn:7:12
  main.pick at /home/ada/heat/main.byn:2:19
```

Debug and release builds both include the debug information a backtrace needs
([`baryon build`](commands.md#debug-and-release-builds)), so every panic prints one. A program without that information
prints the message and location only.

Panics cannot be caught, by `try` or anything else. A bug that could be caught would be a bug that
could be ignored, and a simulation that ignores an index error produces wrong numbers that look
right. This is the largest difference from Python, where every failure is an exception.
