# Parallel and vector loops

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

`prange` takes the same arguments as `range`. Its iterations may run in any order, on any number of
threads, and the result must not depend on that. The compiler checks that it cannot, with the rules
below.

The runtime splits the iterations into chunks and hands them to a thread pool sized to the
machine's cores, started on the first parallel loop. A program with no `prange` starts no threads.

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
   same way: `atoms.velocities[i] += dt * atoms.forces[i]` is allowed.
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
6. A `prange` inside a `prange` body is an error that suggests `range` for the inner loop. Nested
   parallelism would oversubscribe the cores for no gain on the loop shapes simulations use.

Anything else that writes a shared variable is a compile error naming the variable, the line, and
the rule. The most common one:

```text
error: iterations of this parallel loop may write the same element of `forces`
 --> md.npl:58:13
   |
51 |     for i in prange(N):
   |              --------- iterations run in parallel
...
58 |             forces[j] -= force
   |             ^^^^^^^^^ indexed by `j`, not by the loop variable `i`
   = note: each element of a shared array may be written only at the loop variable's index
   = help: compute each element's total in its own iteration, or use `range` for this loop
```

### Reproducible reductions

Floating-point addition is not associative, so a parallel sum normally changes with the number of
threads. Here it does not. The chunks of a `prange` depend only on the iteration count, never on
the thread count; each chunk reduces its iterations in order; and the chunk results combine in a
fixed tree. A program that reduces floats in parallel gives bit-identical results on every run, on
any number of cores.

The result can still differ from the serial loop's in the last bits, because the order of the
additions differs. Integer reductions are exact.

Reproducibility is the design plan's default because a simulation whose result changes with the
machine cannot be debugged or checked against a previous run. Its cost is that chunk sizes cannot
adapt to the thread count, which the benchmark suite (roadmap 0.3) measures.

### What `prange` cannot express yet

A loop that adds to several elements from one iteration, such as applying Newton's third law in a
pair-force loop (`forces[i] += f` and `forces[j] -= f`), breaks rule 2. Version 0 has no safe
parallel form of it: compute each element's total in its own iteration, as the
[Lennard-Jones example](../examples/lennard_jones.npl) does, at twice the arithmetic.

Roadmap idea I.6 tracks the fix. Decision 0013, proposed in
[pull request #8](https://github.com/csimpson1964/New-Programming-Language/pull/8), would allow it
by declaring such arrays add-only: `prange(n, accumulate=forces)`.

### Random numbers

A shared random number generator would be a shared variable written by every iteration, which rule
3 rejects. Use a counter-based generator with one stream per iteration or chunk instead, as in the
[Monte Carlo example](../examples/monte_carlo_pi.npl): `Philox(SEED, stream=chunk)`. The results are
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

`simd` changes speed, never results: a loop that vectorizes computes what the plain loop computes,
except for the order of operations in a reduction, which follows the same fixed-order rule as
`prange`.
