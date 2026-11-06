# 0013. Parallel loops may add into shared arrays they declare with `accumulate=`

Status: Proposed
Date: 2026-11-06
Roadmap: I.6, D.12

## Context

A `prange` loop is safe because the compiler checks that no two iterations write the same memory.
The rule today: an array from outside the loop may be written only at the loop variable, so
iteration `i` owns element `i`, and a scalar written in the body must be a reduction such as
`energy += e`.

Pairwise physics breaks that rule. In Lennard-Jones molecular dynamics, Newton's third law says
the force atom `j` exerts on atom `i` is minus the force `i` exerts on `j`. Computing each pair
once and writing both `forces[i]` and `forces[j]` halves the pair work, but iteration `i` then
writes element `j`, which iteration `j` also writes. The checker rejects it, correctly: unguarded,
it is a data race that silently loses forces.

The current workaround, in [`lennard_jones.npl`](../examples/lennard_jones.npl), computes every
pair twice so each iteration writes only its own atom. That is the "full neighbour list" in
molecular dynamics terms. It is safe and simple, and it does twice the arithmetic. The same shape
appears well beyond molecular dynamics: histograms (`counts[bin] += 1`), particle-in-cell charge
deposition, finite-element assembly, and the transpose of a sparse matrix-vector product. Every
one is an *array-valued reduction*: many iterations add into elements of one array, and the
order of the additions does not matter except for floating-point rounding.

### How other languages and codes handle it

