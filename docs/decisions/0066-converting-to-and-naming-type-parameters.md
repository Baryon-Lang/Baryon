# 0066. Converting to a type parameter, and type arguments on a constructor

Status: Accepted
Date: 2026-11-19
Roadmap: 1.9.59, 1.9.61

## Context

Two rules in the [Generics](../reference/types.md#generics) reference stop ordinary generic
code, and the pre-0.3.0 bug sweep found both. No decision record covered them:
[decision 0014](0014-number-conversions-are-calls.md) covers conversions of concrete numbers,
[decision 0026](0026-functions-as-arguments.md) generic functions and
[decision 0031](0031-traits-are-protocols.md) the traits that bound them.

- **A type parameter is not a conversion.** In
  `def mean[T: Float](xs: Array[T, 1]) -> T`, the line `return s / T(len(xs))` is an error, so a
  generic float cannot be divided by a count. The workaround is one function per element type.
- **A constructor takes no type arguments.** For `class Box[T](Struct)` with a field
  `items: list[T]`, `Box[f64]([])` is an error, and so is `c: Box[f64] = Box([])`, because the
  empty list fixes no `T`. The workaround is two lines: `e: list[f64] = []`, then `Box(e)`. A
  class function already takes them: `Bag[f64].empty()`.

Collins chose on 2026-11-19 at 02:12 UTC (card 50, "Accept all four"): allow `T(x)` from any
number when `T` is bounded by `Number` or `Float`, with decision 0014's panics on loss, and allow
`Box[f64]([])`. The same card accepted `.copy()` on any `T` (row 1.9.60), which this record
gains by amendment once its last detail is settled, and held narrowing a field of a borrowed
struct until after 0.3.0.

## Decision

1. **`T(x)` converts to a type parameter.** Inside a function or struct generic over `T`, where
   `T` is bounded by `Number` or `Float`, `T(x)` converts `x` to `T`. `x` may be any real number:
   an integer or float of any width, a number literal, or a value of type `T` itself. Each copy
   the compiler makes for a type argument converts exactly as the concrete conversion does
   ([Conversions](../reference/types.md#conversions)): `T(n)` with `T` an `f32` is `f32(n)`, and
   with `T` an `i8` it panics when `n` does not fit. A complex `x` is an error, since no
   conversion goes from a complex number to a real one; `T(x)` from a real `x` with `T` a
   `complex128` gives imaginary part `0.0`, as `complex(x)` does. Under any other bound, or none,
   `T(x)` stays an error.
2. **A constructor takes type arguments.** A generic struct's constructor may be written with its
   type arguments, `Box[f64]([])`, as a class function already is. They are the class's, and the
   arguments are checked against them: `Pair[f64](1, 2)` makes the literals `f64`, and
   `Pair[f64](x, y)` with `x: f32` is an error. Writing none keeps today's inference from the
   arguments. The type is not taken from an annotation: `c: Box[f64] = Box([])` stays an error
   that suggests `Box[f64]([])`.

## Alternatives

- **One function per element type.** It works today, and it is what generics exist to avoid.
- **Converting under any bound.** `T(x)` with `T` a `VectorSpace` or a user trait has no
  meaning for most of the types that satisfy it.
- **Taking a constructor's type from its annotation.** `c: Box[f64] = Box([])` would read well,
  but inference in Baryon is local by design: an expression's type never depends on where its
  value goes. It is a bigger change, left for a later card.

## Consequences

- `mean`, `variance` and the like are one generic function each, as in NumPy's generic code.
- The sentence in the reference that `T(0)` is an error is replaced, and so is the one that
  `Pair[f64](1.5, 2.5)` is an error.
- `T(x)` costs what the concrete conversion costs: nothing at run time for a widening one, and a
  range check where one can panic.
