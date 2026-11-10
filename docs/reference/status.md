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
the language that is specified and checked, but not built yet. The editor sees the same split:
[Coding Baryon in VS Code](../guide/vscode.md) underlines every error `baryon check` finds as you
type, while `baryon run` builds only what this page lists.

This page was last brought up to date on 2026-11-09, when the rest of `math` for real numbers
began to build. Every pull request that teaches lowering or the backend something new
updates the table it affects, in the same pull request, as roadmap item 0.8.1 asks.

## The example programs

All five [example programs](../examples/README.md) build and run:
[`heat_equation.byn`](../examples/heat_equation.byn),
[`lennard_jones.byn`](../examples/lennard_jones.byn),
[`lorenz_rk4.byn`](../examples/lorenz_rk4.byn),
[`conjugate_gradient.byn`](../examples/conjugate_gradient.byn) and
[`monte_carlo_pi.byn`](../examples/monte_carlo_pi.byn). Each is built and run by a test that
checks what it prints.

| Example | Still needs |
| --- | --- |
| [`heat_equation.byn`](../examples/heat_equation.byn) | Nothing: it builds and runs |
| [`lennard_jones.byn`](../examples/lennard_jones.byn) | Nothing: it builds and runs |
| [`lorenz_rk4.byn`](../examples/lorenz_rk4.byn) | Nothing: it builds and runs |
| [`conjugate_gradient.byn`](../examples/conjugate_gradient.byn) | Nothing: it builds and runs |
| [`monte_carlo_pi.byn`](../examples/monte_carlo_pi.byn) | Nothing: it builds and runs |

## Syntax

