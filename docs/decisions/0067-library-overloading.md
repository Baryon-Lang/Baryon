# 0067. Library functions may share a name, chosen by argument type

Status: Accepted
Date: 2026-11-21
Roadmap: 1.7.16

## Context

SciPy's `jv(v, z)` takes a real or a complex `z`, and the result's type follows `z`. Baryon's
`jv`, `yv`, `iv` and `kv` take and return `f64` (row 3.18.5). A complex form under the same name
needs two functions that share a name. Today the resolver rejects a top-level name defined twice
in one module ("`jv` is defined twice in this module"). A generic `jv[T: Number]` cannot do the
job either: `int` satisfies `Number` too, and a generic body cannot choose the real or the
complex algorithm by `T`. Decisions 0042 and 0046 say other element types "come with overloading
or generic functions", but neither decides overloading.

Checked on main with `baryon check`: an int literal is accepted where `f64` or `complex128` is
expected; an `i32` variable is not accepted where `f64` is; a user function can be bound to a
name (`k = g`) and called through it; a member of a standard module cannot (`f = math.sqrt` is a
compile error).

The chief designer chose on 2026-11-19 at 02:13 UTC (card 46, "Overload, library only"): library
functions may share a name, picked by argument type, so `jv(v, 1+2j)` works as in SciPy. On
2026-11-20 at 04:00 UTC the chief designer accepted the four details below (card 54, "Accept all
four") and one exception for generic functions (card 53, "Overloads, complex128 first").

## Decision

Points 1 to 3 are card 46's rule. Points 4 to 7 are card 54's, and the exception in point 6 is
card 53's.

1. **Only the standard library defines overloads.** A module under `<std>` may define two or
   more top-level functions with one name: the name's overloads. Everywhere else a name defined
   twice stays the error it is today. User code calls an overloaded name like any function.
2. **A call picks by argument types.** Each overload is checked against the call's arguments,
   positional and keyword, as an ordinary call is, and the one that accepts them runs. The return
   type plays no part in the choice, so overloads may return different types: `jv` of an `f64`
   gives an `f64`, and of a `complex128` a `complex128`. Rank is part of an array's type, so
   `Array[f64, 1]` and `Array[f64, 2]` are different argument types.
3. **No overload accepts the call.** That is a compile error listing each overload's signature,
   in the form of the ordinary argument-type error.
4. **Literals and ties.** An int literal fits `f64` and `complex128` alike, so `jv(1.0, 2)` fits
   both overloads. The overload that takes each literal at its nearest type wins: for an int
   literal an integer type, then `f64`, then `complex128`; for a float literal `f64`, then
   `complex128`. So `jv(1.0, 2)` is the real `jv`, as SciPy gives a float. Any other tie is a
   compile error naming both overloads. The library's tests make sure no call of non-literal
   arguments fits two overloads.
5. **An overloaded name as a value.** Where a `Callable[...]` is expected, an overloaded name
   stands for the one overload whose signature equals it. Anywhere else, and where none or more
   than one fits, it is a compile error. Standard-module members as values stay as they are.
6. **Concrete overloads, with one exception.** Every overload is a concrete, non-generic
   signature, except that a generic function may share a name with concrete overloads when its
   bound excludes every concrete overload's argument types, as `[T: Float]` excludes
   `complex128`. Any overlap between the generic's bound and a concrete overload is a compile
   error where the library defines them, so no call ever fits both.
7. **Re-exports.** `from special_bessel import jv as jv` re-exports every overload of `jv`
   together ([decision 0062](0062-standard-library-reexports.md)); a module cannot re-export only
   some of them.

The reference lists each overload of a name on its own line.

## Alternatives

- **New names such as `jv_complex`** (card 46's second option): 3.18.6 could start at once, but
  it breaks the rule that `special` keeps SciPy's names.
- **Overloading open to user code:** about 4 to 6 weeks of compiler work against 2 to 3, and a
  program's readers would have to find every overload of a name to know what a call does.
- **Generic functions that branch on `T`:** needs compile-time type tests, a larger change, and
  still lets `int` in through `Number`.

## Consequences

- 3.18.6 gives `jv`, `yv`, `iv` and `kv` their complex forms under SciPy's names.
- Array forms of `special` functions (row 3.18.13, idea I.61) and linear algebra by rank or
  element type can use the same dispatch.
- The checker resolves each call, and each overloaded name used as a value, to one overload.
  Later stages see only that function, so lowering and the backends do not change, apart from
  giving each overload a symbol of its own.
