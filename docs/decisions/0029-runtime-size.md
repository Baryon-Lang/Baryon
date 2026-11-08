# 0029. The runtime keeps the standard library; programs drop unused code, not `std`

Status: Accepted
Date: 2026-11-08
Roadmap: 1.12.4

## Context

Roadmap item 1.12.4 asks for a program that prints a number to link into less than 100 KB.
The item says to measure first, then choose between building the standard library for size
and a `no_std` runtime. The runtime is a Rust static library, and it uses `std` for threads,
allocation, output and panic backtraces ([0021](0021-runtime-crate-and-unsafe-code.md)).

Measured on Linux x86-64 for `def main(): print(42)`, with clang 18 and the pinned stable
Rust:

| Program | Size |
| --- | --- |
| `baryon build` before this item: `-O2 -g`, runtime linked whole | 5.2 MB |
| The same, linked with `--gc-sections` | 4.3 MB |
| The same, also stripped of debug information | 317 KB |
| A one-function `std` static library: `opt-level = "z"`, LTO, `panic = "abort"`, stripped | 271 KB |
| A one-function `no_std` static library that calls `write`, stripped | 14 KB |
| C `printf("%d\n", 42)`, stripped | 14 KB |

Most of the 4.3 MB is debug information, which panic backtraces read
([1.12.3](../../ROADMAP.md)). Of the 317 KB of code, about 76 KB is `std`'s backtrace
symbolizer (`gimli`, `addr2line`, `miniz_oxide`, `rustc_demangle`), and most of the rest is
`std` and `core` formatting and panic machinery. The runtime's own code is under 1 KB here.

The fourth row is the floor for any runtime that uses `std` on stable Rust: about 260 KB.
Building `std` for size (`-Z build-std`, `panic_immediate_abort`) goes below it, but needs a
nightly compiler, and the toolchain is pinned to stable so every machine builds the same thing.
So 100 KB is reachable only with a `no_std` runtime.

## Decision

The runtime keeps `std`. The 100 KB target is dropped.

- **Now.** `baryon build` links with `--gc-sections` (`-dead_strip` on macOS), which drops
  code nothing calls and keeps line tables for the code it keeps: 5.2 MB to 4.3 MB.
- **With release builds (roadmap 1.11).** A release build leaves out debug information, which
  gives about 320 KB. A debug build keeps it, because backtraces need it.
- **`no_std` waits for a use that needs it.** An embedded target or a size-sensitive
  deployment would justify it, with its own decision record.

Why not `no_std` now:

- **The mission.** Baryon programs are simulations and analyses. They run for minutes to
  days on arrays of megabytes to terabytes, so 300 KB versus 100 KB of code changes neither
  their speed nor their memory.
- **The cost.**
  - The thread pool would move from `std::thread` to raw `pthread` and Windows threads, which
    means more `unsafe` code.
  - The allocator and output would move to hand-written `libc` bindings.
  - Panic backtraces would need their own DWARF reader, or would go away.
  - Every runtime change after that costs more.
- **Reversibility.** A `no_std` runtime can replace this one later behind the same exported
  functions, as 0021 requires of every runtime. Nothing built now blocks it.

## Consequences

- A program that prints a number links at 4.3 MB with debug information today, and at about
  320 KB once release builds leave it out.
- Roadmap 1.12.4 closes with the `--gc-sections` change and this record. A `no_std` runtime
  is not on the roadmap until a use needs it.
- If the target matters after all, the path is a `no_std` runtime, about 14 KB per program,
  and the cost above.
