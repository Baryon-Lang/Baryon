# 0021. The runtime is one Rust crate of its own kind, the only one outside a backend allowed `unsafe`

Status: Accepted
Date: 2026-11-07
Roadmap: 1.12

## Context

Roadmap item 1.12 is the runtime: the code linked into every compiled program, beside the
program's own. The design plan asks for "an allocator shim over the system `malloc`, a panic
handler with backtraces in debug builds, bounds-check and overflow-trap intrinsics, and a thread
pool for `prange`, started on first use", in a runtime "as small as C's".

Three things about it are hard to change once the backend (1.10) and the driver (1.11) are built
around it, and they cross crates, so they get a record (`CONTRIBUTING.md` rule 7). None of them
changes what a user writes.

1. **`unsafe` code.** The workspace forbids `unsafe` in every crate, and `CONTRIBUTING.md` rule 4
   says the runtime is a crate that gets an exception through a decision record. It cannot avoid
   one: functions called from machine code take raw pointers, exporting a function under a fixed
   C name is itself an `unsafe` attribute in Rust 2024, the system allocator is an `unsafe`
   interface, and a thread pool that runs a body on threads that outlive one call must assert
   that the body may be shared. The question is how to allow it without weakening the rule for
   every other crate.
2. **Where the runtime sits in the crate architecture.** It is not part of the compiler: it ends
   up in the user's program, not in `npl`. None of the five kinds in
   [`docs/architecture.md`](../architecture.md) fits. A `foundation` crate may be used by every
   compiler crate, so the compiler could come to depend on the runtime, and the runtime could
   come to depend on `source` or `diagnostics`, putting compiler code into every program.
3. **How compiled code reaches it, and in what language it is written.** Options considered:
   - **C**, compiled by the driver with the system C compiler. Small, and the natural language
     for a `malloc` shim, but it brings a second language, a second toolchain on every machine
     that builds the compiler, and no thread pool or tests without writing them from nothing.
   - **LLVM IR emitted by the backend** for every program. No library to link, but the thread
     pool and the panic printer would be written as code that writes code, untested by
     `cargo test`, and duplicated again for the Cranelift backend (2.1).
   - **A Rust crate built as a static library**, exporting C functions. One language and one
     toolchain, `cargo test` and doc tests for every function, and the standard library's
     threads, locks and system allocator.

## Decision

### A Rust static library, called through C functions

The runtime is the crate `runtime`, built as a static library (`libruntime.a`) as well as a Rust
library for its own tests. The driver links the archive into each program (1.11). Compiled code
calls it only through functions exported with C names and the C calling convention, every name
starting `npl_rt_`; the backend declares each with the signature the crate documents and calls
it by name. The first set:

| Function | Job |
| --- | --- |
| `npl_rt_allocate(bytes, align, location)`, `npl_rt_deallocate(pointer, bytes, align)` | Heap memory from the system allocator. Zero bytes allocate nothing |
| `npl_rt_allocate_array(count, element_size, align, location)` | An IR `NewArray`: checks the length is not negative and the size fits, then allocates |
| `npl_rt_panic(message, length, location)` | A panic whose message compiled code wrote: overflow, division by zero, a failed `assert` |
| `npl_rt_panic_index(index, dimension, length, location)` | An `IndexInBounds` check that failed (decision 0010) |
| `npl_rt_panic_length_mismatch(dimension, expected, found, location)` | A `SameLength` check that failed |
| `npl_rt_chunk_count(iterations)`, `npl_rt_parallel_for(iterations, body, context)` | A `prange` loop on the thread pool, in fixed chunks |

A `location` is a pointer to a constant the backend emits per check: file name, line and column,
in C layout. The success path of a check carries no location work.

The exported functions are an internal interface between two parts of one compiler release, not
a stable ABI. They change with the backend in the same pull request when they need to.

### Panics print, then exit with status 101

A panic prints `panic: <message>` and `  --> <file>:<line>:<column>`, the form compiler
diagnostics use, to standard error, and exits with status 101. Status 1 is already taken by an
error that reaches the end of `main` (the reference, "Errors"), and 101 is what Rust programs use
for a panic, so scripts can tell a bug from a reported failure. The runtime never unwinds:
panics cannot be caught (decision 0005), so there is nothing to unwind to. When threads of a
parallel loop panic at once, the first prints and exits; the rest stop where they are.

