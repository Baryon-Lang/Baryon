# Language reference

This is the definition of the language: what a program may say and what it means. The compiler
implements this document, and where the two disagree, one of them has a bug. It is roadmap item
0.1.

This is version 0. It covers what the Phase 1 compiler must build, which is enough to write the
[example programs](../examples/README.md) and the simulations the MVP gate names. Anything it does
not mention is not in the language yet; the compiler rejects it with a message, never silently.

The compiler does not build all of version 0 yet. [What builds today](status.md) says, chapter by
chapter, which features `baryon build` compiles into a working program and which it still rejects.

## How it relates to the other documents

- The [decision records](../decisions/README.md) settle the large choices. This reference applies
  them and fills in the detail. Where the [design plan](../design/language-design-plan.md) and a
  decision record differ, the decision record wins.
- Every rule says why it is there, in a sentence, unless the reason is obvious. A rule with a
  cost says what the cost is.
- Every code block is valid Python 3.12 syntax, as
  [decision 0004](../decisions/0004-python-syntax.md) requires of every program.

## Chapters

| Chapter | What it defines |
| --- | --- |
| [Syntax](syntax.md) | Source files, which Python constructs the language accepts, and the names it adds |
| [Types](types.md) | Numbers, number mixing, strings, collections, structs, enums, generics, type inference, constants |
| [Values and references](values-and-references.md) | Moves, copies, borrows, and the exclusivity check, written as a decision procedure |
| [Arrays](arrays.md) | `Array` and `Fixed`: construction, layout, indexing, slicing, whole-array arithmetic |
| [Errors and panics](errors.md) | `Raises`, `raise`, `try`/`except`, and failures that stop the program |
| [Parallel and vector loops](parallel-loops.md) | `prange`, reductions, the disjoint-write rule, adding into shared arrays with `accumulate=`, and `simd` |
| [Tests](tests.md) | `@test` functions and how `baryon test` runs and reports them |
| [Linear algebra and FFT](linear-algebra.md) | `linalg` and `fft`: solving systems, factoring matrices and Fourier transforms, with NumPy's names, and `LinAlgError` |
| [Files and processes](files-and-processes.md) | Command-line arguments, the exit status and environment variables: `sys` and `os` |
| [Data files](data-files.md) | Text tables of numbers and the other data file formats, with NumPy's names: `Array.loadtxt` and `Array.savetxt` |
| [Differences from Python](differences-from-python.md) | Every place a Python programmer will be surprised, with the reason |
| [The `baryon` command](commands.md) | Every command of the `baryon` tool: what it needs, what it prints, and its exit status |
| [The `baryon` command](commands.md) | Every command of the `baryon` tool: what it needs, what it prints, and its exit status |
| [What builds today](status.md) | Which of the above the compiler builds into a working program today, and what it still rejects |

A reader who knows Python should start with [Differences from Python](differences-from-python.md),
then read [Values and references](values-and-references.md), the one chapter with no Python
equivalent.

## Terms

These words have one meaning throughout the reference.

| Term | Meaning |
| --- | --- |
| value | A piece of data: a number, an array, a struct |
| owner | The one variable, field or element that holds a value. Every value has exactly one owner |
| place | Something that can hold a value and be written: a variable, or a path from one such as `atoms.forces[i]` |
| move | Transferring a value to a new owner. The old owner can no longer be used |
| copy | Duplicating a value, leaving the original untouched |
| borrow | Lending a value to a function for the length of one call, without moving it |
| panic | A failure that stops the program, such as an index out of range. It cannot be caught |
| compile-time value | A value the compiler knows before the program runs, such as a `Final` constant |

## Open questions

These choices are not settled. Each is a roadmap decision with an owner; until it is made, the
reference leaves the feature out.

| Question | What is missing until it is decided | Roadmap |
| --- | --- | --- |
| How user code declares a trait and how a type implements it. Decided in [decision 0031](../decisions/0031-traits-are-protocols.md) and [specified](types.md#declaring-a-trait); it builds (roadmap 1.7.10, 1.7.12, 1.9.19) | `Dyn[Body]`, until roadmap 3.5 | D.9 |
| How to spell an enum whose variants carry data (a sum type). Decided in [decision 0032](../decisions/0032-sum-types-are-unions-of-structs.md), a union of structs; the checker does not implement it yet (roadmap 1.7.13) | Enums are plain named constants | D.10 |
| Whether a mutable argument is marked at the call site | No marker, per [decision 0006](../decisions/0006-parameter-passing.md) | D.3 |

## Not yet specified

These are planned and will get their sections as the work that needs them starts: column-major
array layout ([decision 0008](../decisions/0008-row-major-arrays.md) promises an opt-in), the
`stencil` loop form, `with fastmath:`, closures, generators, `dict` and `set` comprehensions,
and the full method lists of the standard library types.
