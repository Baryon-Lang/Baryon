# 0034. `raise ... from` is rejected; a cause is kept in a field

Status: Proposed
Date: 2026-11-09
Roadmap: D.18, 1.9.17

## Context

Python's `raise LoadError(path) from error` raises `LoadError` and stores `error` as its
`__cause__`. Nothing in the program reads the cause; Python's traceback prints it, under "The
above exception was the direct cause of the following exception". `raise X from None` hides the
error being handled, which Python would otherwise print as the implicit `__context__`.

In this language an error is a value returned to the caller ([decision 0005](0005-error-handling.md)).
Each error type is a struct the program declares, so its fields are fixed and visible, and an
error has no traceback: only a panic prints a backtrace. When `main` ends with an error, the
program prints the error's type and fields ([Errors](../reference/errors.md)).

The checker accepted `raise ... from` and typed its cause, but neither the reference nor a
decision said what it means, and lowering reported it as not built. Roadmap 1.9.17 asked for it
to be built. There are three ways to read it:

1. **Reject it.** `baryon check` reports `from` with a note: keep the cause in a field of the new
   error, `raise LoadError(path, cause=error)`. `from None` is reported too, with a note that
   there is no implicit chain to hide.
2. **Accept it and drop the cause.** The cause is computed and destroyed; the error is raised on
   its own. Python code ports unchanged, but a cause the programmer wrote is lost without a word,
   including from the report when `main` ends with the error.
3. **Chain causes as Python does.** Each error carries a hidden, type-erased, heap-allocated
   cause, destroyed with it and printed after it when `main` ends with the error. Every error type
   grows by a pointer, and the runtime needs a printer and a destructor for any error type as a
   cause.

## Decision

Recommended: option 1, reject it. Collins decides.

- The syntax check reports `raise ... from` as not supported, at the `from` keyword, labelled
  "errors do not chain", with two notes: that an error reaches the caller without the error that
  led to it, and then either how to keep the cause in a field or, for `from None`, to remove it.
- An error type that needs its cause declares it as a field, typed as the cause's error type, so
  the code that catches it can read it like any other field.

## Consequences

- Nothing is silently ignored, which [Differences from Python](../reference/differences-from-python.md)
  promises for every construct version 0 leaves out.
- A cause kept in a field is typed, so a handler can match on it, and is printed with its error
  when `main` ends with one, since the report prints every field.
- Python code that uses `from` needs an edit when ported: drop `from None`, and move a cause into
  a field. The message says how.
- Option 3 stays open: accepting `from` later breaks no program that builds today, while
  rejecting it after accepting it would. Option 2 would have closed that door.
- Lowering never sees a `raise` with a cause, so roadmap 1.9.17 needs no lowering for it.