| System | Mechanism | Safe? | Cost |
| --- | --- | --- | --- |
| OpenMP 4.5+ (C, C++, Fortran) | `reduction(+: f[:n])`: each thread gets a private copy of the array, summed at the end | Yes, if the clause is written; nothing checks a forgotten one | A full array copy per thread |
| OpenMP | `#pragma omp atomic` on each update | Yes | Contended atomics; on Apple silicon a 64-bit float atomic add is a compare-and-swap loop. Result order varies run to run |
| Fortran 2023 | `do concurrent (...) reduce(+: f)`; arrays allowed, combined elementwise ([J3 19-255r2](https://j3-fortran.org/doc/year/19/19-255r2.txt)) | The programmer asserts it; the compiler does not verify | Implementation-defined, in practice a private copy per thread |
| Rust, rayon | `fold` a per-thread `Vec`, then `reduce` the vectors; or atomics through bit-casting, since Rust has no atomic floats | Yes, enforced by the borrow checker | A full copy per thread, written as combinator code a physicist would not recognise |
| Chapel | Reduce intents, `forall ... with (+ reduce x)` ([docs](https://chapel-lang.org/docs/technotes/reduceIntents.html)) | Yes | Documented for scalars; whole-array use is not the common path |
| Halide | Scatter in an update definition; parallelize with `rfactor` (private copies) or the `.atomic()` schedule | Yes; the schedule never changes the result beyond rounding | Chosen by the schedule, separate from the algorithm |
| Julia | `Threads.@threads` checks nothing; the idioms are per-task buffers (OhMyThreads, ChunkSplitters) or `Atomix.@atomic` | No | Whatever the programmer builds |
| LAMMPS, OPENMP package | Half neighbour list with `newton on`; each thread writes a private force array, reduced after the loop | Yes | A force array per thread |
| LAMMPS, KOKKOS package | `ScatterView`: duplicated arrays on CPUs, atomics on GPUs. Their guidance: half list fastest on CPUs; full list fastest on GPUs for simple pair potentials, because doubled arithmetic beats atomics ([Sandia slides](https://www.osti.gov/servlets/purl/2431489)) | Yes | Picked per back end, behind one interface |
| GROMACS | Per-thread force buffers split into blocks; a bitmask records which blocks each thread touched, and only those are summed ([threaded force buffer](https://manual.gromacs.org/documentation/current/doxygen/html-full/threaded__force__buffer_8cpp.xhtml)) | Yes | About one array of extra memory when atoms are spatially sorted, not one per thread |
| Graph colouring | Split the cells into colours whose atoms never interact, run one colour at a time | Yes | Restructures the algorithm, cuts parallelism, and is hard to load-balance |

Three lessons come out of this:

1. **The programmer states intent; the implementation picks the strategy.** Kokkos `ScatterView`
   and Halide's schedules separate "add these into that array" from how it is made safe. That
   separation is what lets one source run well on CPUs today and GPUs later.
2. **Write-only additive access is the property that makes it safe.** Any strategy, private
   copies, blocks or atomics, works only if the loop never reads the array or assigns to it.
3. **Per-thread full copies do not scale to huge data.** A billion atoms times 24 bytes times
   64 threads is 1.5 TB. Blocked buffers that track touched blocks, as GROMACS does, keep the
   extra memory near one array for spatially sorted data.

## Decision

A `prange` loop names the arrays it adds into with an `accumulate=` argument. Inside the loop,
such an array may only be added to.

```python
def compute_forces(positions: Array[Vec3, 1], forces: InOut[Array[Vec3, 1]]) -> f64:
    """Each pair is computed once and applied to both atoms (Newton's third law)."""
    for i in prange(N):
        forces[i] = Vec3.zeros()
    energy = 0.0
    for i in prange(N, accumulate=forces):
        for j in range(i + 1, N):
            delta = positions[i] - positions[j]
            delta -= BOX * (delta / BOX).round()
            distance_squared = delta.dot(delta)
            if distance_squared < CUTOFF * CUTOFF:
                inverse_6 = 1.0 / distance_squared**3
                force = delta * (24.0 * inverse_6 * (2.0 * inverse_6 - 1.0) / distance_squared)
                forces[i] += force
                forces[j] -= force
                energy += 4.0 * inverse_6 * (inverse_6 - 1.0)
    return energy
```

Several arrays are listed as a tuple: `prange(N, accumulate=(forces, virial))`.

### The rules the checker enforces

Inside the loop body, an array listed in `accumulate=`:

- appears only as the target of `+=` or `-=`, at any index or slice that `+=` accepts outside a
  loop. Indices are bounds-checked as everywhere else (decision
  [0010](0010-bounds-checks.md));
- is never read, assigned with `=`, passed to a function, viewed, or resized;
- has an element type with addition and a zero: integers, floats, complex numbers and `Fixed`
  arrays of them.

Outside the loop the array is an ordinary array again. Its contents before the loop are kept: the
loop adds to them, exactly as the same loop run serially would. Listing an array the body never
adds to is an error, so the argument cannot go stale.

This extends the disjoint-write rule of the language reference rather than replacing it. Writes
at the loop variable still need no annotation, and scalar reductions are still inferred. Only an
array written at an index other than the loop variable needs `accumulate=`, and the checker's
error says so:

```text
error: `forces[j]` may be written by more than one iteration of this `prange` loop
  --> lennard_jones.npl:59:17
   |
59 |                 forces[j] -= force
   |                 ^^^^^^^^^ written at `j`, not at the loop variable `i`
   |
help: if every write to `forces` in this loop adds to it, list it as accumulated:
   |
50 |     for i in prange(N, accumulate=forces):
   |                      +++++++++++++++++++
```

### What the result is

- **Meaning.** The array ends up equal to the serial loop's result, except that floating-point
  additions may be grouped differently. Integer accumulation is exact.
- **Reproducibility.** Accumulated arrays follow the same reproducibility contract as scalar
  reductions in the language reference: the result is bitwise identical from run to run and
  independent of the thread count, because contributions are combined per fixed chunk of
  iterations in a fixed order. As for scalar reductions, there is no opt-out. A GPU back end
  that wants unordered atomics (Phase 4) needs its own decision record, which would add an
  opt-out for scalar and array reductions together.
- **Strategy.** How the runtime makes it safe is an implementation choice behind the
  `prange` runtime interface, not language semantics. The Phase 1 strategy is GROMACS-style
  blocked private buffers: a chunk allocates a private copy of a block of the array only when
  it first adds into that block, and the combine step sums only touched blocks, in chunk order.
  Colouring stays available as a future strategy that passes the same tests. Atomics do not,
  because they make the order of additions vary, so they wait for the GPU record above.
- **Cost, documented with the feature.** Extra memory is the touched blocks of the chunks in
  flight. For spatially sorted particles that is about one extra copy of the array; the worst
  case, every chunk touching every block, is one copy per chunk in flight. The language
  reference states this, and the compiler's optimization remarks (roadmap 3.7) report it.

### What this decision does not do

- Only addition. `*=`, `min` and `max` into arrays wait for a program that needs them.
- No `Accumulate[...]` parameter type, so a helper function cannot add into an accumulated array
  on the loop's behalf. A helper returns the contribution and the loop adds it. A parameter type
  is the natural extension if real programs need it.
- No user-facing atomics in `prange` bodies. The standard library's atomics (design plan) remain
  for the rare case this does not cover.

## Consequences

- Lennard-Jones, histograms, charge deposition and finite-element assembly are written the way a
  physicist writes them serially, plus one argument, and stay race-free by construction.
- Pairwise kernels can halve their arithmetic on CPUs, where half neighbour lists are fastest
  (LAMMPS guidance). The full-list form stays legal and needs no annotation, and is the better
  choice on GPUs, so the programmer keeps both options and a benchmark decides.
- The cost of privatizing a large array is visible in the source as a named argument, never
  inferred silently. This is why arrays need `accumulate=` while scalars, whose private copies
  cost a few bytes per chunk, stay inferred.
- The CPython shim (roadmap I.11) treats `prange(n, accumulate=...)` as `range(n)`, which is the
  serial meaning, so the oracle still works.
- The runtime (roadmap 1.12) and the array library (1.14) gain blocked buffers and a combine
  pass; the exclusivity checker (1.8) gains the write-only rule. These are the implementation
  items to plan once this record is accepted.
- The performance claim needs evidence before Phase 3: the baseline suite (0.3) should gain a
  half-list, threaded Lennard-Jones variant in C++ and Fortran with OpenMP array reductions, so
  the compiled code has a parallel target to match.
- Reproducibility independent of thread count costs some speed against an unordered atomic
  scheme. The default favours correctness and debuggable results, the same trade the language
  reference makes for scalar reductions.
