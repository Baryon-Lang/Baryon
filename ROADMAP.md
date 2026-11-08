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
| 0.1 | Language reference: grammar, type rules, reference rules, array semantics | 3 | — | In progress | Claude | [#9](https://github.com/Baryon-Lang/Baryon/pull/9), [#15](https://github.com/Baryon-Lang/Baryon/pull/15), [#17](https://github.com/Baryon-Lang/Baryon/pull/17), [#21](https://github.com/Baryon-Lang/Baryon/pull/21) |
| 0.2 | 20 example programs, iterated for readability | 2 | 0.1 | In progress | Claude | [#4](https://github.com/Baryon-Lang/Baryon/pull/4) |
| 0.3 | Physics kernel suite in Fortran and C++ with a benchmark harness: N-body, Lennard-Jones MD, 3D stencil, CG on sparse, RK4, Monte Carlo, matmul, FFT | 1 | — | Done | Claude | [#3](https://github.com/Baryon-Lang/Baryon/pull/3) |
| 0.3.1 | Record the Apple silicon baseline: run `FC=gfortran-14 cargo kernels --record` on an M-series Mac and commit `bench/baselines/macos-aarch64.json` | — | 0.3 | Planned | — | — |
| 0.3.2 | Fail a benchmark run when repeated runs of one kernel report different checksums | — | 0.3 | Done | Claude | [#5](https://github.com/Baryon-Lang/Baryon/pull/5) |
| 0.4 | Repository foundation: ground rules, roadmap, workspace, lints, architecture and link checks, CI | 1 | — | Done | Claude | [#1](https://github.com/Baryon-Lang/Baryon/pull/1) |
| 0.5 | Close gaps found reviewing 0.4: the roadmap check compares against the pull request's base, and the link check handles `~~~` fences and links from the repository root | — | 0.4 | Done | Claude | [#6](https://github.com/Baryon-Lang/Baryon/pull/6) |
| 0.6 | Check dependencies with cargo-deny in `cargo xtask ci`: licenses compatible with Apache-2.0 and MIT, RustSec advisories, banned and duplicate crates, allowed sources | — | 0.4 | Done | Claude | [#10](https://github.com/Baryon-Lang/Baryon/pull/10) |
| 0.7 | Bring the design plan up to date with decision records 0004 to 0016 and the merged front end, pointing to this roadmap for item status instead of repeating it | — | 0.4 | Done | Claude | [#18](https://github.com/Baryon-Lang/Baryon/pull/18) |
| 0.8 | Audit documentation coverage: measure rustdoc coverage and examples per crate, compare the language reference with what the compiler implements, list the gaps as 0.8.1 to 0.8.6, and correct the learner's section of `docs/README.md`, which still said the compiler only checks syntax | — | 0.4 | Done | Claude | [#79](https://github.com/Baryon-Lang/Baryon/pull/79) |
| 0.8.1 | Say in the language reference which features `baryon build` compiles today, chapter by chapter, and update it in each lowering and backend pull request. Today the reference describes the whole of version 0 and none of the five example programs builds (`prange` in the backend, list displays, `Array.copy`, `Fixed.round`, unpacking assignment), so a reader cannot tell what will run | 0.25 | 0.1 | Done | Claude | [#80](https://github.com/Baryon-Lang/Baryon/pull/80) |
| 0.8.2 | Reference entry for default parameter values, which the checker accepts and lowering passes since #77, with a worked example. When a default that is not a literal or `Final` constant is evaluated is a design choice the entry must not settle on its own | 0.25 | 0.1 | Done | Claude | [#77](https://github.com/Baryon-Lang/Baryon/pull/77), [#81](https://github.com/Baryon-Lang/Baryon/pull/81) |
| 0.8.3 | Beginner's guide: install, write `main.byn`, `baryon run`, a first test with `baryon test`, and a short physics program that builds today, every step run in CI | 1 | 0.8.1 | Planned | — | — |
| 0.8.4 | Command reference for `baryon`: `check`, `build`, `run`, `test`, `doc` and `lsp`, with arguments, exit status and what each needs (`clang`, `main` in `main.byn`). Today it is spread over the README, three reference chapters and decision 0019 | 0.25 | 1.11.6 | Done | Claude | [#81](https://github.com/Baryon-Lang/Baryon/pull/81) |
| 0.8.5 | Tested rustdoc examples for the public items callers use directly in `mir` (1 of 15 items rustdoc counts have one), `syntax` (5 of 29), `source` (4 of 23), `diagnostics` (2 of 17) and `documentation` (2 of 15) | 0.5 | — | Planned | — | — |
| 0.8.6 | Bring the design plan up to date with decisions 0019 (language server) and 0022 (`str` values), which it does not cite | 0.1 | 0.7 | Done | Claude | [#81](https://github.com/Baryon-Lang/Baryon/pull/81) |

## Phase 1: MVP compiler

Gate: a 2,000-line Lennard-Jones and 3D heat simulation builds in both modes, runs within 5% of
the Fortran version, and edits in VS Code with live diagnostics; all four test suites are green.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | Lexer with indentation tokens and a token arena. Dropped: the existing Python parser lexes, indentation included | 1 | 0.1 | Dropped | — | [0004](docs/decisions/0004-python-syntax.md) |
| 1.2 | Front end: an existing Python 3.12 parser written in Rust behind an interface crate, lowering to a flat syntax tree, error messages written for this language, parser test corpus | 2 | 0.1 | Done | Claude | [0004](docs/decisions/0004-python-syntax.md), [#7](https://github.com/Baryon-Lang/Baryon/pull/7) |
| 1.2.1 | Reject the Python constructs the language does not support yet, with a diagnostic for each, as a pass over the syntax tree that follows the language reference's list | 1 | 0.1, 1.2 | Done | Claude | [#13](https://github.com/Baryon-Lang/Baryon/pull/13) |
| 1.3 | Tree-sitter grammar for editor highlighting. Dropped: editors already highlight the files as Python | 1 | 0.1 | Dropped | — | [0004](docs/decisions/0004-python-syntax.md) |
| 1.4 | Incremental query engine and the file and module database | 2 | 1.2 | Done | Claude | [0016](docs/decisions/0016-salsa-query-engine.md), [#14](https://github.com/Baryon-Lang/Baryon/pull/14) |
| 1.5 | Name resolution, modules and imports | 2 | 1.4 | Done | Claude | [#16](https://github.com/Baryon-Lang/Baryon/pull/16) |
| 1.6 | Type representation, interning, layouts, compile-time shapes, complex numbers | 2 | 1.5 | Done | Claude | [#19](https://github.com/Baryon-Lang/Baryon/pull/19) |
| 1.7 | Type checker: local inference, generics, static-dispatch traits, enums, match | 6 | 1.6 | Done | Claude | [#20](https://github.com/Baryon-Lang/Baryon/pull/20)–[#31](https://github.com/Baryon-Lang/Baryon/pull/31) |
| 1.7.1 | Warn when reordering a struct's fields would save padding, naming the order that does; fields keep declaration order otherwise (#19) | — | 1.6, 1.7 | Done | Claude | [#22](https://github.com/Baryon-Lang/Baryon/pull/22) |
| 1.7.2 | Declarations: signatures, struct fields, enum variants, aliases and `Final` constants computed by the compiler, with errors for annotations that mean no type | 1 | 1.6 | Done | Claude | [#20](https://github.com/Baryon-Lang/Baryon/pull/20) |
| 1.7.3 | Function bodies: local inference, literals, number mixing, operators, conditions, calls, fields and methods, assignments and returns | 2 | 1.7.2 | Done | Claude | [#24](https://github.com/Baryon-Lang/Baryon/pull/24) |
| 1.7.4 | Arrays: copy-type elements, indexing, slicing, whole-array arithmetic | 1 | 1.7.3 | Done | Claude | [#25](https://github.com/Baryon-Lang/Baryon/pull/25) |
| 1.7.5 | Generics: standard-trait bounds, type arguments inferred at calls | 1 | 1.7.3 | Done | Claude | [#26](https://github.com/Baryon-Lang/Baryon/pull/26) |
| 1.7.6 | Enums and exhaustive `match` | 0.5 | 1.7.3 | Done | Claude | [#28](https://github.com/Baryon-Lang/Baryon/pull/28) |
| 1.7.7 | `Raises`, `raise` and `try`: error types checked through calls | 0.5 | 1.7.3 | Done | Claude | [#29](https://github.com/Baryon-Lang/Baryon/pull/29) |
| 1.7.8 | `None` checks narrow an optional value to its type; using an unchecked optional is an error | 0.5 | 1.7.3 | Done | Claude | [#30](https://github.com/Baryon-Lang/Baryon/pull/30) |
| 1.7.9 | Definite assignment, naming the path that skips the assignment, and a missing `return` at the end of a function | 1 | 1.7.3 | Done | Claude | [#31](https://github.com/Baryon-Lang/Baryon/pull/31) |
| 1.7.10 | User-defined traits and their static dispatch, once D.9 settles their syntax; the standard traits are done in 1.7.5 | 1 | D.9 | Planned | — | — |
| 1.7.11 | Split the checker's calls and statements and the driver's unit tests into modules under 500 lines, which they passed with 1.9.2 | 0.1 | 1.9.2 | Done | Claude | [#47](https://github.com/Baryon-Lang/Baryon/pull/47) |
| 1.8 | Reference and exclusivity checker | 3 | 1.7 | Done | Claude | [#32](https://github.com/Baryon-Lang/Baryon/pull/32)–[#38](https://github.com/Baryon-Lang/Baryon/pull/38) |
| 1.8.1 | `prange(n, accumulate=...)`: check that listed arrays are only added to, and suggest the argument in the disjoint-write error | — | 1.8 | Done | Claude | [0013](docs/decisions/0013-parallel-accumulation.md), [#38](https://github.com/Baryon-Lang/Baryon/pull/38) |
| 1.8.2 | Moves: a moved value is not read again; borrowed parameters, elements and slices cannot move | 1 | 1.7.9 | Done | Claude | [#32](https://github.com/Baryon-Lang/Baryon/pull/32) |
| 1.8.3 | Exclusivity: an `InOut` or `Owned` argument overlaps no other argument; read-only parameters are not changed | 1 | 1.8.2 | Done | Claude | [0006](docs/decisions/0006-parameter-passing.md), [#34](https://github.com/Baryon-Lang/Baryon/pull/34) |
| 1.8.4 | A loop over a collection does not change the collection in its body | 0.5 | 1.8.3 | Done | Claude | [#36](https://github.com/Baryon-Lang/Baryon/pull/36) |
| 1.8.5 | `prange` bodies: shared arrays written only at the loop variable's index, shared scalars only as reductions, no early exit, no nested `prange` | 1 | 1.8.3 | Done | Claude | [#38](https://github.com/Baryon-Lang/Baryon/pull/38) |
| 1.9 | Mid-level IR: lowering, drops, monomorphization | 5 | 1.7 | In progress | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#27](https://github.com/Baryon-Lang/Baryon/pull/27) |
| 1.9.1 | IR design and the `mir` interface crate: bodies as basic blocks over typed locals and places, explicit drops, bounds checks and error edges, a validator and a text form | 1 | 1.6 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#27](https://github.com/Baryon-Lang/Baryon/pull/27) |
| 1.9.2 | Typed bodies: a query that gives the type of every expression and the definition of every name in a function body, for lowering to read | 0.5 | 1.7 | Done | Claude | [#39](https://github.com/Baryon-Lang/Baryon/pull/39), [#41](https://github.com/Baryon-Lang/Baryon/pull/41) |
| 1.9.3 | Lowering: function bodies to `mir`, with control flow, calls, argument modes, bounds checks, `Raises` error edges and scope-exit drops. First slice: numbers, `if`, `while`, `for` over `range`, calls, struct fields, `raise` and drops. Second: arrays, indexing with bounds checks, `a.unchecked`, `len`, `shape`, `size`, `zeros`/`ones`/`fill` and whole-array arithmetic. Third: error edges on calls that can fail, `raise`, `try` and `except`. Fourth: `match`. Then `min`, `max` and defaulted arguments. Slices follow | 1.5 | 1.9.1, 1.9.2 | In progress | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#45](https://github.com/Baryon-Lang/Baryon/pull/45), [#49](https://github.com/Baryon-Lang/Baryon/pull/49), [#51](https://github.com/Baryon-Lang/Baryon/pull/51), [#54](https://github.com/Baryon-Lang/Baryon/pull/54), [#75](https://github.com/Baryon-Lang/Baryon/pull/75), [#77](https://github.com/Baryon-Lang/Baryon/pull/77) |
| 1.9.4 | Drop elaboration: remove drops of moved values, guard partly moved ones with flags | 0.5 | 1.9.1 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md) |
| 1.9.5 | Monomorphization: instances at concrete types, collected from `main` and each `@test`. Substitution done; the driver collects instances from every non-generic function by following calls, and `baryon build` compiles each. `@test` roots come with the test runner | 1 | 1.9.3, 1.7.5 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#67](https://github.com/Baryon-Lang/Baryon/pull/67) |
| 1.9.6 | Whole-array expressions as one `Elementwise` rvalue, expanded to loop nests before a backend. Allocates results with `NewArray` (1.9.8) | 0.5 | 1.9.8 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#40](https://github.com/Baryon-Lang/Baryon/pull/40) |
| 1.9.9 | The dimension in `mir`'s index bounds check, so the runtime's index panic names it (asked by 1.12) | — | 1.9.1 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#46](https://github.com/Baryon-Lang/Baryon/pull/46) |
| 1.9.7 | `prange` bodies outlined behind a `ParallelFor` terminator, and `simd` loops marked and checked. Done: `ParallelFor` and outlined bodies in `mir`, carried through the passes; lowering of `prange` with shared reads, writes at the loop variable and `+`/`*` reductions; `Min` and `Max` reduction operators in `mir`, and `min`/`max` lowered, as reductions too; `simd(range(...))` typed and its body checked with the `prange` rules, LLVM's own verdict left to 3.7 (Collins, 2026-11-08). Next: the LLVM backend, `accumulate=` (1.12.1), and the `simd` mark in `mir` and lowering | 1 | 1.9.3, 1.8 | In progress | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [0011](docs/decisions/0011-checked-simd.md), [0013](docs/decisions/0013-parallel-accumulation.md), [#70](https://github.com/Baryon-Lang/Baryon/pull/70), [#72](https://github.com/Baryon-Lang/Baryon/pull/72), [#73](https://github.com/Baryon-Lang/Baryon/pull/73), [#75](https://github.com/Baryon-Lang/Baryon/pull/75), [#83](https://github.com/Baryon-Lang/Baryon/pull/83) |
| 1.9.8 | `NewArray` in `mir`: allocate an `Array` with run-time lengths, so array expansion can create its result | — | 1.9.1 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#37](https://github.com/Baryon-Lang/Baryon/pull/37) |
| 1.9.10 | Built-in callees in `mir`: `Callee::Builtin` for functions the language provides that have no item, starting with `print` | — | 1.9.1 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [#56](https://github.com/Baryon-Lang/Baryon/pull/56) |
| 1.9.11 | Report each construct lowering does not handle once: an argument of `print` that it cannot lower, such as `print(units.C)` or `print(V(1.0).x)`, was reported twice | — | 1.9.10 | Done | Claude | [#80](https://github.com/Baryon-Lang/Baryon/pull/80), [#84](https://github.com/Baryon-Lang/Baryon/pull/84) |
| 1.10 | LLVM backend: no-alias metadata, debug info, Apple AArch64 ABI, Mach-O output | 6 | 1.9 | In progress | Claude | [0020](docs/decisions/0020-llvm-backend-emits-text.md) |
| 1.10.1 | Scalar bodies to LLVM IR text: numbers, `bool`, enums, tuples and structs, control flow, direct calls, checks as traps, Python's `//` and `%` | 1 | 1.9.1 | Done | Claude | [0020](docs/decisions/0020-llvm-backend-emits-text.md), [#43](https://github.com/Baryon-Lang/Baryon/pull/43) |
| 1.10.2 | Failed checks call the runtime's panic functions with a message and a file, line and column constant, in place of traps | 0.5 | 1.10.1, 1.12 | Done | Claude | [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md), [#48](https://github.com/Baryon-Lang/Baryon/pull/48) |
| 1.10.3 | `Array` values: the header from `types::layout`, `NewArray` through the runtime allocator, `Length`, element addresses through the strides, index checks naming the dimension, and drops that free | 0.5 | 1.10.2, 1.9.9 | Done | Claude | [#48](https://github.com/Baryon-Lang/Baryon/pull/48) |
| 1.10.4 | `Fixed` arrays in the LLVM backend: nested LLVM arrays, built from row-major elements, indexed in place and passed by value; array programs build and run end to end | 0.25 | 1.10.3, 1.9.3 | Done | Claude | [0020](docs/decisions/0020-llvm-backend-emits-text.md), [#52](https://github.com/Baryon-Lang/Baryon/pull/52) |
| 1.10.5 | Errors in the LLVM backend: a function that can fail takes an error pointer and returns a failure flag, `raise` writes the tag and the error, calls branch to their error edge, and `ErrorIndex` and the `Error` projection read the error value | 0.5 | 1.10.4, 1.9.3 | Done | Claude | [0005](docs/decisions/0005-error-handling.md), [0018](docs/decisions/0018-mid-level-ir.md), [#53](https://github.com/Baryon-Lang/Baryon/pull/53) |
| 1.10.6 | `print` end to end: lowering passes literals as constants and borrows the rest, and the LLVM backend calls the runtime per argument with the separator and line end as byte constants. Numbers, `bool` and string literals | 0.25 | 1.10.1, 1.12.5, 1.9.10 | Done | Claude | [0004](docs/decisions/0004-python-syntax.md), [#57](https://github.com/Baryon-Lang/Baryon/pull/57) |
| 1.10.7 | A `main` that raises: the C `main` passes it an error value and, when it fails, prints `error: Type(field=value, …)` to standard error, a `str` field quoted as Python's `repr` writes it, and returns 1. The runtime's print functions take the stream | 0.25 | 1.10.5, 1.10.6 | Done | Claude | [#59](https://github.com/Baryon-Lang/Baryon/pull/59) |
| 1.10.8 | `str` values and f-strings in the LLVM backend: a pointer and a length with a static flag, literals never copied, f-strings through a runtime string builder. First slice: `str` values, literals, printing and drops. Second: format specifications checked at compile time and the IR's `Format` rvalue. Third: the runtime's string builder, formatting as Python's `format` does. Last: `copy()`, `+` and f-strings end to end, through the checker, lowering and the backend | 1 | 1.10.6 | Done | Claude | [0022](docs/decisions/0022-str-values.md), [#60](https://github.com/Baryon-Lang/Baryon/pull/60), [#66](https://github.com/Baryon-Lang/Baryon/pull/66), [#68](https://github.com/Baryon-Lang/Baryon/pull/68), [#69](https://github.com/Baryon-Lang/Baryon/pull/69), [#74](https://github.com/Baryon-Lang/Baryon/pull/74) |
| 1.10.9 | Debug line tables: a `DISubprogram` per function and a `!dbg` location per instruction, so `gdb`, `lldb`, profilers and later backtraces (1.12.3) show `.byn` lines. Variables and types come later | 0.25 | 1.10.1 | Done | Claude | [#61](https://github.com/Baryon-Lang/Baryon/pull/61) |
| 1.10.10 | Optionals in the LLVM backend: an optional `T` as `{ T, i1 }`, absent as all zeros, `is None` reading the flag, narrowed reads of the value, and drops that free an owned value only when present | 0.25 | 1.10.3 | Done | Claude | [#62](https://github.com/Baryon-Lang/Baryon/pull/62) |
| 1.10.11 | `prange` loops in the LLVM backend: the outlined body as an internal function, a chunk function per loop that gives each reduction a private copy at its operator's identity, `baryon_rt_parallel_for` over a context of shared places, and the partials combined in a fixed pairwise tree with overflow checks | 0.5 | 1.9.7, 1.12.2 | Done | Claude | [0018](docs/decisions/0018-mid-level-ir.md), [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md), [#76](https://github.com/Baryon-Lang/Baryon/pull/76) |
| 1.11 | Driver: build, run, test, manifest, linking, compile-time reporting | 3 | 1.10 | Planned | — | — |
| 1.11.1 | Prototype `baryon check <directory>`: load a package into the query database and print its module and syntax errors | — | 1.4 | Done | Claude | [#14](https://github.com/Baryon-Lang/Baryon/pull/14) |
| 1.11.2 | `baryon check` also runs the syntax check (1.2.1) and name resolution (1.5) | — | 1.2.1, 1.5, 1.11.1 | Done | Claude | [#16](https://github.com/Baryon-Lang/Baryon/pull/16) |
| 1.11.3 | Prototype `baryon build <directory> <output>`: compile every non-generic function through lowering, the passes and the LLVM backend, add a C `main` that runs the package's `main`, and link with `clang` and the runtime archive | 0.5 | 1.10.3, 1.12.2 | Done | Claude | [0020](docs/decisions/0020-llvm-backend-emits-text.md), [#50](https://github.com/Baryon-Lang/Baryon/pull/50) |
| 1.11.4 | `baryon run <directory>`: build into a scratch directory, run with the terminal's input and output, and exit with the program's status | — | 1.11.3 | Done | Claude | [#58](https://github.com/Baryon-Lang/Baryon/pull/58) |
| 1.11.5 | Fix a flaky `baryon build` test: two tests shared one package directory and deleted each other's files; a reused name now fails every run | — | 1.11.4 | Done | Claude | [#65](https://github.com/Baryon-Lang/Baryon/pull/65) |
| 1.11.6 | Prototype `baryon test <directory>`: build every top-level `@test` function into one program whose C `main` runs the test its argument names, run each in its own process, and report `ok` or `FAILED` per test with a failed test's output | — | 1.11.4 | Done | Claude | [#78](https://github.com/Baryon-Lang/Baryon/pull/78) |
| 1.12 | Runtime: allocator, panics, traps, thread pool | 2 | 1.10 | In progress | Claude | [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md), [#44](https://github.com/Baryon-Lang/Baryon/pull/44) |
| 1.12.1 | Accumulated arrays in `prange`: blocked private buffers per chunk, combined in the fixed reduction tree | — | 1.12 | Planned | — | [0013](docs/decisions/0013-parallel-accumulation.md) |
| 1.12.2 | The `runtime` crate and its `unsafe` policy: allocator shim, array allocation for `NewArray`, panic entry points with locations, and the `prange` thread pool over fixed chunks | 1 | — | Done | Claude | [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md), [#44](https://github.com/Baryon-Lang/Baryon/pull/44) |
| 1.12.3 | Backtraces in panic messages in debug builds: the `.byn` frames from the line tables, most recent last, inlined calls included | 0.25 | 1.10.9, 1.12.2 | Done | Claude | [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md), [#63](https://github.com/Baryon-Lang/Baryon/pull/63) |
| 1.12.4 | Shrink a statically linked program that prints a number under 100 KB: measure, then choose between the standard library built for size and a `no_std` runtime | — | 1.11, 1.12.2 | Planned | — | [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md) |
| 1.12.5 | `print` in the runtime: one entry point per kind of value, writing integers, floats, `bool` and text to standard output as Python's `print` and `repr` do | 0.25 | 1.12.2 | Done | Claude | [0004](docs/decisions/0004-python-syntax.md), [#55](https://github.com/Baryon-Lang/Baryon/pull/55) |
| 1.12.6 | Fix backtraces on macOS: give each function an unwind table, which LLVM leaves out of a function it infers cannot unwind, and collect the line tables with `dsymutil` into `program.dSYM`, which `clang` does not do for an LLVM module | — | 1.12.3 | Done | Claude | [#71](https://github.com/Baryon-Lang/Baryon/pull/71) |
| 1.13 | Standard library: core and collections | 3 | 1.11 | Planned | — | — |
| 1.14 | Standard library: N-dimensional arrays, views, stencil, simd and parallel loops, random numbers, BLAS | 5 | 1.13 | Planned | — | — |
| 1.14.1 | Half-list Lennard-Jones with `accumulate=` in the examples, and a threaded half-list OpenMP baseline in the benchmark suite to compare against | — | 1.14 | Planned | — | [0013](docs/decisions/0013-parallel-accumulation.md) |
| 1.15 | Formatter built on the syntax tree. Dropped: existing Python formatters such as Ruff and Black format the files | 2 | 1.2 | Dropped | — | [0004](docs/decisions/0004-python-syntax.md) |
| 1.16 | Language server: diagnostics, go to definition, hover, completion, fixes | 4 | 1.8 | Planned | — | — |
| 1.16.1 | `baryon lsp`: a language server over standard input and output that publishes `baryon check`'s diagnostics as the user types, from the same incremental session | 0.5 | 1.11.2 | Done | Claude | [0019](docs/decisions/0019-language-server-protocol.md), [#35](https://github.com/Baryon-Lang/Baryon/pull/35) |
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
| 3.8 | Documentation generator: `baryon doc` writes one Markdown page per module from its documentation strings and public signatures, and an index | 1 | 1.2 | Done | Claude | [#42](https://github.com/Baryon-Lang/Baryon/pull/42) |
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
| D.4 | Default array layout: column-major (plan) or row-major (review). Decided: row-major | — | — | Done | Collins | [0008](docs/decisions/0008-row-major-arrays.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| D.5 | C++ interop: full, a C-shaped subset, or dropped | — | — | Planned | Collins | — |
| D.6 | Language name and file extension; blocks 1.17. Decided: Baryon, `.byn`, tool `baryon` | — | — | Done | Collins | [0023](docs/decisions/0023-language-name-baryon.md), [#64](https://github.com/Baryon-Lang/Baryon/pull/64) |
| D.7 | Resolve the plan's FFI contradiction: no FFI before Phase 4, yet an `ffi` module in the MVP. Decided: no user-facing FFI before Phase 4 | — | — | Done | Collins | [0012](docs/decisions/0012-no-ffi-in-mvp.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| D.8 | License: Apache-2.0 alone, or dual Apache-2.0 and MIT | — | — | Planned | Collins | — |
| D.9 | Traits: how user code declares a trait and how a type implements it, structurally as Python's `Protocol` does or by naming it as Rust does | — | — | Planned | Collins | [reference](docs/reference/README.md#open-questions) |
| D.10 | Sum types: how to spell an enum whose variants carry data, in valid Python syntax | — | — | Planned | Collins | [reference](docs/reference/README.md#open-questions) |
| D.11 | Integer overflow in release builds: wrap, as the plan says, or panic, as in debug builds | — | — | Planned | Collins | [reference](docs/reference/README.md#open-questions) |
| D.12 | Parallel loops that add into shared arrays (Newton's third law, histograms, assembly). Decided: `prange(n, accumulate=forces)`, add-only, reproducible, blocked private buffers | — | I.6 | Done | Collins | [0013](docs/decisions/0013-parallel-accumulation.md), [#8](https://github.com/Baryon-Lang/Baryon/pull/8) |
| D.13 | Class inheritance: Python-style subclassing, or classes as plain structs with methods. Decided: no inheritance; shared behavior comes from traits | — | — | Done | Collins | [0015](docs/decisions/0015-no-class-inheritance.md), [#12](https://github.com/Baryon-Lang/Baryon/pull/12) |
| D.14 | Mid-level IR shape: static single assignment (plan) or locals and places; how `Raises`, monomorphization and whole-array expressions lower. Decided: locals and places, error edges on calls, instance query, `Elementwise` kept for fusion | — | — | Done | Collins | [0018](docs/decisions/0018-mid-level-ir.md), [#27](https://github.com/Baryon-Lang/Baryon/pull/27) |
| D.15 | When a default parameter value that is not a literal or a `Final` constant is evaluated: at each call that leaves the argument out, once and shared as in Python, or never because only compile-time defaults are allowed | — | 0.8.2 | Planned | Collins | [reference](docs/reference/README.md#open-questions) |

## Ideas

Anyone may add an idea. The first ones come from the review of the design plan.

| ID | Item | Weeks | Depends on | Status | Owner | Link |
| --- | --- | --- | --- | --- | --- | --- |
| I.1 | Make every source file valid Python grammar and parse it with an existing parser; replaces 1.1, 1.2, 1.3 and 1.15 | — | — | Done | Collins | [0004](docs/decisions/0004-python-syntax.md), [0005](docs/decisions/0005-error-handling.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| I.2 | Before the full spec, build a small compiler that emits C and benchmark it against gfortran | 4 | 0.3 | Idea | — | — |
| I.3 | Pass parameters as read-only borrows by default instead of moving them | — | — | Done | Collins | [0006](docs/decisions/0006-parameter-passing.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| I.4 | Keep bounds checks on in release builds, with an `unsafe` opt-out | — | — | Done | Collins | [0010](docs/decisions/0010-bounds-checks.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| I.5 | Export functions to Python in Phase 2; move DataFrame, file formats and plotting (3.13 to 3.16) to Phase 4 | 3 | 1.10 | Idea | — | — |
| I.6 | Array-valued reductions in parallel loops, needed for Lennard-Jones force accumulation. Accepted as `prange(n, accumulate=forces)` | — | 1.14 | Done | Collins | [0013](docs/decisions/0013-parallel-accumulation.md), [#8](https://github.com/Baryon-Lang/Baryon/pull/8) |
| I.7 | Python-style number mixing: `/` returns a float, literals adapt, integers promote to floats | — | — | Done | Collins | [0009](docs/decisions/0009-number-mixing.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2), [0014](docs/decisions/0014-number-conversions-are-calls.md) |
| I.8 | One array family: `Array[T, rank]` on the heap and a fixed-shape inline type | — | — | Done | Collins | [0007](docs/decisions/0007-array-types.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| I.9 | `simd` loops as a checked request that fails to compile when vectorization fails | — | — | Done | Collins | [0011](docs/decisions/0011-checked-simd.md), [#2](https://github.com/Baryon-Lang/Baryon/pull/2) |
| I.10 | Position the language against Julia, Mojo, Numba, Pythran, Codon and Chapel | — | — | Idea | — | — |
| I.11 | Run the example programs under CPython with a small shim library, as a correctness oracle for the compiler's output | 1 | I.1 | Idea | — | — |
| I.12 | Diagnostics measure the display width of wide characters (CJK, emoji) so markers line up under them | — | — | Idea | — | — |
| I.13 | Run the RustSec advisory check on a daily schedule and on pushes to `main` instead of on every pull request, so a newly published advisory does not turn unrelated pull requests red; the license, ban and source checks stay on every pull request and need no network | — | 0.6 | Idea | — | [#10](https://github.com/Baryon-Lang/Baryon/pull/10) |
| I.14 | A way to keep a struct's field order on purpose without the padding warning (1.7.1), for a layout fixed by a file format or C library. Accepted by Collins as `@layout("C")` | — | 1.7.1 | Done | Claude | [#22](https://github.com/Baryon-Lang/Baryon/pull/22), [0017](docs/decisions/0017-layout-decorator.md), [#23](https://github.com/Baryon-Lang/Baryon/pull/23) |
| I.15 | Run the code examples in documentation strings as tests, as Rust's doc tests do, so documented examples cannot drift from the code; needs 1.10 to run programs | — | 3.8 | Idea | — | — |
| I.16 | Link names in documentation to their definitions across pages, using the resolver, and publish HTML with search | — | 3.8 | Idea | — | — |
| I.17 | Let a user choose how many threads `prange` uses, such as with an environment variable; today the pool follows the machine's cores. User-visible, so it needs Collins | — | 1.12.2 | Idea | — | [0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md) |
| I.18 | Name the values in overflow and conversion panics, as the reference's panic table asks: `u8(300)` reports 300. Needs runtime entry points that take the operands, or formatting in compiled code | — | 1.10.2 | Idea | — | — |
| I.19 | Accept integer literals above `2^63 - 1` where the expected type is `u64`: `x: u64 = 18446744073709551615` is rejected today as not fitting in `i64` | — | 1.7 | Idea | — | — |
| I.20 | Reject a default on an `InOut` parameter in the checker: a call that leaves the argument out would lose what the callee writes into it. Lowering reports it as not lowered today. User-visible, so it needs Collins | — | 1.7 | Idea | — | [#77](https://github.com/Baryon-Lang/Baryon/pull/77) |
| I.21 | A `cargo xtask` check that fails on leftover merge-conflict markers, `<<<<<<<`, `=======` or `>>>>>>>` at the start of a line, in tracked files: a merge into #76 left them in `README.md` and every other check passed | — | — | Done | Claude | [#76](https://github.com/Baryon-Lang/Baryon/pull/76), [#86](https://github.com/Baryon-Lang/Baryon/pull/86) |
