# 0005. Errors use Python's `raise` and `try`, compiled to plain error returns

Status: Proposed
Date: 2026-11-06
Roadmap: I.1

## Context

The design plan returns `Result[T, E]` and propagates errors with Rust's `?` operator. `?` is not
valid Python, so decision 0004 rules it out. Python's exceptions, as CPython implements them, unwind
the stack and hide which calls can fail, which costs speed and makes control flow hard to follow.
Swift shows a third way: exception syntax, but every function that can fail says so in its
signature, and an error travels back as an ordinary return value.

## Decision

- A function that can fail declares it in its return annotation: `-> Raises[f64, IOError]`.
- `raise IOError(...)` inside it returns the error to the caller; nothing unwinds.
- A call to a fallible function inside another fallible function propagates its error
  automatically, as in Python. Calling one from a function that cannot fail requires a
  `try`/`except` that handles every error type it raises; the compiler checks this.
- `try`/`except` compiles to a branch on the returned error, with no runtime machinery.
- Unrecoverable bugs (index out of range, integer overflow in debug builds) are panics, not
  errors, and cannot be caught.

## Consequences

- Error handling reads exactly like Python, and every file stays valid Python syntax.
- Which functions can fail is part of their signature, so it is visible in the editor and checked
  by the compiler, as with Rust's `Result`.
- Failure costs a branch per fallible call, the same as Rust, and nothing when no error occurs.
- Propagation is implicit inside fallible functions, so a reader can't see from a call alone that
  it may return early. The editor will mark fallible calls to compensate.
- Catching a panic is impossible by design, which differs from Python, where everything is an
  exception. The "differences from Python" page must say so.
