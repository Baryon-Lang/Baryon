# What builds today

The rest of this reference describes the whole of version 0. The compiler does not build all of it
yet. This chapter says, chapter by chapter, what `baryon build`, `baryon run` and `baryon test`
compile into a working program today, and what they still reject.

Two stages decide that. `baryon check` already accepts every rule in the reference: a program
that breaks one gets the error the reference describes. Building goes further: lowering turns the
checked program into the mid-level IR (roadmap 1.9.3), and the LLVM backend turns that into machine
code (roadmap 1.10). Each grows one feature at a time. A feature either stage cannot handle yet
is reported as an error that names it and the stage, never miscompiled:

```text
error: lowering does not handle `**` yet
 --> main.byn:3:11
  |
3 |     print(2 ** n)
  |           ^^^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues
```

So a program that `baryon check` accepts but `baryon build` rejects is not wrong: it uses a part of
the language that is specified and checked, but not built yet.

This page was last brought up to date on 2026-11-08, when functions passed as arguments began
to build. Every pull request that teaches lowering or the backend something new updates the table
it affects, in the same pull request, as roadmap item 0.8.1 asks.

## The example programs

Three of the five [example programs](../examples/README.md) build and run:
[`heat_equation.byn`](../examples/heat_equation.byn),
[`lennard_jones.byn`](../examples/lennard_jones.byn) and
[`lorenz_rk4.byn`](../examples/lorenz_rk4.byn). Each of the others needs the features listed
against it.

| Example | Still needs |
| --- | --- |
| [`heat_equation.byn`](../examples/heat_equation.byn) | Nothing: it builds and runs |
| [`lennard_jones.byn`](../examples/lennard_jones.byn) | Nothing: it builds and runs |
| [`lorenz_rk4.byn`](../examples/lorenz_rk4.byn) | Nothing: it builds and runs |
| [`conjugate_gradient.byn`](../examples/conjugate_gradient.byn) | lists and `list.append`, `Array(values)` |
| [`monte_carlo_pi.byn`](../examples/monte_carlo_pi.byn) | the `random` module, `math.pi`, operators on structs |

## Syntax

