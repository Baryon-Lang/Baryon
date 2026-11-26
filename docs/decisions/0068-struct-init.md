# 0068. A struct may define `__init__`, which builds it from other arguments

Status: Accepted
Date: 2026-11-26
Roadmap: 1.7.17

## Context

SciPy builds an interpolator with its class: `CubicSpline(x, y)` solves for the coefficients
and keeps them. Today a Baryon struct is built from its fields, in order, and nothing else
([Structs](../reference/types.md#structs)). Two things stand in the way (checked on main
6358dd5):

- A method that sets `self.slope` is an error, because `self` is read-only.
- A class and a function cannot share a name, so `def CubicSpline(x, y)` beside
  `class CubicSpline` is "defined twice".

The chief designer chose on 2026-11-19 (card 47, point 1) to add `__init__` to structs, so the
type keeps SciPy's name, and on 2026-11-20 accepted the seven points below as drafted (card 52,
"Accept all seven"). A user can then write `cs: CubicSpline` and hold one in a field of their own
struct. The same rule decides `KDTree(points)` in row 3.22.

## Decision

### What `__init__` is

1. **It replaces the field constructor.** A struct that defines `__init__(self, ...)` is built
   by calling the class with `__init__`'s arguments: `CubicSpline(x, y)`. Building it from its
   fields in order is then no longer possible, as in Python, where `__init__` is the only
   constructor a class has. A struct without `__init__` is built from its fields, as today.
2. **It sets every field.** On every path that reaches the end of `__init__`, or a `return`,
   every field has been assigned. A path that misses one is a compile error that names it:

   ```text
   error: `__init__` can end without setting `offset`
   ```

3. **`self` is the struct being built.** `self.field = value` sets a field, and `self.field`
   reads one that is already set. Reading a field before it is set is an error. `self` is not
   used as a whole: no `self.method()`, no `f(self)`, no `return self`. A struct that needs a
   helper's result computes it from the arguments first.
4. **It returns nothing and raises nothing.** `return` takes no value. A bad argument panics
   with SciPy's message, as row 3.21 does (card 47, point 6). A `Raises[None, E]` on
   `__init__` is an error in this version.
5. **It is called only by the class.** `p.__init__(...)` is an error.
6. **Generic structs** take their type arguments from `__init__`'s arguments, as a generic
   function's call does, or as written, `Box[f64](...)`.
7. **`Copy` structs and embedded structs** work the same. An embedded struct
   ([decision 0031](0031-traits-are-protocols.md)) is one field, set as a whole.

### How it is built

The checker checks the body as a method whose `self` starts empty and tracks which fields are
set on each path, as it tracks whether a variable is assigned. Lowering keeps each field in a
local of its own while `__init__` runs, and builds the struct from those locals where the body
ends or returns. The struct is never half built in the IR, so drop elaboration and the backends
see nothing new: a field set twice drops its old value as a variable does.

## Alternatives considered

- **A function named as SciPy's class, returning a struct with another name**, such as
  `_CubicSpline`. Every call ports unchanged, but a type annotation or a field must name
  `_CubicSpline`, and error messages show it. Checked: it works today.
- **A class function, `CubicSpline.fit(x, y)`.** Works today, but every call differs from SciPy.
- **Keeping the field constructor alongside `__init__`.** Two ways to build the same type,
  told apart by argument count, is overloading by another name, and Python has only one.

## Consequences

- SciPy's constructors port unchanged: `CubicSpline(x, y)`, `PchipInterpolator(x, y)`,
  `RegularGridInterpolator((x, y), values)` and, later, `KDTree(points)`.
- A library struct that also needs to be built from its parts, such as a spline's
  `derivative()` returning a new spline from coefficients, gives `__init__` arguments that take
  those parts, as SciPy's `PPoly(c, x)` does.
- Cost: not yet checked; estimated 1.5 to 2 weeks for the checker rule, the lowering, tests and
  the reference entry.
