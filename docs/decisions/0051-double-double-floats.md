# 0051. Double-double floats are an `f64x2` struct in `math`, with methods until operators exist

Status: Accepted (Collins, 2026-11-13)
Date: 2026-11-13
Roadmap: 3.33

## Context

An `f64` holds about 16 significant digits. Some calculations need more in a few places: the
residual `b - A x` in iterative refinement of an ill-conditioned solve, a long accumulation, or a
check of an `f64` result computed another way. A software `f128` costs 30 to 100 times an `f64`
operation, because neither x86_64 nor Apple silicon has one in hardware, and Baryon has none
(`f64` is the default float). A double-double holds a value as the unevaluated sum of two
`f64`s, `hi + lo`, for about 32 digits at 5 to 20 times the cost of an `f64` operation, with
`fma` doing the hard part. Roadmap 3.33 asks for one as a library type written in Baryon.

Baryon cannot yet let a struct define `+` or `*` (roadmap D.19), and a struct has no functions
called on the class itself, such as `f64x2.of(x)`. So the question was how the type is spelled
until operators exist. Collins chose among three options on 2026-11-13: methods on an `f64x2`
struct imported from `math` (this record), functions in a module of their own, such as
`dd.add(a, b)`, or waiting for D.19 and building nothing yet. He took the first.

## Decision

1. **An `f64x2` struct in the standard `math` module**, written in Baryon in
   `<std>/math.byn` (decision 0038): `from math import f64x2`. Its fields are `hi` and `lo`,
   with `hi` the value rounded to an `f64` and `lo` no larger than half a unit in the last place
   of `hi`. It is `Copy`, as a number is.
2. **Built from an `f64` with the constructor, `f64x2(x, 0.0)`**, which is exact. The card
   proposed `f64x2.of(x)`, but a struct has no class-level functions yet; the constructor needs
   none and stays correct when they arrive.
3. **Methods for everything else.** `add`, `sub`, `mul` and `div` take another `f64x2`; `sqrt`,
   `neg` and `abs` take none; `eq`, `ne`, `lt`, `le`, `gt` and `ge` compare; `to_f64()` rounds
   to an `f64` and `to_str()` writes 31 significant digits in scientific notation. Python's
   operator names without the underscores, so a formula reads left to right:
   `a.mul(b).add(c)` for `a * b + c`.
4. **Operators come on top.** When D.19 lets a struct define `+`, `-`, `*`, `/` and the
   comparisons, they call these methods, and nothing is renamed.
5. **IEEE 754 at the edges.** An infinite or NaN result has `lo == 0.0` and the `hi` an `f64`
   would give; `sqrt` of a negative value is NaN, as `math.sqrt` gives, the rule every
   [`math` function](../reference/arrays.md#math-functions) follows.

## Consequences

- Double-double code is wordier than `f64` code until D.19. It is meant for the few lines that
  need the digits, not whole programs.
- The algorithms are the classic ones (Dekker, Knuth, the QD library) with `fma` for exact
  products; each operation is accurate to a few units of 2**-104. They must not be compiled
  inside `with fastmath:`, whose reordering removes the extra digits.
- `to_str` can be one off in its last digit, as double-double digit generation allows.
- `print(a)` writes the struct's fields, `f64x2(hi=..., lo=...)`; `a.to_str()` writes the
  decimal value.

## Amendment, 2026-11-14: `f64x2.of(x)`

Point 2 built an `f64x2` from an `f64` with the constructor because a struct had no functions
called on its class. The card Collins chose on 2026-11-13 proposed `f64x2.of(x)`. Row 1.9.28
([#398](https://github.com/Baryon-Lang/Baryon/pull/398)) made such functions work, so `f64x2`
now has `of` too (roadmap 3.33.1):

- `f64x2.of(x)` is `x` as an `f64x2`, exactly, the same value as `f64x2(x, 0.0)`.
- The constructor stays, and both spellings build the same value. `of` reads as a conversion,
  which is what it is. The constructor shows the two parts, which code written against the
  algorithms needs.
- `f64x2.of(0.1)` holds the `f64` nearest 0.1, as `f64x2(0.1, 0.0)` does. To get 0.1 to 32
  digits, divide two exact values: `f64x2.of(1.0).div(f64x2.of(10.0))`.
