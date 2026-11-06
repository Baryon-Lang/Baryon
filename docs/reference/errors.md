# Errors and panics

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
part of a hierarchy: `Exception` is a marker, as `Struct` is, and an error type cannot have another
error type as its base.

## Functions that can fail

A function that can fail says so in its return type with `Raises[T, E]`: it returns a `T`, or fails
with an `E`. Several error types are written as a union, `Raises[T, ParseError | IOError]`, and a
function that returns nothing on success uses `Raises[None, E]`.

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
  name with `as`.
- A `try` handles the errors of the calls in its body; an error type no `except` names passes on to
  the caller, which must allow it.
- `try` and `except` compile to a branch on the returned value. When no error occurs, the cost is
  that one branch per fallible call, the same as checking a `Result` in Rust.

The editor marks every call that can fail, because the source alone does not show which calls may
return early.

## `main`

`main` may fail: `def main() -> Raises[None, ConfigError]`. An error that reaches the end of `main`
prints its type and fields and ends the program with exit status 1.

## Panics

These panic:

| Failure | Message names |
| --- | --- |
| Index or slice out of range | The index, the dimension and its length |
| Array shapes that do not match in whole-array arithmetic | Both shapes |
| `assert` with a false condition | The condition's source and the message, if given |
| Integer division or `%` by zero | The operation |
| A conversion that cannot hold the value, such as `u8(300)` | The value and the target type |
| Integer overflow, in debug builds | The operation and its operands |
| A negative exponent in integer `**` | The exponent |

A panic prints its message and the source location, with a backtrace in debug builds, and ends the
program with a non-zero exit status. A panic in any iteration of a parallel loop ends the whole
program.

Panics cannot be caught, by `try` or anything else. A bug that could be caught would be a bug that
could be ignored, and a simulation that ignores an index error produces wrong numbers that look
right. This is the largest difference from Python, where every failure is an exception.
