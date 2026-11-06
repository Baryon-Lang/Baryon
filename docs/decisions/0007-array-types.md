# 0007. One array family: `Array` on the heap, `Fixed` inline

Status: Accepted
Date: 2026-11-06
Roadmap: I.8

## Context

The design plan names four array forms: `[T; N]`, `Array[T]`, `Array[T, N]` and
`Array[f64, (3, 3)]`. `Array[f64, 3]` is ambiguous (rank 3, or length 3?), and `[T; N]` is Rust
syntax that Python can't parse (decision 0004). Physics code needs two very different things:
huge grids and particle arrays whose size is known only at run time, and tiny vectors and tensors
(a position, a 3 × 3 stress tensor) that should live in registers.

## Decision

- `Array[T, rank]`: an N-dimensional array whose shape is known at run time, stored on the heap.
  `Array[f64, 3]` is a 3-dimensional grid. Slices of it are views with an offset and strides, not
  copies.
- `Fixed[T, shape]`: an array whose shape is part of its type, stored inline with no heap
  allocation, with operations unrolled by the compiler. `Fixed[f64, (3,)]` is a position;
  `Fixed[f64, (3, 3)]` is a stress tensor. It is a `Copy` type.
- There is no third array type. `[T; N]` is removed from the design.

## Consequences

- Every array in a program is one of two types, chosen by one question: is the shape known when
  the code is written?
- Shapes are explicit in signatures, which lets the compiler unroll small loops and prove bounds
  for `Fixed` arrays at compile time.
- `Fixed` arrays with large shapes would bloat the stack and the generated code. The compiler will
  warn above a size limit set in the language reference.
