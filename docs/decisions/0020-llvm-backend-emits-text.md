# 0020. The first LLVM backend writes textual IR and hands it to `clang`

Status: Accepted
Date: 2026-11-07
Roadmap: 1.10

## Context

Roadmap item 1.10 is the LLVM backend: it translates each `mir` body (decision 0018) into LLVM
IR, which LLVM optimizes and compiles to a Mach-O object for Apple silicon. The design plan's
technology table names "LLVM via `inkwell`" for this. That line was written before any backend
code existed and has no decision record, so this record settles it before code depends on it.

There are two ways for a Rust program to drive LLVM:

1. **Link LLVM as a library, through `inkwell`** (safe bindings) over `llvm-sys` (the C API).
   The compiler builds IR in memory and runs LLVM's passes in-process.
   - Building the compiler needs the LLVM development libraries, of the exact major version
     `llvm-sys` was built for, found through `llvm-config` or an environment variable. Every
     contributor's machine and both CI runners (macOS and Linux) must install that version,
     and a version bump is a coordinated change to all of them.
   - Two external crates, and a native library of several hundred megabytes behind them.
     `CONTRIBUTING.md` rule 6 asks compiler crates for zero external dependencies.
   - Fastest at compile time: no text is written or parsed, and the IR can be handed to LLVM's
     JIT later, which a REPL wants (roadmap 3.11, which plans to use Cranelift instead).
2. **Write LLVM's textual IR (`.ll`) and run `clang` on it.** The backend produces a string;
   `clang -O2 -c module.ll` optimizes it and writes the object file, and `clang` also links.
   - No new dependency of any kind. Building the compiler needs only Rust; running a build
     needs `clang`, which every Mac with the Xcode command line tools has and which the driver
     will use as the linker anyway (roadmap 1.11).
   - The textual format is LLVM's stable, documented interface. Every metadata the backend
     needs (`noalias`, `nonnull`, `dereferenceable`, debug info, loop metadata) has a text form.
   - Each backend test compares the emitted text of a small body against the expected `.ll`,
     the same full-text testing the rest of the compiler uses, with no LLVM linked into tests.
   - Costs: writing and re-parsing the text, and starting a process per module. Both are
     linear in the IR's size and small next to `-O2` optimization, which the design plan's own
     risk table names as what dominates release builds; the decision's last point says how
     that is checked. Errors in emitted IR surface as `clang` messages rather than Rust
     types, which the backend's tests and LLVM's verifier (run on every test module) catch.

## Decision

- **The first LLVM backend emits textual LLVM IR and invokes `clang` to optimize, compile and
  link.** It is an implementation crate, `llvm_backend`, that reads `mir` and `types` and
  returns the module text. It does not run processes: the driver, whose job that is, writes the
  file and runs `clang`.
- **The emitted IR targets the LLVM version the build machine's `clang` reports, from 17 up.**
  The backend uses only opaque pointers and constructs stable since LLVM 15.
- **Moving to in-process LLVM is an optimization, made with evidence.** When a benchmark of
  release builds shows writing and parsing text costs more than a few percent of build time,
  a new record replaces the text writer with `inkwell` behind the same backend interface. The
  translation logic, the tests' expected IR, and the driver's interface do not change.

## Consequences

- The backend lands with no dependency added, builds on any machine Rust builds on, and its
  tests run in the time the rest of the compiler's take.
- `clang` becomes a requirement of `baryon build`, alongside its role as the linker. The
  driver reports a missing `clang` with the command that installs it.
- The backend's tests compile each module with `clang`, link it with a small C program and run
  it, so they check what the code computes, Python's rounding for `//` and `%` included.
- The design plan's technology table changes from "LLVM via `inkwell`" to "LLVM, through its
  textual IR and `clang`", linking here.
- A JIT for the REPL stays with Cranelift (roadmap 3.11), as already planned.
