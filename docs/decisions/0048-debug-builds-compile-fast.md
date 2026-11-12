# 0048. Debug builds compile fast and run unoptimized; Cranelift must earn its place

Status: Accepted
Date: 2026-11-12
Roadmap: 2.1, 2.5

## Context

The design plan makes compile speed a product requirement: a debug build at 100,000 lines per
second, and a one-function edit rebuilt in under 200 ms. It planned to meet them with a second
backend, Cranelift, for debug builds, generating code "10 to 20 times faster than LLVM" with no
optimization beyond register allocation (roadmap 2.1).

Decision [0030](0030-integer-overflow.md) then made both build modes optimized: a debug build
differs from a release build only in that integer overflow panics. Every build runs `clang -O2`.

Roadmap 2.0 measured where a build's time goes (`baryon build --timings`, baseline in
[`bench/README.md`](../../bench/README.md#compile-speed)). On a 4,706-line program, Linux x86-64,
4 cores:

| Stage | Time |
| --- | --- |
| `baryon`: check, lower, IR passes, LLVM text | 0.30 s |
| `clang -O2`: optimize and write machine code | 9.0 s |
| `clang -O0` on the same module, for comparison | 1.8 s |
| Link with the runtime | 0.13 s |

So `clang -O2` is 95% of a build, about 500 lines per second. Levels in between were measured
too: the machine-code time of the same module, and the run time of the `nbody` kernel, 1,000
bodies for 200 steps, built the same way.

| Pipeline | Machine code, 4,706 lines | `nbody` run |
| --- | --- | --- |
| `clang -O0` | 1.6 s | 5.3 s |
| `opt` with `sroa`, `early-cse`, `simplifycfg`, `instcombine`, then `llc -O0` | 2.0 s | 1.4 s |
| The same passes, then `llc -O1` | 4.1 s | 0.80 s |
| `clang -O1` | 6.2 s | 0.74 s |
| `clang -O2` | 9.6 s | 0.42 s |

Every row printed the same checksum. The second row is the best trade, a quarter more build time
for a program four times faster than `-O0`, but `clang` cannot run it: it needs LLVM's `opt` and
`llc`, which Apple's Command Line Tools do not ship. Its gain is mostly `sroa`, which turns the
backend's stack slots into registers, and the backend can write registers itself.

Cranelift is also an unoptimized code generator, so it pays the same run-time price. What it would
add over `clang -O0` is unmeasured: the plan's "10 to 20 times" compares it with optimized LLVM,
and the gain over unoptimized LLVM is much smaller. It brings about ten external crates (rule 6)
and a second backend to keep in step with the first.

## Decision

Collins chose this on 2026-11-12.

- **A debug build is for the edit-run loop and is not optimized.** `baryon build`, `run` and
  `test` without `--release` compile at `clang -O0` (roadmap 2.5). A release build stays at
  `-O2`. This replaces one point of 0030, "both are optimized"; its overflow rules are unchanged.
- **A debug build's run time is won back in the backend, not in `clang`.** The LLVM backend
  writing scalar locals as registers instead of stack slots should give `-O0` much of what `sroa` gave
  the second row above, with no extra tool; it is part of roadmap 2.3.
- **The mid-level IR passes still run in both modes.** Fusion and bounds-check hoisting change
  what is computed, not how it is optimized, and keeping them means a debug build computes the
  same bits as a release build.
- **Parallel code generation comes before a second backend.** The driver splits the LLVM module
  into several and runs one `clang` per core, which serves both modes.
- **Cranelift is prototyped and measured, not assumed.** Roadmap 2.1 starts as a prototype
  behind the driver's existing backend interface, timed against `clang -O0` with parallel code
  generation on the 2.0 baseline. It becomes the debug backend only if it is at least twice as
  fast end to end; its crates are then justified in that pull request. Otherwise 2.1 is
  resized or dropped in the roadmap, and the REPL (3.11) chooses its JIT on its own.

## Consequences

- A debug build of the 4,706-line program goes from 9.5 s to 1.85 s, about 2,500 lines per
  second. The small benchmark kernels, where linking dominates, gain less: `nbody` goes from
  0.40 s to 0.20 s.
- Debug programs run up to an order of magnitude slower. Timing runs, benchmarks and long
  simulations use `--release`, as in Rust. The command reference, the beginner's guide and the
  benchmark instructions say so.
- A debug build and a release build of the same program give the same results to the bit, as
  today: `-O2` already adds float sums in order (`-force-ordered-reductions`), and `-O0` never
  reorders.
- The link step, about 120 ms, is now a large share of a small build, and on its own nearly uses
  up the 200 ms edit budget. Shrinking it (a prelinked or shared runtime, `lld`) is roadmap 2.3.
- The plan's 100,000 lines per second is still 50 times away after this. The rest has to come
  from emitting less LLVM IR (about 80 lines per source line today), parallel code generation,
  and incremental rebuilds (2.2).

## Notes

- 2026-11-12, after roadmap 2.5 to 2.7: a debug build of the 4,706-line program takes 0.9 s, of
  which `clang -O0` on 4 cores is 0.3 s, `baryon` itself 0.45 s and the link 0.17 s. A code
  generator that took no time at all would make it 0.9 / 0.6 = 1.5 times faster, so no
  Cranelift prototype can pass the twice test above. Roadmap 2.1 stays planned for its other
  role, the REPL's JIT (3.11, decision 0020); whether the design plan should still name
  Cranelift as the debug backend is Collins's call.
