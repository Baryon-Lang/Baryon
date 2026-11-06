# 0009. Numbers mix the way Python's do, within one width

Status: Accepted
Date: 2026-11-06
Roadmap: I.7

## Context

The design plan forbids implicit conversion between numeric types. Its own example, `mean`,
returns `total / len(x)`: a float divided by an integer, which those rules reject. Python, NumPy
and Julia all let integers and floats mix in arithmetic. What they guard against, and what causes
real bugs in numeric code, is precision changing silently, for example a 64-bit float quietly
becoming a 32-bit one.

## Decision

- `/` always returns a float and `//` is floor division, as in Python 3.
- Number literals take the type their context needs, as Go's untyped constants do: `2 * x` works
  whether `x` is `f32` or `f64`.
- In arithmetic that mixes an integer and a float, the integer converts to the float's type.
- Numbers of the same kind but different widths never mix implicitly: `f32` with `f64` and `i32`
  with `i64` are errors that suggest an explicit conversion, written as a call: `f64(x)`.

## Consequences

- The plan's `mean` example compiles as written, and so does almost any Python arithmetic.
- Precision never changes without a conversion call, such as `f32(x)`, in the source.
- Converting a large `i64` to `f64` can lose precision silently, as it does in Python and NumPy.
  The type checker may warn on conversions that a constant shows to be lossy.