| Builds today | Not built yet |
| --- | --- |
| Packages of several modules; `import m`, `from m import x`, `as`; calling a function, reading a constant and naming an enum member through its module, `physics.units.twice(x)`, `units.C`, `units.Phase.GAS` | |
| `main` in `main.byn`; `print` of numbers, `bool` and `str` | `print` with keyword arguments; `print` of `None`, `T \| None`, tuples, arrays and structs |
| Assignment, annotated assignment, augmented assignment, `**=` on floats only; unpacking a tuple or a rank-1 `Fixed` array, `a, b = b, a` | `**=` on integers; nested unpacking, `a, (b, c) = e` |
| `if`, `elif`, `else`, `while`, `break`, `continue`, `pass`, `return`, `assert` | |
| `for` over `range` with a constant step, positive or negative | `for` over arrays, lists, strings or tuples; a `range` step that is not a constant |
| `def` at module level and methods; keyword arguments; [default parameter values](types.md#default-parameter-values) that are literals or `Final` constants | Leaving out an argument whose default is any other expression |
| `class` with `Struct`, `Copy`, `Exception` and `Enum` bases; `@layout("C")` | |
| `raise` and `try`/`except`, including bare `raise` | `raise ... from` |
| `match` on numbers, `bool`, enums, `T \| None` and tuples of these, with `\|` patterns and guards | `match` on `str`; `\|` patterns that capture names |
| `with unsafe:` | |
| Number, string, `True`, `False` and `None` literals, with Python's backslash escapes such as `\n`, `\t` and `\u00e9`, and raw strings `r"..."`; f-strings with format specifications | Complex literals such as `2j`; `\N{name}` escapes; bytes `b"..."`; a literal written in parts, `"a" "b"` |
| `+ - * / // %`, `**` on floats, unary `-` and `+`, bitwise operators, chained comparisons, `and`, `or`, `not`, `is None`, conditional expressions | `**` on integers; `in` and `not in`; `==` on `str` |
| `min`, `max`, `len` of arrays and lists, `int`, `float` and the number conversions such as `f64(n)`; the [`math`](arrays.md#math-functions) functions for real numbers, from `sqrt` and `atan2` to `erf`, `fma`, `floor` and `isnan`, and the constants `pi`, `e`, `tau`, `inf` and `nan`; [`random.Philox`](arrays.md#random-numbers) and `uniform()`; `round(x)`; `abs` of integers and floats | `abs` of a complex number; `float(text)` and the other conversions of a `str` to a number; `round(x, digits)`, `sum`, `enumerate`, `zip`; `len` of `str`; the integer and sequence functions of `math`, such as `gcd` and `fsum`; the rest of `random` |
| `@test` functions, with `baryon test` | |

## Types

| Builds today | Not built yet |
| --- | --- |
| Every integer type, `f32`, `f64` and `bool`, with Python's number mixing, `//` and `%` | `complex64` and `complex128` |
| Conversions that panic when the value does not fit | |
| `str`: literals, f-strings, `+` and `s.copy()` | `==`, `!=`, `in` and `len` on `str` |
| Tuples: building one, reading `t[0]` with a constant index, and unpacking | |
| `T \| None`, with every narrowing rule | |
| Structs, `Copy` structs, fields, methods, `InOut[Self]` methods, generic structs and their methods, each method compiled once per type its struct is used at | |
| Enums: members, `==` and `!=`, `match` | |
| Type aliases, including as a constructor: `Vec3(1.0, 0.0, 0.0)` | |
| `Final` constants of numbers, including `Final[f32]` and arithmetic on other constants | `Final` constants that are not numbers |
| Generic functions, compiled once per type they are called at, including `VectorSpace` arithmetic on `Fixed` vectors | |
| [Functions as arguments](types.md#functions-as-arguments): a module-level function passed to a `Callable` parameter, directly or through a local, compiled once per function passed (decision 0026); a `Callable` parameter passed on to another call or copied into a local | A local that holds one of two different functions depending on a branch, which needs a function value at run time: the build reports it |
| Lists of copy types, such as `list[f64]`: displays `[a, b]` and `[]`, `xs.append(value)`, `len(xs)`, and `xs[i]` with bounds checks, reading and writing | Lists of other types, such as `list[str]`; `pop`, slices, `in` and comprehensions |

Integer overflow ([decision 0030](../decisions/0030-integer-overflow.md)): a debug build panics
on `+`, `-`, `*`, `+=`, `-=`, `*=` and `-x` that overflow, and `--release` wraps. That includes
arithmetic in a generic function whose type parameter is an integer, `abs` of the type's minimum,
whole-array arithmetic on integer arrays, and integer `a.sum()` and `v.dot(w)` (roadmap 1.11.8).
Indices the compiler computes itself, such as a `range` loop's counter, are not checked.

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
| `Array.zeros`, `Array.ones` and `Array.fill`, with the element type from context, a `dtype` or `f64`; `a.copy()`; `Array(values)` of a list | `Array.zeros` and `Array.ones` of an enum, whose members are not numbers; `Array.fill` with a member works |
| Indexing `a[i, j]` with bounds checks, reading and writing; negative literal indices | Slices `a[1:n]` |
| `len(a)`, `a.shape`, `a.shape[k]` and `a.size`, of `Array` and `Fixed` arrays | `print(a.shape)`, since `print` of a tuple is not built |
| `a.unchecked[i]` inside `with unsafe:` | |
| `Fixed` arrays: calling the type with elements, `Vec3.zeros()`, indexing, `len`, unpacking `x, y, z = v`, `v.dot(w)`, `v.round()`, and whole-array arithmetic | |
| Arrays of `Fixed` arrays and of `Copy` structs | |
| Whole-array arithmetic, `+= -= *= /=` and expressions such as `a + 2.0 * b` | `@`, the matrix product |
| `a.sum()` of numbers, floats in [NumPy's pairwise order](arrays.md#the-order-sum-adds-in) | `a.min()`, `a.max()`; `a.dot(b)` and `a.round()` on an `Array`; `a.fill(value)`; printing an array |

## Errors and panics

Everything in this chapter builds: error types, `Raises`, `raise`, errors passed on through
callers, `try` with `except` naming one type or a tuple of them, `as`, bare `raise`, `main`
ending with an error, and every panic in the table, with its message, location and backtrace.
The one exception is the overflow panic: it names the operator but not yet its operands (roadmap
idea I.18), and [Types](#types) lists the operations it does not check yet.

## Parallel and vector loops

| Builds today | Not built yet |
| --- | --- |
| `prange` with the same arguments as `range`, its inferred reductions and their reproducible combine; `accumulate=` of array variables and of struct fields such as `atoms.forces`, adding into elements and their lanes | Adding into a slice of an accumulated array |
| `simd(range(...))`, which asks LLVM to vectorize the loop | A `simd` loop that sums or multiplies floats into a shared variable runs unvectorized until its 8 fixed lanes are built (roadmap 1.9.14); an error when LLVM cannot vectorize a `simd` loop, such as one whose bounds checks it cannot move out of the loop (roadmap 3.7); until then such a loop runs unvectorized |

`baryon check` applies every rule of the chapter, so a `prange` loop that breaks one is reported
today, before it can be built.

## Tests

Everything in this chapter builds: `@test` functions anywhere in the package, tests that raise, and
`baryon test` with its report and exit status.
