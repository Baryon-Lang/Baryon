# 0004. Every source file is valid Python syntax

Status: Accepted
Date: 2026-11-06
Roadmap: I.1

## Context

The design plan says "if Python has a spelling for it, use Python's spelling," but still adds a
few keywords Python lacks: `struct`, `const`, `inout`, `parallel for`, `simd for`, `unsafe:` and
`a[i]!`. Each one means a custom lexer and parser, and editors, formatters and highlighters that
do not understand the files.

Going one step further, so that every file parses as Python 3.12, is the route Numba, Taichi,
Triton and Pythran took. It changes how the language looks in a handful of places and how much of
the compiler must be written from scratch.

## Decision

Every source file is valid Python 3.12 syntax. The compiler gives that syntax static types and
compiled semantics; it never extends the grammar. The constructs Python has no spelling for are
written as follows:

| Design plan | This decision |
| --- | --- |
| `struct P:` | `class P(Struct):` |
| `const X = 1` | `X: Final = 1` (Python's own `typing.Final`) |
| `y: inout Array[f64]` | `y: InOut[Array[f64]]` (see decision 0006) |
| `parallel for i in range(n):` | `for i in prange(n):` |
| `simd for i in range(n):` | `for i in simd(range(n)):` (see decision 0011) |
| `unsafe:` | `with unsafe:` |
| `a[i]!` | `a.unchecked[i]`, only inside `with unsafe:` (see decision 0010) |
| `#[test]`, `#[bench]` | `@test`, `@bench` |
| `x = f()?` | no equivalent; see decision 0005 |

The front end parses with an existing Python parser written in Rust, behind an interface crate so
the parser stays replaceable (decision 0001).

## Consequences

- About 7 weeks of planned work shrink to integration: the hand-written lexer (1.1), most of the
  parser (1.2), the tree-sitter grammar (1.3) and the formatter (1.15).
- Every editor, GitHub and every Python formatter and highlighter handle the files from day one.
- With a small shim module (`Array` over NumPy, `f64 = float`, `InOut[T] = T`), most programs also
  run under CPython. The test suite can compare compiled output with CPython output, which gives a
  correctness oracle before the compiler is complete.
- The language can never add a keyword. Every new construct must be spelled as a call, a
  decorator, a context manager, a subscript or an annotation.
- Looking exactly like Python raises the expectation of Python semantics: reference types,
  exceptions, duck typing. Error messages must be written for Python programmers, and the
  documentation needs a "differences from Python" page.
- The file extension and the parser's error messages must make clear the file is not CPython
  code, even though CPython parses it.
