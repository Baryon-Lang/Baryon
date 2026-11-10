# Decision records

A decision record captures one choice that is hard to reverse or that affects more than one crate:
what was decided, why, and what it costs. A record starts as `Proposed` and becomes `Accepted` or
`Rejected` when the project owner decides. Records are never edited after acceptance except to
change their status; a new record supersedes an old one.

## Format

Name the file `NNNN-short-title.md`, numbered in order, and use this outline:

```markdown
# NNNN. Title

Status: Proposed | Accepted | Rejected | Superseded by NNNN
Date: YYYY-MM-DD
Roadmap: the ROADMAP.md item IDs this record settles, if any

## Context

The problem and the forces at play.

## Decision

What we will do.

## Consequences

What becomes easier, what becomes harder, and what we give up.
```

## Index

| Record | Status |
| --- | --- |
| [0001. Modular crate architecture with enforced dependency rules](0001-modular-crate-architecture.md) | Accepted |
| [0002. The roadmap is the source of truth for project management](0002-roadmap-source-of-truth.md) | Accepted |
| [0003. A C++ and Fortran baseline suite in `bench/`, run by a Rust harness](0003-benchmark-suite.md) | Accepted |
| [0004. Every source file is valid Python syntax](0004-python-syntax.md) | Accepted |
| [0005. Errors use Python's `raise` and `try`, compiled to plain error returns](0005-error-handling.md) | Accepted |
| [0006. Parameters are read-only borrows unless marked `InOut` or `Owned`](0006-parameter-passing.md) | Accepted |
| [0007. One array family: `Array` on the heap, `Fixed` inline](0007-array-types.md) | Accepted |
| [0008. Arrays are row-major by default](0008-row-major-arrays.md) | Accepted |
| [0009. Numbers mix the way Python's do, within one width](0009-number-mixing.md) | Superseded by 0014 |
| [0010. Bounds checks stay on in release builds](0010-bounds-checks.md) | Accepted |
| [0011. `simd` loops are a checked request, not an unchecked promise](0011-checked-simd.md) | Accepted |
| [0012. No foreign function interface before Phase 4](0012-no-ffi-in-mvp.md) | Accepted |
| [0013. Parallel loops may add into shared arrays they declare with `accumulate=`](0013-parallel-accumulation.md) | Accepted |
| [0014. Numbers mix the way Python's do, and conversions are calls](0014-number-conversions-are-calls.md) | Accepted |
| [0015. Classes have no inheritance](0015-no-class-inheritance.md) | Accepted |
| [0016. Incremental queries run on the `salsa` crate](0016-salsa-query-engine.md) | Accepted |
| [0017. `@layout("C")` pins a struct's layout and silences the padding warning](0017-layout-decorator.md) | Accepted |
| [0018. The mid-level IR is basic blocks over typed locals and places](0018-mid-level-ir.md) | Accepted |
| [0019. The language server is `baryon lsp`, speaking the protocol over `serde_json`](0019-language-server-protocol.md) | Accepted |
| [0020. The first LLVM backend writes textual IR and hands it to `clang`](0020-llvm-backend-emits-text.md) | Accepted |
| [0021. The runtime is one Rust crate of its own kind, the only one outside a backend allowed `unsafe`](0021-runtime-crate-and-unsafe-code.md) | Accepted |
| [0022. A `str` is a pointer and a length, and a literal is never copied](0022-str-values.md) | Accepted |
| [0023. The language is named Baryon, its files end in `.byn`, and its tool is `baryon`](0023-language-name-baryon.md) | Accepted |
| [0024. Accumulated arrays compile to an `Accumulate` statement over lazily made private blocks](0024-compiling-accumulated-arrays.md) | Accepted |
| [0025. The VS Code extension lives in `editors/vscode/` and highlights with Python's grammar](0025-vscode-extension.md) | Accepted |
| [0026. A function passed as an argument is a compile-time argument, compiled once per function](0026-functions-as-arguments.md) | Accepted |
| [0027. When a default parameter value is evaluated](0027-default-parameter-evaluation.md) | Accepted |
| [0028. `random.Philox` gives the same numbers as NumPy's Philox, and the compiler builds it in](0028-philox-random-numbers.md) | Accepted |
| [0029. The runtime keeps the standard library; programs drop unused code, not `std`](0029-runtime-size.md) | Accepted |
| [0030. Integer overflow panics in debug builds and wraps in release builds](0030-integer-overflow.md) | Accepted |
| [0031. A trait is a `Protocol` class, and a type implements it by naming it as a base](0031-traits-are-protocols.md) | Accepted |
| [0032. A value that is one of several kinds is a union of structs, taken apart with `match`](0032-sum-types-are-unions-of-structs.md) | Accepted |
| [0033. `print` writes an array as NumPy does](0033-printing-arrays.md) | Accepted |
| [0034. `raise ... from` is rejected; a cause is kept in a field](0034-no-error-chaining.md) | Accepted |
| [0035. `dict` is Python's dict, with keys that compare exactly](0035-dict.md) | Accepted |
| [0036. Files and processes use Python's names, built natively in the runtime](0036-files-and-processes.md) | Accepted |
| [0038. Standard-library classes are Baryon source shipped inside the compiler](0038-standard-library-in-baryon.md) | Accepted |
