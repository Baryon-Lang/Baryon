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
4. **New ideas** go in the Ideas table with status `Idea`. An accepted idea becomes `Done`, linking
   the decision or pull request that accepted it, and its work moves into a phase as `Planned`
   items. A rejected idea becomes `Dropped` with a one-line reason.
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
| 0.1 | Language reference: grammar, type rules, reference rules, array semantics | 3 | — | In progress | Claude | [#9](https://github.com/csimpson1964/New-Programming-Language/pull/9), [#15](https://github.com/csimpson1964/New-Programming-Language/pull/15), [#17](https://github.com/csimpson1964/New-Programming-Language/pull/17), [#21](https://github.com/csimpson1964/New-Programming-Language/pull/21) |
| 0.2 | 20 example programs, iterated for readability | 2 | 0.1 | In progress | Claude | [#4](https://github.com/csimpson1964/New-Programming-Language/pull/4) |
| 0.3 | Physics kernel suite in Fortran and C++ with a benchmark harness: N-body, Lennard-Jones MD, 3D stencil, CG on sparse, RK4, Monte Carlo, matmul, FFT | 1 | — | Done | Claude | [#3](https://github.com/csimpson1964/New-Programming-Language/pull/3) |
| 0.3.1 | Record the Apple silicon baseline: run `FC=gfortran-14 cargo kernels --record` on an M-series Mac and commit `bench/baselines/macos-aarch64.json` | — | 0.3 | Planned | — | — |
| 0.3.2 | Fail a benchmark run when repeated runs of one kernel report different checksums | — | 0.3 | Done | Claude | [#5](https://github.com/csimpson1964/New-Programming-Language/pull/5) |
| 0.4 | Repository foundation: ground rules, roadmap, workspace, lints, architecture and link checks, CI | 1 | — | Done | Claude | [#1](https://github.com/csimpson1964/New-Programming-Language/pull/1) |
| 0.5 | Close gaps found reviewing 0.4: the roadmap check compares against the pull request's base, and the link check handles `~~~` fences and links from the repository root | — | 0.4 | Done | Claude | [#6](https://github.com/csimpson1964/New-Programming-Language/pull/6) |
| 0.6 | Check dependencies with cargo-deny in `cargo xtask ci`: licenses compatible with Apache-2.0 and MIT, RustSec advisories, banned and duplicate crates, allowed sources | — | 0.4 | Done | Claude | [#10](https://github.com/csimpson1964/New-Programming-Language/pull/10) |
| 0.7 | Bring the design plan up to date with decision records 0004 to 0016 and the merged front end, pointing to this roadmap for item status instead of repeating it | — | 0.4 | Done | Claude | [#18](https://github.com/csimpson1964/New-Programming-Language/pull/18) |

## Phase 1: MVP compiler

Gate: a 2,000-line Lennard-Jones and 3D heat simulation builds in both modes, runs within 5% of
the Fortran version, and edits in VS Code with live diagnostics; all four test suites are green.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | Lexer with indentation tokens and a token arena. Dropped: the existing Python parser lexes, indentation included | 1 | 0.1 | Dropped | — | [0004](docs/decisions/0004-python-syntax.md) |
| 1.2 | Front end: an existing Python 3.12 parser written in Rust behind an interface crate, lowering to a flat syntax tree, error messages written for this language, parser test corpus | 2 | 0.1 | Done | Claude | [0004](docs/decisions/0004-python-syntax.md), [#7](https://github.com/csimpson1964/New-Programming-Language/pull/7) |
| 1.2.1 | Reject the Python constructs the language does not support yet, with a diagnostic for each, as a pass over the syntax tree that follows the language reference's list | 1 | 0.1, 1.2 | Done | Claude | [#13](https://github.com/csimpson1964/New-Programming-Language/pull/13) |
| 1.3 | Tree-sitter grammar for editor highlighting. Dropped: editors already highlight the files as Python | 1 | 0.1 | Dropped | — | [0004](docs/decisions/0004-python-syntax.md) |
| 1.4 | Incremental query engine and the file and module database | 2 | 1.2 | Done | Claude | [0016](docs/decisions/0016-salsa-query-engine.md), [#14](https://github.com/csimpson1964/New-Programming-Language/pull/14) |
| 1.5 | Name resolution, modules and imports | 2 | 1.4 | Done | Claude | [#16](https://github.com/csimpson1964/New-Programming-Language/pull/16) |
| 1.6 | Type representation, interning, layouts, compile-time shapes, complex numbers | 2 | 1.5 | Done | Claude | [#19](https://github.com/csimpson1964/New-Programming-Language/pull/19) |
| 1.7 | Type checker: local inference, generics, static-dispatch traits, enums, match | 6 | 1.6 | Done | Claude | [#20](https://github.com/csimpson1964/New-Programming-Language/pull/20)–[#31](https://github.com/csimpson1964/New-Programming-Language/pull/31) |
| 1.7.1 | Warn when reordering a struct's fields would save padding, naming the order that does; fields keep declaration order otherwise (#19) | — | 1.6, 1.7 | Done | Claude | [#22](https://github.com/csimpson1964/New-Programming-Language/pull/22) |
| 1.7.2 | Declarations: signatures, struct fields, enum variants, aliases and `Final` constants computed by the compiler, with errors for annotations that mean no type | 1 | 1.6 | Done | Claude | [#20](https://github.com/csimpson1964/New-Programming-Language/pull/20) |
| 1.7.3 | Function bodies: local inference, literals, number mixing, operators, conditions, calls, fields and methods, assignments and returns | 2 | 1.7.2 | Done | Claude | [#24](https://github.com/csimpson1964/New-Programming-Language/pull/24) |
| 1.7.4 | Arrays: copy-type elements, indexing, slicing, whole-array arithmetic | 1 | 1.7.3 | Done | Claude | [#25](https://github.com/csimpson1964/New-Programming-Language/pull/25) |
| 1.7.5 | Generics: standard-trait bounds, type arguments inferred at calls | 1 | 1.7.3 | Done | Claude | [#26](https://github.com/csimpson1964/New-Programming-Language/pull/26) |
| 1.7.6 | Enums and exhaustive `match` | 0.5 | 1.7.3 | Done | Claude | [#28](https://github.com/csimpson1964/New-Programming-Language/pull/28) |
| 1.7.7 | `Raises`, `raise` and `try`: error types checked through calls | 0.5 | 1.7.3 | Done | Claude | [#29](https://github.com/csimpson1964/New-Programming-Language/pull/29) |
| 1.7.8 | `None` checks narrow an optional value to its type; using an unchecked optional is an error | 0.5 | 1.7.3 | Done | Claude | [#30](https://github.com/csimpson1964/New-Programming-Language/pull/30) |
| 1.7.9 | Definite assignment, naming the path that skips the assignment, and a missing `return` at the end of a function | 1 | 1.7.3 | Done | Claude | [#31](https://github.com/csimpson1964/New-Programming-Language/pull/31) |
| 1.7.10 | User-defined traits and their static dispatch, once D.9 settles their syntax; the standard traits are done in 1.7.5 | 1 | D.9 | Planned | — | — |
| 1.8 | Reference and exclusivity checker | 3 | 1.7 | In progress | Claude | — |
| 1.8.1 | `prange(n, accumulate=...)`: check that listed arrays are only added to, and suggest the argument in the disjoint-write error | — | 1.8 | Planned | — | [0013](docs/decisions/0013-parallel-accumulation.md) |
| 1.8.2 | Moves: a moved value is not read again; borrowed parameters, elements and slices cannot move | 1 | 1.7.9 | Done | Claude | [#32](https://github.com/csimpson1964/New-Programming-Language/pull/32) |
| 1.8.3 | Exclusivity: an `InOut` or `Owned` argument overlaps no other argument; read-only parameters are not changed | 1 | 1.8.2 | Done | Claude | [0006](docs/decisions/0006-parameter-passing.md), [#34](https://github.com/csimpson1964/New-Programming-Language/pull/34) |
| 1.8.4 | A loop over a collection does not change the collection in its body | 0.5 | 1.8.3 | Planned | — | — |
| 1.9 | Mid-level IR: lowering, drops, monomorphization | 5 | 1.7 | In progress | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#27](https://github.com/csimpson1964/New-Programming-Language/pull/27) |
| 1.9.1 | IR design and the `mir` interface crate: bodies as basic blocks over typed locals and places, explicit drops, bounds checks and error edges, a validator and a text form | 1 | 1.6 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#27](https://github.com/csimpson1964/New-Programming-Language/pull/27) |
| 1.9.2 | Typed bodies: a query that gives the type of every expression and the definition of every name in a function body, for lowering to read | 0.5 | 1.7 | Planned | — | — |
| 1.9.3 | Lowering: function bodies to `mir`, with control flow, calls, argument modes, bounds checks, `Raises` error edges and scope-exit drops | 1.5 | 1.9.1, 1.9.2 | Planned | — | [0018](docs/decisions/0018-mid-level-ir.md) |
| 1.9.4 | Drop elaboration: remove drops of moved values, guard partly moved ones with flags | 0.5 | 1.9.3 | Planned | — | [0018](docs/decisions/0018-mid-level-ir.md) |
| 1.9.5 | Monomorphization: instances at concrete types, collected from `main` and each `@test` | 1 | 1.9.3, 1.7.5 | Planned | — | [0018](docs/decisions/0018-mid-level-ir.md) |
| 1.9.6 | Whole-array expressions as one `Elementwise` rvalue, expanded to loop nests before a backend | 0.5 | 1.9.3, 1.7.4 | Planned | — | [0018](docs/decisions/0018-mid-level-ir.md) |
| 1.9.7 | `prange` bodies outlined behind a `ParallelFor` terminator, and `simd` loops marked and checked | 1 | 1.9.3, 1.8 | Planned | — | [0018](docs/decisions/0018-mid-level-ir.md), [0011](docs/decisions/0011-checked-simd.md), [0013](docs/decisions/0013-parallel-accumulation.md) |
| 1.10 | LLVM backend: no-alias metadata, debug info, Apple AArch64 ABI, Mach-O output | 6 | 1.9 | Planned | — | — |
| 1.11 | Driver: build, run, test, manifest, linking, compile-time reporting | 3 | 1.10 | Planned | — | — |
| 1.11.1 | Prototype `npl check <directory>`: load a package into the query database and print its module and syntax errors | — | 1.4 | Done | Claude | [#14](https://github.com/csimpson1964/New-Programming-Language/pull/14) |
| 1.11.2 | `npl check` also runs the syntax check (1.2.1) and name resolution (1.5) | — | 1.2.1, 1.5, 1.11.1 | Done | Claude | [#16](https://github.com/csimpson1964/New-Programming-Language/pull/16) |
| 1.12 | Runtime: allocator, panics, traps, thread pool | 2 | 1.10 | Planned | — | — |
| 1.12.1 | Accumulated arrays in `prange`: blocked private buffers per chunk, combined in the fixed reduction tree | — | 1.12 | Planned | — | [0013](docs/decisions/0013-parallel-accumulation.md) |
| 1.13 | Standard library: core and collections | 3 | 1.11 | Planned | — | — |
| 1.14 | Standard library: N-dimensional arrays, views, stencil, simd and parallel loops, random numbers, BLAS | 5 | 1.13 | Planned | — | — |
| 1.14.1 | Half-list Lennard-Jones with `accumulate=` in the examples, and a threaded half-list OpenMP baseline in the benchmark suite to compare against | — | 1.14 | Planned | — | [0013](docs/decisions/0013-parallel-accumulation.md) |
| 1.15 | Formatter built on the syntax tree. Dropped: existing Python formatters such as Ruff and Black format the files | 2 | 1.2 | Dropped | — | [0004](docs/decisions/0004-python-syntax.md) |
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
| D.4 | Default array layout: column-major (plan) or row-major (review). Decided: row-major | — | — | Done | Collins | [0008](docs/decisions/0008-row-major-arrays.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| D.5 | C++ interop: full, a C-shaped subset, or dropped | — | — | Planned | Collins | — |
| D.6 | Language name and file extension; blocks 1.17 | — | — | Planned | Collins | — |
| D.7 | Resolve the plan's FFI contradiction: no FFI before Phase 4, yet an `ffi` module in the MVP. Decided: no user-facing FFI before Phase 4 | — | — | Done | Collins | [0012](docs/decisions/0012-no-ffi-in-mvp.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| D.8 | License: Apache-2.0 alone, or dual Apache-2.0 and MIT | — | — | Planned | Collins | — |
| D.9 | Traits: how user code declares a trait and how a type implements it, structurally as Python's `Protocol` does or by naming it as Rust does | — | — | Planned | Collins | [reference](docs/reference/README.md#open-questions) |
| D.10 | Sum types: how to spell an enum whose variants carry data, in valid Python syntax | — | — | Planned | Collins | [reference](docs/reference/README.md#open-questions) |
| D.11 | Integer overflow in release builds: wrap, as the plan says, or panic, as in debug builds | — | — | Planned | Collins | [reference](docs/reference/README.md#open-questions) |
| D.12 | Parallel loops that add into shared arrays (Newton's third law, histograms, assembly). Decided: `prange(n, accumulate=forces)`, add-only, reproducible, blocked private buffers | — | I.6 | Done | Collins | [0013](docs/decisions/0013-parallel-accumulation.md), [#8](https://github.com/csimpson1964/New-Programming-Language/pull/8) |
| D.13 | Class inheritance: Python-style subclassing, or classes as plain structs with methods. Decided: no inheritance; shared behavior comes from traits | — | — | Done | Collins | [0015](docs/decisions/0015-no-class-inheritance.md), [#12](https://github.com/csimpson1964/New-Programming-Language/pull/12) |
| D.14 | Mid-level IR shape: static single assignment (plan) or locals and places; how `Raises`, monomorphization and whole-array expressions lower. Decided: locals and places, error edges on calls, instance query, `Elementwise` kept for fusion | — | — | Done | Collins | [0018](docs/decisions/0018-mid-level-ir.md), [#27](https://github.com/csimpson1964/New-Programming-Language/pull/27) |

## Ideas

Anyone may add an idea. The first ones come from the review of the design plan.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| I.1 | Make every source file valid Python grammar and parse it with an existing parser; replaces 1.1, 1.2, 1.3 and 1.15 | — | — | Done | Collins | [0004](docs/decisions/0004-python-syntax.md), [0005](docs/decisions/0005-error-handling.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| I.2 | Before the full spec, build a small compiler that emits C and benchmark it against gfortran | 4 | 0.3 | Idea | — | — |
| I.3 | Pass parameters as read-only borrows by default instead of moving them | — | — | Done | Collins | [0006](docs/decisions/0006-parameter-passing.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| I.4 | Keep bounds checks on in release builds, with an `unsafe` opt-out | — | — | Done | Collins | [0010](docs/decisions/0010-bounds-checks.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| I.5 | Export functions to Python in Phase 2; move DataFrame, file formats and plotting (3.13 to 3.16) to Phase 4 | 3 | 1.10 | Idea | — | — |
| I.6 | Array-valued reductions in parallel loops, needed for Lennard-Jones force accumulation. Accepted as `prange(n, accumulate=forces)` | — | 1.14 | Done | Collins | [0013](docs/decisions/0013-parallel-accumulation.md), [#8](https://github.com/csimpson1964/New-Programming-Language/pull/8) |
| I.7 | Python-style number mixing: `/` returns a float, literals adapt, integers promote to floats | — | — | Done | Collins | [0009](docs/decisions/0009-number-mixing.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2), [0014](docs/decisions/0014-number-conversions-are-calls.md) |
| I.8 | One array family: `Array[T, rank]` on the heap and a fixed-shape inline type | — | — | Done | Collins | [0007](docs/decisions/0007-array-types.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| I.9 | `simd` loops as a checked request that fails to compile when vectorization fails | — | — | Done | Collins | [0011](docs/decisions/0011-checked-simd.md), [#2](https://github.com/csimpson1964/New-Programming-Language/pull/2) |
| I.10 | Position the language against Julia, Mojo, Numba, Pythran, Codon and Chapel | — | — | Idea | — | — |
| I.11 | Run the example programs under CPython with a small shim library, as a correctness oracle for the compiler's output | 1 | I.1 | Idea | — | — |
| I.12 | Diagnostics measure the display width of wide characters (CJK, emoji) so markers line up under them | — | — | Idea | — | — |
| I.13 | Run the RustSec advisory check on a daily schedule and on pushes to `main` instead of on every pull request, so a newly published advisory does not turn unrelated pull requests red; the license, ban and source checks stay on every pull request and need no network | — | 0.6 | Idea | — | [#10](https://github.com/csimpson1964/New-Programming-Language/pull/10) |
| I.14 | A way to keep a struct's field order on purpose without the padding warning (1.7.1), for a layout fixed by a file format or C library. Accepted by Collins as `@layout("C")` | — | 1.7.1 | Done | Claude | [#22](https://github.com/csimpson1964/New-Programming-Language/pull/22), [0017](docs/decisions/0017-layout-decorator.md), [#23](https://github.com/csimpson1964/New-Programming-Language/pull/23) |
