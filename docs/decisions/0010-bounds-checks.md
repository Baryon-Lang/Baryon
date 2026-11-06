# 0010. Bounds checks stay on in release builds

Status: Proposed
Date: 2026-11-06
Roadmap: I.4

## Context

The design plan promises memory safety but turns bounds checks off in release builds and allows
unchecked indexing with `a[i]!` anywhere. An out-of-bounds write is memory corruption, so release
builds would not be memory safe. Rust keeps bounds checks in release builds and pays little for
them, because the optimizer removes a check when it can prove the index is in range, as it can for
a loop over `range(len(a))`.

## Decision

- Every array access is bounds-checked in every build mode.
- Unchecked access, `a.unchecked[i]`, is allowed only inside `with unsafe:`.
- Bounds-check hoisting (roadmap 3.2) moves earlier, into Phase 1, if the Phase 0 benchmarks show
  checks costing more than 2% on any kernel.

## Consequences

- The memory-safety promise holds in the builds people run in production.
- Some loops will be slower until hoisting removes their checks. The benchmark suite (0.3)
  measures exactly how much, so the cost is known before Phase 1 starts.
- Code that truly needs unchecked access marks it as unsafe, where reviewers will look.
