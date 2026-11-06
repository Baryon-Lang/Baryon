# Roadmap

This file is the single source of truth for what is planned, in progress and done. Every
contributor, human or AI agent, updates it when they start work and when they finish. New ideas
are welcome at any time: add them to [Ideas](#ideas).

The phases and work packages come from the [design plan](docs/design/language-design-plan.md),
which explains the reasoning behind each one.

## How to use it

1. **Before you start**, find the item you will work on, or add one. Set its status to
   `In progress` and put your name in Owner. Open a draft pull request early and put its link in
   Link, so others can see the claim.
2. **When you finish**, set the item to `Done` in the same pull request that completes it.
3. **If you are stuck**, set the item to `Blocked` and add the reason to its description.
4. **New ideas** go in the Ideas table with status `Idea`. An accepted idea moves into a phase as
   `Planned`; a rejected one becomes `Dropped` with a one-line reason.
5. **Never delete a row.** Finished and dropped items stay as the project's history.

`cargo xtask check-roadmap` keeps this file valid, and CI fails any pull request that changes
something without updating it. In every table, an empty cell is written `—`.

| Status | Meaning |
| --- | --- |
| `Idea` | Proposed; not yet accepted into a phase |
| `Planned` | Accepted and waiting for an owner |
| `In progress` | Someone is working on it; Owner says who |
| `Blocked` | Started, but waiting on something named in the description |
| `Done` | Finished and merged; Link points to the pull request |
| `Dropped` | Decided against; the description says why |

## Phase 0: specification and foundation

Gate: the 20 example programs are frozen and the reference rules are written as a decision
procedure on paper.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 0.1 | Language reference: grammar, type rules, reference rules, array semantics | 3 | — | In progress | Claude | [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| 0.2 | 20 example programs, iterated for readability | 2 | 0.1 | In progress | Claude | [#4](https://github.com/csimpson1964/New-Programming-Language/pull/4) |
| 0.3 | Physics kernel suite in Fortran and C++ with a benchmark harness: N-body, Lennard-Jones MD, 3D stencil, CG on sparse, RK4, Monte Carlo, matmul, FFT | 1 | — | Planned | — | — |
| 0.4 | Repository foundation: ground rules, roadmap, workspace, lints, architecture and link checks, CI | 1 | — | Done | Claude | [#1](https://github.com/csimpson1964/New-Programming-Language/pull/1) |

## Phase 1: MVP compiler

Gate: a 2,000-line Lennard-Jones and 3D heat simulation builds in both modes, runs within 5% of
the Fortran version, and edits in VS Code with live diagnostics; all four test suites are green.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | Lexer with indentation tokens and a token arena | 1 | 0.1 | Planned | — | — |
| 1.2 | Parser, flat syntax tree, error recovery, parser test corpus | 3 | 1.1 | Planned | — | — |
| 1.3 | Tree-sitter grammar for editor highlighting | 1 | 0.1 | Planned | — | — |
| 1.4 | Incremental query engine and the file and module database | 2 | 1.2 | Planned | — | — |
| 1.5 | Name resolution, modules and imports | 2 | 1.4 | Planned | — | — |
| 1.6 | Type representation, interning, layouts, compile-time shapes, complex numbers | 2 | 1.5 | Planned | — | — |
| 1.7 | Type checker: local inference, generics, static-dispatch traits, enums, match | 6 | 1.6 | Planned | — | — |
| 1.8 | Reference and exclusivity checker | 3 | 1.7 | Planned | — | — |
| 1.9 | Mid-level IR: lowering, drops, monomorphization | 5 | 1.7 | Planned | — | — |
| 1.10 | LLVM backend: no-alias metadata, debug info, Apple AArch64 ABI, Mach-O output | 6 | 1.9 | Planned | — | — |
| 1.11 | Driver: build, run, test, manifest, linking, compile-time reporting | 3 | 1.10 | Planned | — | — |
| 1.12 | Runtime: allocator, panics, traps, thread pool | 2 | 1.10 | Planned | — | — |
| 1.13 | Standard library: core and collections | 3 | 1.11 | Planned | — | — |
| 1.14 | Standard library: N-dimensional arrays, views, stencil, simd and parallel loops, random numbers, BLAS | 5 | 1.13 | Planned | — | — |
| 1.15 | Formatter built on the syntax tree | 2 | 1.2 | Planned | — | — |
| 1.16 | Language server: diagnostics, go to definition, hover, completion, fixes | 4 | 1.8 | Planned | — | — |
| 1.17 | VS Code extension | 1 | 1.16 | Planned | — | — |
| 1.18 | Execution corpus, benchmark baselines and fuzzing in CI | 2 | 1.11 | Planned | — | — |

## Phase 2: compile speed and the edit loop

Gate: every number in the design plan's compile-speed table is met on the reference project.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 2.1 | Cranelift debug backend | 4 | 1.9 | Planned | — | — |
| 2.2 | Incremental code generation and object patching | 3 | 2.1 | Planned | — | — |
| 2.3 | Compile-speed profiling and tuning to the targets | 3 | 2.2 | Planned | — | — |
| 2.4 | Language server: rename, inlay hints, run tests from the editor, debugger hookup | 2 | 1.16 | Planned | — | — |

## Phase 3: optimization and data science

Gate: within 5% of gfortran on every kernel, the 30-line load, group, fit and plot script runs in
the REPL, and three external users have written something real.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 3.1 | Array expression fusion | 4 | 1.14 | Planned | — | — |
| 3.2 | Bounds-check hoisting and shape inference | 3 | 3.1 | Planned | — | — |
| 3.3 | Vectorizer audit against gfortran on each kernel | 3 | 3.1 | Planned | — | — |
| 3.4 | Thin LTO, profile-guided optimization, instantiation deduplication | 3 | 1.10 | Planned | — | — |
| 3.5 | Trait objects and dynamic dispatch | 4 | 1.7 | Planned | — | — |
| 3.6 | First-class references or regions: design and prototype | 4 | 1.8 | Planned | — | — |
| 3.7 | Optimization remarks in the editor | 2 | 3.1 | Planned | — | — |
| 3.8 | Documentation generator | 1 | 1.2 | Planned | — | — |
| 3.9 | Broadcasting and axis reductions | 3 | 3.1 | Planned | — | — |
| 3.10 | Linear algebra and FFT over LAPACK and FFTW or Accelerate | 2 | 1.14 | Planned | — | — |
| 3.11 | REPL on the Cranelift backend with persistent values | 4 | 2.1 | Planned | — | — |
| 3.12 | Jupyter kernel | 1 | 3.11 | Planned | — | — |
| 3.13 | Arrow memory layout, DataFrame, group by, join | 5 | 3.9 | Planned | — | — |
| 3.14 | File formats: CSV, Parquet, HDF5, NetCDF, FITS | 3 | 3.13 | Planned | — | — |
| 3.15 | Plotting | 4 | 3.9 | Planned | — | — |
| 3.16 | Statistics | 2 | 3.9 | Planned | — | — |

## Phase 4: ecosystem and interop

Ongoing. Automatic differentiation, distributed arrays, GPU offload, more targets, a package
registry and a standalone IDE are candidates, each added here when it is planned.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 3.17 | Python interop with zero-copy arrays | 3 | 1.10 | Planned | — | — |
| 4.1 | C++ interop: header import, templates by instantiation, exceptions at the boundary | 12+ | 3.5 | Planned | — | — |

## Decisions needed

Open questions from the design plan and its review. A decision is `Done` once it is recorded in
[`docs/decisions/`](docs/decisions/README.md).

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| D.1 | Team size and hours per week; sets every estimate | — | — | Planned | Collins | — |
| D.2 | Target workload for the Phase 3 gate (review suggests Lennard-Jones MD) | — | — | Planned | Collins | — |
| D.3 | Mark `inout` at the call site as well as the signature | — | 0.2 | Planned | — | [0006](docs/decisions/0006-parameter-passing.md) |
| D.4 | Default array layout: column-major (plan) or row-major (review) | — | — | Planned | Collins | [0008](docs/decisions/0008-row-major-arrays.md) |
| D.5 | C++ interop: full, a C-shaped subset, or dropped | — | — | Planned | Collins | — |
| D.6 | Language name and file extension; blocks 1.3 and 1.17 | — | — | Planned | Collins | — |
| D.7 | Resolve the plan's FFI contradiction: no FFI before Phase 4, yet an `ffi` module in the MVP | — | — | Planned | Collins | [0012](docs/decisions/0012-no-ffi-in-mvp.md) |
| D.8 | License: Apache-2.0 alone, or dual Apache-2.0 and MIT | — | — | Planned | Collins | — |

## Ideas

Anyone may add an idea. The first ones come from the review of the design plan.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| I.1 | Make every source file valid Python grammar and parse it with an existing parser; replaces 1.1, 1.2, 1.3 and 1.15 | — | — | Idea | — | [0004](docs/decisions/0004-python-syntax.md), [0005](docs/decisions/0005-error-handling.md) |
| I.2 | Before the full spec, build a small compiler that emits C and benchmark it against gfortran | 4 | 0.3 | Idea | — | — |
| I.3 | Pass parameters as read-only borrows by default instead of moving them | — | — | Idea | — | [0006](docs/decisions/0006-parameter-passing.md) |
| I.4 | Keep bounds checks on in release builds, with an `unsafe` opt-out | — | — | Idea | — | [0010](docs/decisions/0010-bounds-checks.md) |
| I.5 | Export functions to Python in Phase 2; move DataFrame, file formats and plotting (3.13 to 3.16) to Phase 4 | 3 | 1.10 | Idea | — | — |
| I.6 | Array-valued reductions in parallel loops, needed for Lennard-Jones force accumulation | — | 1.14 | Idea | — | — |
| I.7 | Python-style number mixing: `/` returns a float, literals adapt, integers promote to floats | — | — | Idea | — | [0009](docs/decisions/0009-number-mixing.md) |
| I.8 | One array family: `Array[T, rank]` on the heap and a fixed-shape inline type | — | — | Idea | — | [0007](docs/decisions/0007-array-types.md) |
| I.9 | `simd` loops as a checked request that fails to compile when vectorization fails | — | — | Idea | — | [0011](docs/decisions/0011-checked-simd.md) |
| I.10 | Position the language against Julia, Mojo, Numba, Pythran, Codon and Chapel | — | — | Idea | — | — |
| I.11 | Run the example programs under CPython with a small shim library, as a correctness oracle for the compiler's output | 1 | I.1 | Idea | — | — |
