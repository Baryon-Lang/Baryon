# 0012. No foreign function interface before Phase 4

Status: Accepted
Date: 2026-11-06
Roadmap: D.7

## Context

The design plan defers all interop with other languages to Phase 4, so that nothing in the core is
shaped by another language's conventions. Yet it also lists an `ffi` module in the MVP standard
library, lets `unsafe` blocks call C, puts a C ABI in the Phase 0 language reference, and
describes a `row_major` attribute "for C interop". The plan contradicts itself in four places.

## Decision

- The MVP has no user-facing foreign function interface: no `ffi` module, no calls to C from
  `unsafe`, no C ABI in the language reference.
- The standard library reaches libm, BLAS and LAPACK through an internal mechanism that user code
  cannot name.
- Exporting functions to Python (roadmap idea I.5) is decided separately, when that work is
  planned.

## Consequences

- The core types, calling convention and array layout are designed for simulation code alone.
- The MVP is smaller: the `ffi` module and C-call support leave Phase 1.
- Nobody can call an existing C library from the language until Phase 4 or I.5. Libraries the
  data-science half needs (HDF5, NetCDF) must wait for that, or for the internal mechanism to grow.
