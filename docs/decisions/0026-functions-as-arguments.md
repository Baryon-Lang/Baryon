# 0026. A function passed as an argument is a compile-time argument, compiled once per function

Status: Accepted
Date: 2026-11-08
Roadmap: 1.9.12

## Context

The reference ([Functions as arguments](../reference/types.md#functions-as-arguments)) promises
two things about a parameter of type `Callable[[f64, Vec3], Vec3]`:

- In version 0, the argument is the name of a module-level function, not a closure.
- The compiler makes one copy of the receiving function per function passed to it, so the call
  is inlined and costs nothing.

[`lorenz_rk4.byn`](../examples/lorenz_rk4.byn) depends on both: `rk4_step(lorenz, t, state,
STEP)` runs 40,000 times, and each step calls `derivative` four times. An ODE integrator, a root
finder, a quadrature rule and a force-field kernel all have this shape. An indirect call in that
inner loop blocks inlining and vectorization, which is the cost the promise rules out.

Today the checker types a function name read as a value as `Callable[[...], R]`, and
`types/src/layout.rs` gives a `Callable` zero bytes. Lowering refuses the name ("names other than
variables, parameters and constants"), and the LLVM backend refuses `Callee::Value`. Building it
touches `types`, the checker, lowering, monomorphization and the driver's instance collection,
so this record settles the shape before code (`CONTRIBUTING.md` rule 7).

## Decision

A function passed as an argument is a compile-time argument, like a type argument. Each function
with `Callable` parameters is compiled once per combination of functions it receives, through
the same monomorphization that generics use (decision [0018](0018-mid-level-ir.md)).

### Each function has its own type

`types` gains `TypeKind::Function { item, callable }`: the type of one module-level function's
name read as a value, such as `lorenz`. `callable` is the function's signature as a `Callable`
type, so diagnostics can show it without reading the declaration. It takes no bytes, because the value is known when compiling. It is
compatible with a `Callable` whose parameter types, modes and return type match the function's
signature exactly, as Rust's function item types are. The checker reports a mismatch at the
argument with both signatures.

A local may hold one (`f = lorenz`), and passing `f` passes `lorenz`. Two branches that assign
different functions to one local give it two types, which is the existing "a variable keeps one
type" error.

### A `Callable` parameter is an implicit type parameter

Each `Callable` parameter of a function is an implicit type parameter of that function. Implicit
parameters come after the written ones, in parameter order. Its bound is the `Callable`
signature, and the checker checks the body against that signature, as it does today.

At a call, the checker appends one type argument per `Callable` parameter: the type of the
argument passed. That is `Function(lorenz)` for a function name, or the caller's own implicit
parameter when the caller passes on a `Callable` it received. The instance key stays the
function and its type arguments, so the driver's instance collection needs no new case.

### Monomorphization turns the call into a direct call

After substitution, the parameter's type is `Function(item)`. Monomorphization rewrites each
`Callee::Value(operand)` whose operand has that type into
`Callee::Function { item, type_arguments: [] }`. Every call in an instance is then direct, and
the backend's inliner sees `lorenz` called from `rk4_step` like any other function. The LLVM
backend never sees `Callee::Value` in a ready body, so it keeps refusing it as a check.

A parameter of `Function` type takes no bytes. The LLVM backend represents it as it represents
`None`, with no LLVM value, and passes nothing for it, as it already returns nothing for a
function that returns `None`.

### Not in version 0

Each of these is a checker error that names this record, never a miscompile:

- A closure, a lambda, a method or a generic function as the argument. Generic functions
  would need their type arguments inferred from the `Callable`'s signature, which can come later
  without changing this design.
- A `Callable` stored in a struct field, in a list or array, or returned from a function. These
  need a run-time function value with an indirect call, a separate feature with its own record.

## Alternatives considered

- **A function pointer and an indirect call.** It needs one copy of `rk4_step` and less compile
  time. Inlining then depends on LLVM propagating a constant pointer through the call, which it
  does only when it chooses to inline or clone the caller. The reference promises the call
  costs nothing, so a pointer is the wrong default. It remains the way to build the stored
  `Callable` values that version 0 leaves out.
- **Inlining the function body in lowering.** It breaks the rule that lowering builds one body
  per function, duplicates code at every call site rather than per function passed, and loses
  the call boundary that debug information and error locations rely on.
- **A new kind of instance key** holding a list of functions next to the type arguments. It
  works, but every pass that handles type arguments would need a second list. Treating the
  function as a type reuses substitution, instance collection and the existing layout rule.

## Consequences

- **What users get.** `rk4_step(lorenz, ...)` compiles to a loop with `lorenz` inlined, as fast
  as writing the derivative by hand. `lorenz_rk4.byn` builds once the built-in `round` is also
  lowered.
- **Code size and compile time.** One copy of the receiving function per distinct function
  passed, the same cost as one instance per type argument. A program that passes ten
  right-hand sides to one integrator gets ten integrators.
- **Error messages.** A type error shows `lorenz` as its signature,
  `Callable[[f64, Vec3], Vec3]`, never as an internal function type.
- **Interface changes.** `types` gains `TypeKind::Function` in its own pull request, with this
  record, as an interface crate must. `mir` needs no change: `Callee::Value` and
  `Callee::Function` already exist. The checker, lowering, monomorphization and the driver follow
  together.
