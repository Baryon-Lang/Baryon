# 0022. A `str` is a pointer and a length, and a literal is never copied

Status: Proposed
Date: 2026-11-07
Roadmap: 1.10.8

## Context

`print` (roadmap 1.10.6) can write a string literal, but no `str` value exists in a compiled
program yet: no `str` variable, no field, no f-string. Every example program prints its results
with f-strings, and errors usually carry a message, so the backend needs `str` values next.

The language already fixes what a `str` means (the reference, "Strings", and "Values and
references"). It is immutable UTF-8 text. It has one owner and moves on assignment, and `copy()`
makes a second one. `types::layout` already gives it two words. What no record settles is the
machine form, and that is hard to change once the backend, the runtime and the passes rely on it:

1. **What the two words hold.** The obvious answer is a pointer to the bytes and their length.
   An immutable string never grows, so it needs no capacity word, unlike Rust's `String`.
2. **Who frees a literal.** `name = "heat"` makes an owned `str`, and dropping it must free what
   it owns. A literal's bytes are a constant in the program, which must never be freed. The
   options:
   - **Copy every literal to the heap** when it is evaluated. Drop always frees. This is
     simple, but every literal in a loop body costs an allocation each time round, and
     `print("step", i)` style code is common in loops.
   - **Mark a static string in its length.** The top bit of the length word says "not
     owned": a literal sets it, and drop frees only when it is clear. A literal costs nothing.
     Drop costs one test, and every length read masks the bit off. No real string is 2^63
     bytes long, so the bit is free.
   - **Count references**, so a copy is cheap too. This needs a header on every heap string,
     an atomic count once `prange` shares strings, and it changes `copy()` from a copy into a
     share. That is more machinery than an immutable value needs today.
3. **How an f-string is built.** `f"step {i}: {energy:.6f}"` needs number formatting at run
   time, which already lives in the runtime for `print`.

## Decision

- A `str` value is `{ ptr, i64 }`: a pointer to its UTF-8 bytes and a length word. The low 63
  bits of the length word are the byte count. The top bit is set when the bytes are not owned,
  as for a literal.
- A literal is a byte constant with the top bit set, never copied. Drop frees the bytes through
  `baryon_rt_deallocate` only when the bit is clear. `copy()` always allocates and makes an owned
  string, so a copy outlives nothing it points into.
- An f-string compiles to calls of a runtime string builder: start a buffer, append each
  literal piece and each formatted value with its format spec, then finish into an owned `str`.
  Values are formatted by the same code that formats `print`'s arguments, so `f"{x}"` and
  `print(x)` write the same text. A format spec such as `.6f` is checked at compile time, and the
  runtime gets its parts.
- `print` of a `str` value writes its bytes with `baryon_rt_print_str`, masking the bit off.

## Consequences

- A literal costs nothing at run time, so printing in a loop allocates only for f-strings.
- Every place that reads a length has to mask the bit: `len`, comparison, printing, slicing
  later. One helper in the backend keeps that in one place.
- A `str` cannot be longer than 2^63 bytes, a limit no machine reaches.
- If strings later need sharing across threads cheaply, reference counting can replace the flag
  under the same two words, since the flag already separates "free me" from "do not".
- The runtime gains a small string builder and number formatting with Python's format specs,
  the start of the standard library's `io` (roadmap 1.13).
