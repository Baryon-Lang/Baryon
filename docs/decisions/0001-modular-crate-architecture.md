# 0001. Modular crate architecture with enforced dependency rules

Status: Accepted
Date: 2026-11-06

## Context

The compiler will grow through prototypes: a first backend that emits C, then LLVM, then
Cranelift; possibly a parser replaced once the grammar settles. Each replacement must not ripple
through the rest of the compiler. Conventions written only in prose erode, especially with many
contributors and AI agents working in parallel.

## Decision

- Split the compiler into small crates in one Cargo workspace.
- Give every crate a kind (`foundation`, `interface`, `implementation`, `application`, `tooling`)
  declared in its manifest, with dependency rules per kind as listed in
  [`docs/architecture.md`](../architecture.md).
- Forbid any implementation crate from depending on another. Stages exchange data only through
  interface crates.
- Enforce the rules mechanically with `cargo xtask check-architecture`, run by CI on every pull
  request.

## Consequences

- Any implementation can be rewritten or swapped while the rest of the compiler stays untouched.
- Crates compile in parallel and are tested in isolation.
- Interfaces need deliberate design up front, and changing one is a visible, separate change.
- More crates means more manifests. We accept that and keep each one short by inheriting
  workspace settings.
