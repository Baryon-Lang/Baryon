# Example programs

Eight small physics and mathematics programs written in the language as currently designed. They
are roadmap item 0.2: the way to test the design on real code before any compiler exists. The
compiler now builds and runs all eight, and a test checks what each prints;
[What builds today](../reference/status.md#the-example-programs) says so. `baryon run` runs a
program from a directory holding it as `main.byn`.

The syntax follows decisions [0004](../decisions/0004-python-syntax.md) to
[0012](../decisions/0012-no-ffi-in-mvp.md). If a decision changes, these programs change with it.
Every file is valid Python 3.12 syntax, which `cargo xtask check-examples` verifies in CI.
Baryon source files end in `.byn` ([decision 0023](../decisions/0023-language-name-baryon.md)).

| Program | Physics or mathematics | Language features it exercises |
| --- | --- | --- |
| [`heat_equation.byn`](heat_equation.byn) | 3D heat equation, explicit 7-point stencil | `Array[f64, 3]`, `InOut`, `prange`, swapping arrays by move |
| [`lennard_jones.byn`](lennard_jones.byn) | Lennard-Jones molecular dynamics, velocity Verlet | `Fixed` vectors, arrays of vectors, `Struct` state, disjoint field borrows, inferred reductions, `accumulate=` |
| [`conjugate_gradient.byn`](conjugate_gradient.byn) | Poisson's equation, sparse CSR matrix, conjugate gradient | `Struct` methods, `list` to `Array`, `Raises`, `try`/`except` |
| [`lorenz_rk4.byn`](lorenz_rk4.byn) | Lorenz system, fourth-order Runge-Kutta | generic functions with trait bounds, functions as arguments |
| [`monte_carlo_pi.byn`](monte_carlo_pi.byn) | Monte Carlo estimate of pi | reproducible parallel random streams, integer reductions, number mixing |
| [`rod_boundaries.byn`](rod_boundaries.byn) | 1D heat equation with a different boundary condition at each end | a union of structs as a sum type, class patterns in `match` ([decision 0032](../decisions/0032-sum-types-are-unions-of-structs.md)) |
| [`energy_audit.byn`](energy_audit.byn) | Energy drift of a harmonic oscillator under leapfrog steps | a context manager, a class with `__enter__` and `__exit__` in a `with` block, and an iterator, a class with `__next__` in a `for` loop ([decision 0053](../decisions/0053-with-and-for-on-user-classes.md)) |
| [`flight_log.byn`](flight_log.byn) | Flight of a ball thrown upward, with and without air drag | a class with `__del__`, which reports a log when its owner drops it ([decision 0063](../decisions/0063-drop-hooks.md)) |

The design plan calls for 20 programs before the Phase 0 gate; these are the first eight.

## Where the design got in the way

Writing these programs surfaced questions the language reference (roadmap 0.1) had to answer. The
[reference](../reference/README.md) settles most of them; the rest stay open where shown.

| Program | Finding | Where it stands |
| --- | --- | --- |
| `lennard_jones.byn` | Newton's third law writes `forces[i]` and `forces[j]` from one parallel iteration, which the disjoint-write rule rejects. The example computes each pair twice instead, doubling the work. | Decided: [0013](../decisions/0013-parallel-accumulation.md); see [adding into shared arrays](../reference/parallel-loops.md#adding-into-shared-arrays). Built: the example computes each pair once in `prange(N, accumulate=forces)` (roadmap 1.14.1) |
| `lennard_jones.byn` | Built and run, the first version never moved: a perfect lattice at rest is in balance, so every force cancels. | Fixed: each atom starts a small fixed displacement off its site, from a formula. A seeded random start waits on the `random` module (roadmap 1.14) |
| `lennard_jones.byn` | `atoms.positions` and `atoms.forces` are passed to one call, one read-only and one `InOut`. The exclusivity check must accept disjoint fields of one struct, not just distinct variables. | Settled: [places and overlap](../reference/values-and-references.md#places-and-overlap) |
| `lennard_jones.byn` | `Array.zeros((N,), Vec3)` names the element type as an argument. The reference needs one way to give an array's element type at construction. | Settled: [creating arrays](../reference/arrays.md#creating-arrays) |
| `heat_equation.byn` | `u, out = out, u` swaps two arrays. The reference must define tuple assignment as a simultaneous move, so the swap copies nothing. | Settled: [tuple assignment](../reference/values-and-references.md#tuple-assignment) |
| `conjugate_gradient.byn` | `x` is assigned inside `try` and used only there. Using it after the `try` would need definite-assignment rules across `except` branches. | Settled: [definite assignment](../reference/types.md#scope-and-definite-assignment) |
| `conjugate_gradient.byn` | `direction = residual + beta * direction` allocates a new array every iteration unless the compiler reuses `direction`'s storage. | Open: roadmap 3.1; the meaning is fixed in [whole-array arithmetic](../reference/arrays.md#whole-array-arithmetic) |
| `conjugate_gradient.byn` | Errors subclass `Exception`, Python's spelling, but they are values with fields, not unwinding exceptions. | Settled: [declaring an error type](../reference/errors.md#declaring-an-error-type) |
| `lorenz_rk4.byn` | `rk4_step` needs a `VectorSpace` trait: addition, and scaling by a float. The standard library must define it, and `Fixed` and `Array` must implement it. | Settled for version 0: [traits](../reference/types.md#traits); user-defined traits wait on D.9 |
| `lorenz_rk4.byn` | `lorenz` takes `t` only to match the `Callable` signature. Unused-parameter warnings must allow that. | Open: roadmap 1.7, diagnostics |
| `monte_carlo_pi.byn` | `from random import Philox` shadows the name of Python's own `random` module. That is fine for the compiler but stops the program running under CPython with a shim. | Open: idea I.11; see [names that look like Python's modules](../reference/differences-from-python.md#names-that-look-like-pythons-modules) |
| All | Methods that mutate their struct have no spelling yet. `def push(self: InOut[Self], ...)` is the candidate. | Settled: [methods](../reference/values-and-references.md#methods) |
