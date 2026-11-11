# Parallel and vector loops

[What builds today](status.md#parallel-and-vector-loops) says which parts of this chapter the
compiler builds into programs yet.

Two loop forms ask for more speed than a plain `for`: `prange` runs iterations on several cores,
and `simd` runs them several at a time on one core. Both are checked: a loop the compiler cannot
prove safe is a compile error, never a wrong answer.

## `prange`

```python
for i in prange(1, N - 1):
    for j in range(1, N - 1):
        for k in range(1, N - 1):
            out[i, j, k] = u[i, j, k] + ALPHA * (laplacian(u, i, j, k))
```

`prange` takes the same arguments as `range`, plus an optional `accumulate=` described in
[adding into shared arrays](#adding-into-shared-arrays). Its iterations may run in any order, on
any number of threads, and the result must not depend on that. The compiler checks that it cannot,
with the rules below.

The runtime splits the iterations into chunks and hands them to a thread pool sized to the
machine's cores, started on the first parallel loop. A program with no `prange` starts no threads.

`prange` drives only a `for` statement, never a comprehension. To build a list or array in
parallel, make it first and fill it by index:

```python
def squares(n: i64) -> Array[f64, 1]:
    out = Array.zeros(n)
    for i in prange(n):
        out[i] = f64(i * i)
    return out
```

`[i * i for i in prange(n)]` is a compile error that suggests this form, so every parallel loop
is a statement the rules below check. `simd(range(n))` may drive a comprehension; it runs as the
plain `range`.

### What a `prange` body may do

A variable assigned inside the body is **private**: each iteration has its own. A variable assigned
before the loop and used inside it is **shared**. The rules concern shared variables.

1. **Reading** a shared variable is always allowed, as long as no rule below makes it a reduction.
2. **Writing an element of a shared array** is allowed only when every access to that array in the
   body, read or write, indexes one fixed dimension with exactly the loop variable, unchanged:

   ```python
   for i in prange(n):
       forces[i] = total_force(positions, i)  # allowed: forces is accessed only at [i]
   ```

   ```python
   for i in prange(1, n):
       a[i] = a[i - 1] + 1.0  # error: reads a[i - 1], which another iteration writes
   ```

   This guarantees each element is written by one iteration and read by no other. Fields work the
   same way: `atoms.velocities[i] += dt * atoms.forces[i]` is allowed. An array that iterations
   only add into, at any index, is the exception: list it in `accumulate=`, as
   [adding into shared arrays](#adding-into-shared-arrays) explains.
3. **Writing a shared scalar** is allowed only as a **reduction**: every write in the body has one
   form, `x += e` and `x -= e` (which may be mixed), `x *= e`, `x = min(x, e)` or
   `x = max(x, e)`, and the body reads `x` nowhere else. Each thread then keeps a private `x`,
   starting from the operation's identity, and the partial results are combined after the loop.

   ```python
   energy = 0.0
   for i in prange(n):
       energy += 0.5 * velocities[i].dot(velocities[i])  # a + reduction, inferred
   ```

4. **Passing a shared place as `InOut`** to a function counts as writing it, under rule 2 or 3.
5. **Leaving the loop early** is not allowed: no `break`, `return` or `raise` in the body.
   `continue` is allowed. A call that can fail must be handled by a `try` inside the body.
   Neither may the body end the program: `sys.exit` in it is a compile error, and one reached
   through a function the body calls panics, since other iterations may still be writing their
   files. Count failures with a reduction, `failed += 1`, and call `sys.exit` after the loop.
6. A `prange` inside a `prange` body is an error that suggests `range` for the inner loop. Nested
   parallelism would oversubscribe the cores for no gain on the loop shapes simulations use.

Anything else that writes a shared variable is a compile error naming the variable, the line, and
the rule. The most common one:

```text
error: iterations of this parallel loop may write the same element of `forces`
  --> md.byn:58:13
   |
58 |             forces[j] -= force
   |             ^^^^^^^^^ indexed by `j`, not by the loop variable `i`
  ::: md.byn:51:14
   |
51 |     for i in prange(N):
   |              --------- iterations run in parallel
   = note: each element of a shared array may be written only at the loop variable's index
   = note: if every write to `forces` in this loop adds to it, declare it add-only: `prange(N, accumulate=forces)`
   = note: or compute each element's total in its own iteration, or use `range` for this loop
```

A variable assigned before the loop counts as shared even when the body assigns it too, so
`last = a[i]` with `last` assigned earlier is an error under rule 3. Give a value that belongs to
one iteration a name first assigned inside the loop.

### Reproducible reductions

Floating-point addition is not associative, so a parallel sum normally changes with the number of
threads. Here it does not. The chunks of a `prange` depend only on the iteration count, never on
the thread count; each chunk reduces its iterations in order; and the chunk results combine in a
fixed tree. A program that reduces floats in parallel gives bit-identical results on every run, on
any number of cores.

The chunks and the tree are exact. A loop of `n` iterations has `c = min(n, 1024)` chunks of
consecutive iterations, as even as integers allow: each has `n // c` iterations, and the first
`n % c` have one more. Each chunk starts its copy of the
reduced variable at the operator's identity (`-0.0` or `0` for a sum, `1` for a product, the
type's largest value or `inf` for `min`, its smallest or `-inf` for `max`) and combines its
iterations into it in order, giving partials `p[0]` to `p[c-1]`. The partials then combine
pairwise:

```text
stride = 1
while stride < c:
    for i in 0, 2*stride, 4*stride, ... while i + stride < c:
        p[i] = p[i] ⊕ p[i + stride]
    stride = stride * 2
x = x ⊕ p[0]
```

The variable's value from before the loop comes in last. With 1024 chunks the tree is 10 levels
deep, so the rounding error of a float sum grows with 10 additions, not 1023.

The result can still differ from the serial loop's in the last bits, because the order of the
additions differs. Integer reductions are exact. In a debug build one that overflows panics,
whether the overflow happens inside a chunk or when the partials combine; a release build wraps
both, which gives the same bits on any number of threads
([Integer overflow](types.md#integer-overflow)).

Reproducibility is the design plan's default because a simulation whose result changes with the
machine cannot be debugged or checked against a previous run. Its cost is that chunk sizes cannot
adapt to the thread count, which the benchmark suite (roadmap 0.3) measures.

### Adding into shared arrays

Some loops add into elements that other iterations add into too. The pair-force loop of molecular
dynamics is the common case. By Newton's third law, the force atom `j` exerts on atom `i` is minus
the force `i` exerts on `j`, so computing each pair once and applying it to both atoms halves the
work. Iteration `i` then writes `forces[j]`, which breaks rule 2. Histograms
(`counts[bin] += 1`), charge deposition in particle-in-cell codes and finite-element assembly have
the same shape.

Name such arrays in `accumulate=`
([decision 0013](../decisions/0013-parallel-accumulation.md)):

```python
def compute_forces(positions: Array[Vec3, 1], forces: InOut[Array[Vec3, 1]]) -> f64:
    """Each pair is computed once and applied to both atoms."""
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

Several arrays go in a tuple: `prange(N, accumulate=(forces, virial))`. Inside the loop, an array
listed in `accumulate=`:

- is only added to. It appears only as the target of `+=` or `-=`, at any index or slice that `+=`
  accepts outside a loop, and indices are bounds-checked as everywhere else.
- appears nowhere else in the body. It is never read, assigned with `=` or passed to a function. A
  helper function cannot add into it on the loop's behalf: the helper returns its contribution,
  and the loop adds it.
- holds elements that can be added and have a zero: integers, floats, complex numbers, and `Fixed`
  arrays of them.

Listing an array the body never adds to is an error, so the argument cannot go stale. Only arrays
written at an index other than the loop variable need it: writes at the loop variable still follow
rule 2 with no annotation, and scalar reductions are still inferred. Only addition is supported;
`*=`, `min` and `max` into arrays wait for a program that needs them.

After the loop, the array is an ordinary array again, holding what it held before the loop plus
everything the loop added, as the same loop run serially would. That is why the example zeroes
`forces` first, in a loop of its own.

**Result.** Accumulated arrays follow the same contract as scalar reductions: each chunk adds its
iterations in order, and the chunk results combine in the fixed tree described in
[reproducible reductions](#reproducible-reductions). Float results are bit-identical on every run
and any number of cores, and may differ from the serial loop's in the last bits. Integer results
are exact.

**Cost.** The runtime gives each chunk private copies of only the blocks of the array it adds
into, made when it first touches them, and combines two chunks' copies as soon as both are done,
summing only the blocks they touched. For spatially sorted particles the extra memory is about one
more copy of the array. The worst case needs every chunk to touch every block while one chunk
stalls and the later ones finish: `threads + (threads + 1) × ceil(log2(chunks))` copies, 98 on 8
threads with 1024 chunks ([decision 0024](../decisions/0024-compiling-accumulated-arrays.md)).
That cost is why an array must be named while a scalar, whose private copy costs a few bytes, is
inferred: privatizing a whole array should be visible in the source.

The alternative that needs no `accumulate=` and no extra memory is to compute every pair twice, so
iteration `i` writes only `forces[i]`. That is what the
[Lennard-Jones example](../examples/lennard_jones.byn) does today, at twice the arithmetic.
Molecular dynamics codes such as LAMMPS find the add-into form faster on CPUs; a benchmark
decides for a given kernel.

### Random numbers

A shared random number generator would be a shared variable written by every iteration, which rule
3 rejects. Use a counter-based generator with one stream per iteration or chunk instead, as in the
[Monte Carlo example](../examples/monte_carlo_pi.byn): `Philox(SEED, stream=chunk)`. The results are
then reproducible from one seed, whatever the thread count.

## `simd`

```python
for i in simd(range(n)):
    y[i] = a * x[i] + y[i]
```

`simd(range(...))` asks the compiler to vectorize the loop: to run several iterations at once with
the processor's vector instructions ([decision 0011](../decisions/0011-checked-simd.md)). If it
cannot, compilation fails and says why: the dependence between iterations it found, the operation
that has no vector form, or the call it could not inline.

The reference rules already prove that arrays never alias, so most simple loops vectorize without
`simd`. Write `simd` where vectorization matters enough that a silent failure to vectorize should be
an error; the error is also the quickest way to learn why a loop is slow.

### What a `simd` body may do

A `simd` body follows [the rules of a `prange` body](#what-a-prange-body-may-do), with the loop
variable of the `simd` loop in place of the `prange` one: shared arrays are written only at the
loop variable's index, shared scalars only as reductions, and the body never ends early. Running
several iterations at once is safe exactly when running them in any order is, so one set of rules
covers both.

```python
for i in prange(n):
    for j in simd(range(n)):
        grid[i, j] = grid[i, j] * factor  # both loops' rules hold: [i] for prange, [j] for simd
```

- `simd` takes exactly one `range(...)`: `simd(range(n))`, `simd(range(1, n - 1))`.
- A `simd` loop may sit inside a `prange` body, the usual shape: threads take the rows and each
  row is vectorized. Then both loops' rules apply to its body.
- A `simd` body holds no `prange` and no other `simd`: only one loop is vectorized.

For now these rules are the whole check. The compiler asks LLVM to vectorize every loop that
passes them; reporting a loop LLVM still declines to vectorize, with the reason mapped back to the
source, comes with roadmap item 3.7. The rules stay as they are then: a loop they reject would not
vectorize safely anyway.

`simd` never makes results depend on the machine: a float sum or product gives the same bits
everywhere, vectorized or not, though its grouping differs from a loop without `simd`. Every other
loop computes exactly what the plain loop computes. The grouping of a sum's additions is fixed by
the language, not by the processor's vector width. The sum splits into 8 lanes, each
starting at the identity (`-0.0` for a sum, `1.0` for a product); iteration `i`, counted from 0,
adds into lane `i % 8`; and the lanes combine in a fixed pairwise tree. The variable's value from
before the loop comes in last, as it does after the partials of a
[`prange` reduction](#reproducible-reductions):

```text
x = x ⊕ (((l0 ⊕ l1) ⊕ (l2 ⊕ l3)) ⊕ ((l4 ⊕ l5) ⊕ (l6 ⊕ l7)))
```

The result can differ from the plain loop's in the last bits, because the order of the additions
differs, but it is the same on every machine, vectorized or not. Eight independent sums also
remove the wait for each addition to finish before the next can start, which is most of the
speed-up. Integer reductions, `min` and `max` give the same result in any order and are not split.
A `simd` loop that sums into a complex variable runs unvectorized, in the plain loop's order,
until complex sums get lanes of their own (roadmap idea I.37).
