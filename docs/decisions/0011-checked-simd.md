# 0011. `simd` loops are a checked request, not an unchecked promise

Status: Accepted
Date: 2026-11-06
Roadmap: I.9

## Context

In the design plan, `simd for` asserts that a loop has no dependence between iterations. If the
programmer is wrong, results are silently wrong, which is unsafe behavior outside an `unsafe`
block. The plan's reference rules already prove that arrays never alias, so the compiler can
usually establish independence itself. What numeric programmers lack is not a way to assert
vectorization, but a way to learn why a loop did not vectorize.

## Decision

`for i in simd(range(n)):` asks the compiler to vectorize the loop. If it cannot, compilation
fails with the reason: the dependence it found, the operation it could not vectorize, or the call
it could not inline.

## Consequences

- `simd` can never produce wrong results; at worst it produces a compile error.
- The error doubles as the diagnostic numeric programmers want most: why a loop is slow.
- A loop that truly needs an unchecked independence assertion can still get one inside
  `with unsafe:`, if a real program shows the need.
- The compiler must carry vectorization decisions from the backend back to the source, which the
  optimization remarks work (roadmap 3.7) needs anyway.
