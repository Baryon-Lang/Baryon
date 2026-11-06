# 0014. Numbers mix the way Python's do, and conversions are calls

Status: Accepted
Date: 2026-11-06
Roadmap: I.7

## Context

[Decision 0009](0009-number-mixing.md) settled how numbers mix in arithmetic, and told the
programmer to convert between widths with Rust's `x as f64`. That spelling is not valid Python:
`as` appears in Python only in `import`, `with`, `except` and `match`.
[Decision 0004](0004-python-syntax.md) requires every program to be valid Python syntax, so 0009
could not be implemented as written. This record restates 0009 with a spelling that parses, and
changes nothing else. It supersedes 0009.

## Decision

- `/` always returns a float and `//` is floor division, as in Python 3.
- Number literals take the type their context needs, as Go's untyped constants do: `2 * x` works
  whether `x` is `f32` or `f64`.
- In arithmetic that mixes an integer and a float, the integer converts to the float's type.
- Numbers of the same kind but different widths never mix implicitly: `f32` with `f64` and `i32`
  with `i64` are errors that suggest an explicit conversion.
- A conversion is a call to the target type: `f64(x)`, `i32(n)`. This is Python's own spelling for
  `int(x)` and `float(x)`, and NumPy's for `np.float64(x)`.

## Consequences

- The design plan's `mean` example compiles as written, and so does almost any Python arithmetic.
- Precision never changes without a conversion call in the source.
- Converting a large `i64` to `f64` can lose precision silently, as it does in Python and NumPy.
  The type checker may warn on conversions that a constant shows to be lossy.
- Type names are callable, so a type and a function share one namespace. The language reference
  defines what each conversion does with a value that does not fit.
