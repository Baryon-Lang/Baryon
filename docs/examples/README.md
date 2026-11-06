# Example programs

Five small physics and mathematics programs written in the language as currently designed. They
are roadmap item 0.2: the way to test the design on real code before any compiler exists. No
compiler can run them yet.

The syntax follows decisions [0004](../decisions/0004-python-syntax.md) to
[0012](../decisions/0012-no-ffi-in-mvp.md). If a decision changes, these programs change with it.
Every file is valid Python 3.12 syntax, which `cargo xtask check-examples` verifies in CI.
`.npl` is a working extension until roadmap decision D.6 names the language.

| Program | Physics or mathematics | Language features it exercises |
| --- | --- | --- |
| [`heat_equation.npl`](heat_equation.npl) | 3D heat equation, explicit 7-point stencil | `Array[f64, 3]`, `InOut`, `prange`, swapping arrays by move |
| [`lennard_jones.npl`](lennard_jones.npl) | Lennard-Jones molecular dynamics, velocity Verlet | `Fixed` vectors, arrays of vectors, `Struct` state, disjoint field borrows, inferred reductions |
| [`conjugate_gradient.npl`](conjugate_gradient.npl) | Poisson's equation, sparse CSR matrix, conjugate gradient | `Struct` methods, `list` to `Array`, `Raises`, `try`/`except` |
| [`lorenz_rk4.npl`](lorenz_rk4.npl) | Lorenz system, fourth-order Runge-Kutta | generic functions with trait bounds, functions as arguments |
| [`monte_carlo_pi.npl`](monte_carlo_pi.npl) | Monte Carlo estimate of pi | reproducible parallel random streams, integer reductions, number mixing |

The design plan calls for 20 programs before the Phase 0 gate; these are the first five.

## Where the design got in the way

Writing these programs surfaced questions the language reference (roadmap 0.1) had to answer. The
[reference](../reference/README.md) settles most of them; the rest stay open where shown.

| Program | Finding | Where it stands |
| --- | --- | --- |
| `lennard_jones.npl` | Newton's third law writes `forces[i]` and `forces[j]` from one parallel iteration, which the disjoint-write rule rejects. The example computes each pair twice instead, doubling the work. | Open: idea I.6; proposed decision [0013](../decisions/0013-parallel-accumulation.md); see [what `prange` cannot express yet](../reference/parallel-loops.md#what-prange-cannot-express-yet) |
| `lennard_jones.npl` | `atoms.positions` and `atoms.forces` are passed to one call, one read-only and one `InOut`. The exclusivity check must accept disjoint fields of one struct, not just distinct variables. | Settled: [places and overlap](../reference/values-and-references.md#places-and-overlap) |
| `lennard_jones.npl` | `Array.zeros((N,), Vec3)` names the element type as an argument. The reference needs one way to give an array's element type at construction. | Settled: [creating arrays](../reference/arrays.md#creating-arrays) |
| `heat_equation.npl` | `u, out = out, u` swaps two arrays. The reference must define tuple assignment as a simultaneous move, so the swap copies nothing. | Settled: [tuple assignment](../reference/values-and-references.md#tuple-assignment) |
| `conjugate_gradient.npl` | `x` is assigned inside `try` and used only there. Using it after the `try` would need definite-assignment rules across `except` branches. | Settled: [definite assignment](../reference/types.md#scope-and-definite-assignment) |
| `conjugate_gradient.npl` | `direction = residual + beta * direction` allocates a new array every iteration unless the compiler reuses `direction`'s storage. | Open: roadmap 3.1; the meaning is fixed in [whole-array arithmetic](../reference/arrays.md#whole-array-arithmetic) |
| `conjugate_gradient.npl` | Errors subclass `Exception`, Python's spelling, but they are values with fields, not unwinding exceptions. | Settled: [declaring an error type](../reference/errors.md#declaring-an-error-type) |
| `lorenz_rk4.npl` | `rk4_step` needs a `VectorSpace` trait: addition, and scaling by a float. The standard library must define it, and `Fixed` and `Array` must implement it. | Settled for version 0: [traits](../reference/types.md#traits); user-defined traits wait on D.9 |
| `lorenz_rk4.npl` | `lorenz` takes `t` only to match the `Callable` signature. Unused-parameter warnings must allow that. | Open: roadmap 1.7, diagnostics |
| `monte_carlo_pi.npl` | `from random import Philox` shadows the name of Python's own `random` module. That is fine for the compiler but stops the program running under CPython with a shim. | Open: idea I.11; see [names that look like Python's modules](../reference/differences-from-python.md#names-that-look-like-pythons-modules) |
| All | Methods that mutate their struct have no spelling yet. `def push(self: InOut[Self], ...)` is the candidate. | Settled: [methods](../reference/values-and-references.md#methods) |
