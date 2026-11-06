# 0015. Classes have no inheritance

Status: Accepted
Date: 2026-11-06
Roadmap: D.13

## Context

Every program is valid Python ([decision 0004](0004-python-syntax.md)), so `class` is the spelling
for the language's record types: `class P(Struct)` for a struct, `class E(Exception)` for an error
type, `class C(Enum)` for an enumeration. Python programmers will also expect `class B(A)` to make
`B` inherit `A`'s fields and methods, and to override them.

Inheritance as Python, C++ and Java implement it costs a numeric language three things:

- **Layout.** A value of type `A` may really be a `B` with more fields, so it cannot be stored
  inline at a size known when compiling. Arrays of structs stop being contiguous, or values are
  sliced when copied.
- **Speed.** An overridable method is called through a table at run time, which blocks inlining,
  and a loop that cannot inline its calls does not vectorize.
- **Reasoning.** Behavior is spread across a hierarchy, and a change to a base class changes every
  subclass.

Rust, Go, Zig and Julia, the closest language to this one's purpose, all leave inheritance out and
share behavior through traits or interfaces instead.

## Decision

- A class has no user-defined base. Its only bases are the language's markers: `Struct`,
  optionally with `Copy`; `Exception`; or `Enum`. `class B(A)` where `A` is a user class is a
  compile error whose message suggests a trait or a field holding an `A`.
- There is no method overriding, no `super()` and no abstract base class.
- Behavior shared between types comes from traits and generic functions. How user code declares a
  trait is roadmap decision D.9.
- Data shared between types comes from composition: a struct holds another struct as a field.
- Error types form no hierarchy either. A function that can fail in several ways lists each error
  type in its `Raises`, and `except (ParseError, IOError)` handles several at once.

## Consequences

- Every struct's size and layout follow from its own fields, so arrays of structs are contiguous
  and every method call is resolved when compiling, where it can be inlined.
- When a program needs to choose behavior at run time, it uses an enum with a `match`, or trait
  objects once they arrive in Phase 3, not a class hierarchy.
- Python code built on class hierarchies must be restructured into composition and traits when
  ported. The "differences from Python" page and the compile error say how.
- Leaving inheritance out keeps the choice open: it could be added later without breaking any
  program, while removing it after programs depend on it could not be done.