| Builds today | Not built yet |
| --- | --- |
| Packages of several modules; `import m`, `from m import x`, `as`; calling a function through its module, `physics.units.twice(x)` | Reading a constant through its module, `physics.units.C`; import it by name instead |
| `main` in `main.byn`; `print` of numbers, `bool` and `str` | `print` with keyword arguments; `print` of `None`, `T \| None`, tuples, arrays and structs |
| Assignment, annotated assignment, augmented assignment, `**=` on floats only; unpacking a tuple or a rank-1 `Fixed` array, `a, b = b, a` | `**=` on integers; nested unpacking, `a, (b, c) = e` |
| `if`, `elif`, `else`, `while`, `break`, `continue`, `pass`, `return`, `assert` | |
| `for` over `range` with a constant step, positive or negative | `for` over arrays, lists, strings or tuples; a `range` step that is not a constant |
| `def` at module level and methods; keyword arguments; [default parameter values](types.md#default-parameter-values) that are literals or `Final` constants | Leaving out an argument whose default is any other expression |
| `class` with `Struct`, `Copy`, `Exception` and `Enum` bases; `@layout("C")` | |
| `raise` and `try`/`except`, including bare `raise` | `raise ... from` |
| `match` on numbers, `bool`, enums, `T \| None` and tuples of these, with `\|` patterns and guards | `match` on `str`; `\|` patterns that capture names |
| `with unsafe:` | |
| Number, string, `True`, `False` and `None` literals; f-strings with format specifications | Complex literals such as `2j`; string literals with escapes or prefixes |
| `+ - * / // %`, `**` on floats, unary `-` and `+`, bitwise operators, chained comparisons, `and`, `or`, `not`, `is None`, conditional expressions | `**` on integers; `in` and `not in`; `==` on `str` |
| `min`, `max`, `len` of arrays, `int`, `float` and the number conversions such as `f64(n)`; [`math`](arrays.md#math-functions) `sqrt`, `exp`, `log`, `sin`, `cos` and `pow`; `round(x)` | `abs`, `round(x, digits)`, `sum`, `enumerate`, `zip`; `len` of `str` and lists; the rest of `math`, and its constants such as `math.pi` |
| `@test` functions, with `baryon test` | |

## Types

| Builds today | Not built yet |
| --- | --- |
| Every integer type, `f32`, `f64` and `bool`, with Python's number mixing, `//` and `%` | `complex64` and `complex128` |
| Conversions that panic when the value does not fit | |
| `str`: literals, f-strings, `+` and `s.copy()` | `==`, `!=`, `in` and `len` on `str` |
| Tuples: building one, reading `t[0]` with a constant index, and unpacking | |
| `T \| None`, with every narrowing rule | |
| Structs, `Copy` structs, fields, methods, `InOut[Self]` methods, generic structs | |
| Enums: members, `==` and `!=`, `match` | |
| Type aliases, including as a constructor: `Vec3(1.0, 0.0, 0.0)` | |
| `Final` constants of numbers, including `Final[f32]` and arithmetic on other constants | `Final` constants that are not numbers |
| Generic functions, compiled once per type they are called at, including `VectorSpace` arithmetic on `Fixed` vectors | |
| [Functions as arguments](types.md#functions-as-arguments): a module-level function passed to a `Callable` parameter, directly or through a local, compiled once per function passed (decision 0026) | Passing on a `Callable` parameter to another call; copying a `Callable` parameter into a local when another `Callable` parameter of the same signature received a different function |
| | Lists, `list[T]`, in any form |

Integer overflow: there is one build mode today, and arithmetic that overflows an integer wraps
around, as the release-build rule says. The panic the debug-build rule promises is not built yet;
roadmap decision D.11 may change both.

## Values and references

Every rule in this chapter is checked by `baryon check`, so a program that breaks one never builds.
What it means at run time is built: values move and copy as described, read-only borrows, `InOut`
and `Owned` parameters work, and every value is destroyed when its owner goes away, on every path
out of a function, including through a `raise`.

| Builds today | Not built yet |
| --- | --- |
| Moves, copies, `.copy()` on `str` and `Array` | `.copy()` on lists and structs |
| `InOut` and `Owned` parameters, including `InOut` arrays and whole-array arithmetic on them | Slices, as views passed to a function or as the target of an assignment |

## Arrays

| Builds today | Not built yet |
| --- | --- |
| `Array.zeros`, `Array.ones` and `Array.fill`, with the element type from context, a `dtype` or `f64`; `a.copy()` | `Array(values)` |
| Indexing `a[i, j]` with bounds checks, reading and writing; negative literal indices | Slices `a[1:n]` |
| `len(a)`; `a.shape` bound to a name, then indexed | `a.shape[0]` written directly; `a.size` |
| `a.unchecked[i]` inside `with unsafe:` | |
| `Fixed` arrays: calling the type with elements, `Vec3.zeros()`, indexing, `len`, unpacking `x, y, z = v`, `v.dot(w)`, `v.round()`, and whole-array arithmetic | |
| Arrays of `Fixed` arrays and of `Copy` structs | |
| Whole-array arithmetic, `+= -= *= /=` and expressions such as `a + 2.0 * b` | `@`, the matrix product |
| `a.sum()` of numbers, floats in [NumPy's pairwise order](arrays.md#the-order-sum-adds-in) | `a.min()`, `a.max()`; `a.dot(b)` and `a.round()` on an `Array`; `a.fill(value)`; printing an array |

## Errors and panics

Everything in this chapter builds: error types, `Raises`, `raise`, errors passed on through
callers, `try` with `except` naming one type or a tuple of them, `as`, bare `raise`, `main`
ending with an error, and every panic in the table, with its message, location and backtrace.
The one exception is the overflow panic, described under [Types](#types).

## Parallel and vector loops

| Builds today | Not built yet |
| --- | --- |
| `prange` with the same arguments as `range`, its inferred reductions and their reproducible combine; `accumulate=` of array variables and of struct fields such as `atoms.forces`, adding into elements and their lanes | Adding into a slice of an accumulated array |
| `simd(range(...))`, which asks LLVM to vectorize the loop | A `simd` loop that sums or multiplies floats into a shared variable runs unvectorized until its 8 fixed lanes are built (roadmap 1.9.13); an error when LLVM cannot vectorize a `simd` loop, such as one whose bounds checks it cannot move out of the loop (roadmap 3.7); until then such a loop runs unvectorized |

`baryon check` applies every rule of the chapter, so a `prange` loop that breaks one is reported
today, before it can be built.

## Tests

Everything in this chapter builds: `@test` functions anywhere in the package, tests that raise, and
`baryon test` with its report and exit status.