Backtraces in debug builds are a later slice (1.12.3). They need the debug information the
backend does not emit yet.

### `prange` chunks depend only on the iteration count

A loop of `n` iterations is split into `min(n, 1024)` contiguous chunks, as evenly as integers
allow. The thread pool runs them in any order, but their boundaries never depend on the number
of threads, which is what makes a parallel floating-point reduction give the same bits on every
machine (the reference, "Reproducible reductions"). Compiled code keeps one partial result per
chunk and combines them in a fixed tree after the loop. The pool has one worker per core but one,
started by the first loop with more than one chunk; the calling thread works too, and runs every
chunk itself when there are no workers. 1024 chunks is eight per thread on a 128-core machine,
for load balance, at one atomic addition per chunk. The number changes the last bits of
reductions, so it changes only with a benchmark that justifies it (roadmap 0.3).

### A new crate kind, `runtime`

`docs/architecture.md` gains a sixth kind. A `runtime` crate may depend only on other `runtime`
crates; only `application` crates (the driver, to find the archive) and `tooling` may depend on
one. `cargo xtask check-architecture` enforces both directions. The backend, an implementation,
cannot depend on the runtime: it knows the functions by their documented names, and a test in
an application crate will check that the two agree once the backend exists.

### `unsafe` is denied, not forbidden, in the runtime, and allowed one item at a time

The runtime does not inherit the workspace lint table, because `forbid` cannot be relaxed by an
attribute. It copies the table with `unsafe_code = "deny"` instead, plus three lints of its own:

- each item that uses `unsafe` allows it with `#[allow(unsafe_code, reason = "...")]`, so a
  reader sees the reason where the code is, and `clippy::allow_attributes_without_reason`
  rejects an allow without one;
- `clippy::undocumented_unsafe_blocks`: every `unsafe` block and `unsafe impl` carries a
  `// SAFETY:` comment saying why it is sound;
- `clippy::multiple_unsafe_ops_per_block`: each block does one unsafe thing, so each comment
  justifies one thing.

The logic lives in safe functions: chunk geometry, layout arithmetic and message formatting have
no `unsafe` at all. The exported functions are thin wrappers that turn raw pointers into
references under the promise in their `# Safety` section. Every other crate keeps
`unsafe_code = "forbid"`. A backend that needs `unsafe` (the LLVM bindings, 1.10) gets its own
record, not this one.

### The runtime is the one place with global state

`CONTRIBUTING.md` rule 1 forbids global mutable state. The runtime holds exactly one piece: the
thread pool, started once and shared by every loop, because a program has one set of cores and
a pool per loop would start threads on every loop. A panic also uses one flag so that two
threads panicking at once print one message. Both are documented where they are declared.

## Consequences

- The runtime is tested like the rest of the compiler: unit tests for chunks, layouts and
  messages, a test that runs a panic in a child process and checks its full output and exit
  status, and doc tests that call each exported function. A C program links the archive and
  runs a `prange`-style loop through it today.
- `unsafe` is contained: one crate, each use visible, explained and lint-checked. A reviewer can
  find every unsafe line with one search for `allow(unsafe_code`.
- The size goal is not met yet. Built with the standard library, a stripped C program that
  allocates an array and runs a parallel loop through the runtime is about 380 KB, against the
  design plan's 100 KB for a program that prints a number; most of it is the standard library's
  formatting and thread code. Roadmap 1.12.4 measures it once the driver links real programs and
  decides, with that measurement, between building the standard library with `panic = "abort"`
  and optimizing for size, and a `no_std` runtime over the C library. The interface above does
  not change either way.
- The runtime's lint table is a copy and can drift from the workspace's. The comment above it in
  `crates/runtime/Cargo.toml` says to change both together.
- The thread count follows the machine's cores; there is no setting yet. A way to choose it,
  such as an environment variable, is user-visible, so it is an idea on the roadmap (I.15), not
  part of this record.
- Accumulated arrays in `prange` (1.12.1) build on the same chunks: each chunk adds into its
  blocked private buffer, combined in a fixed tree, as decision 0013 describes.
