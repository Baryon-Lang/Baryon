# 0006. Parameters are read-only borrows unless marked `InOut` or `Owned`

Status: Accepted
Date: 2026-11-06
Roadmap: I.3, D.3

## Context

In the design plan, a parameter with a bare type is moved into the callee. Its own example,
`mean(y)`, would therefore move `y`, and the caller could not use `y` again. A Python programmer
calls helper functions on arrays constantly, so every such call would end the array's life and
produce a "value was moved" error. Hylo and Mojo, which share the plan's no-lifetimes model, pass
parameters as read-only borrows by default instead.

The plan also leaves open whether a mutable argument is marked at the call site as well as in the
signature (roadmap decision D.3).

## Decision

| Parameter | Meaning | Spelling |
| --- | --- | --- |
| default | Read-only borrow. The caller keeps the value. | `x: Array[f64, 1]` |
| mutable | Exclusive borrow. The callee's changes are visible to the caller. | `y: InOut[Array[f64, 1]]` |
| consuming | The value moves into the callee; the caller can't use it afterward. | `buffer: Owned[Array[f64, 1]]` |

- Borrows are second-class, as in the plan: they cannot be stored, returned or captured.
- Assignment `b = a` still moves, with an error that suggests `.copy()` if `a` is used again.
- Call sites carry no marker: `saxpy(2.0, x, y)`. The editor shows `InOut` arguments as an inlay
  hint. Decision D.3 is revisited after the Phase 0 example programs.

## Consequences

- Calls behave the way a Python programmer expects: passing an array to a function never makes it
  unusable.
- `ref T` disappears from the language; read-only is simply the default.
- Small `Copy` types (numbers, `Fixed` arrays) are passed by value under the hood when that is
  faster; the borrow rules are unchanged.
- Mutation is visible in the signature but not at the call site, which is the trade-off D.3
  weighs.
