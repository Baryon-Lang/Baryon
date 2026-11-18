# 0058. User types define operators with Python's dunder methods

Status: Accepted
Date: 2026-11-13
Roadmap: D.19

## Context

Only built-in types have operators today. A number-like library type, such as `f64x2`
([decision 0051](0051-double-double-floats.md)), a quantity with units, an interval or a
quaternion, has to spell arithmetic as methods: `a.mul(b).add(c)`. Python gives a class operators
through dunder methods, `__add__`, `__mul__`, `__eq__` and the rest, and Baryon already follows
Python's dunders for `with` and `for` on library classes.

## Decision

A struct defines operators with Python's dunder methods: `__add__`, `__sub__`, `__mul__`,
`__truediv__`, `__eq__`, `__lt__` and the rest, so `a + b` on a struct calls `a.__add__(b)`.
Collins chose this on 2026-11-13. The detailed rules, such as which dunders exist, how reflected
operators like `__radd__` and in-place ones like `__iadd__` are found, and how they meet number
mixing, are settled by the record that comes with the work.

### Calling a struct: `__call__`

A struct that defines `__call__` can be called like a function: `p(x)` calls `p.__call__(x)`,
with `p` passed as `self` in the mode the method declares, as Python calls an object. The
checker resolves the call to the method, so it costs what any method call costs. Collins chose
this on 2026-11-14 (card 43), so that SciPy's dense output, `sol.sol(t)` on an `OdeSolution`,
ports unchanged (roadmap 3.19.8).

A struct with `__call__` is not a function value: it cannot be passed to a `Callable`
parameter. Such a value carries data, so passing it would make it a closure, which decision
[0026](0026-functions-as-arguments.md) leaves out of version 0. Passing one is a checker error
that says so; a module-level function that calls the struct can be passed instead.

## Consequences

- Python code that defines operators ports unchanged, and `f64x2`, units and quaternions get
  `+` and `*`.
- The checker resolves an operator on a struct to a method call, compiled like any other call, so
  it costs nothing at run time.
- Units ([decision 0059](0059-units-compile-time-first.md)) build on this.
- `__call__` was built first, ahead of the operators of row 3.34, for `integrate`'s dense
  output.
