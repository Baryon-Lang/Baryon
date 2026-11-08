# 0028. `random.Philox` gives the same numbers as NumPy's Philox, and the compiler builds it in

Status: Accepted
Date: 2026-11-08
Roadmap: 1.9.12

## Context

The design plan picks counter-based generators for `random`, so that parallel streams are
reproducible from one seed ([language design plan](../design/language-design-plan.md), "Random
numbers"). The reference tells a `prange` loop to give each chunk its own stream,
`Philox(SEED, stream=chunk)` ([parallel loops](../reference/parallel-loops.md)), and
[`monte_carlo_pi.byn`](../examples/monte_carlo_pi.byn) does exactly that. Nothing said which
Philox variant, how a seed and a stream become its key and counter, or how a 64-bit output
becomes a float. Those choices fix every number a program draws, so changing them later breaks
every result a user has recorded.

The standard library (roadmap 1.13 and 1.14) does not exist yet, and there is no way to write a
generator in Baryon itself: Philox needs the high half of a 64-by-64-bit product, which the
language cannot express. So the compiler has to know the type until the library can define it.

## Decision

### The numbers match `numpy.random.Philox` bit for bit

Collins chose this on 2026-11-08, as he chose NumPy's pairwise `sum` (roadmap I.22): a NumPy run
can check every value, and 64-bit multiplies are cheap on the CPUs Baryon targets first.

- The generator is Philox4x64-10 (Salmon et al., "Parallel random numbers: as easy as 1, 2, 3",
  SC 2011), as NumPy's `Philox` bit generator and Random123 implement it.
- The key is the seed: its low 64 bits in the first word, the next 64 in the second. A seed is a
  Baryon `int`, so the second word is 0.
- The counter is four 64-bit words. It starts at `(0, 0, stream, 0)` and goes up by one, with
  carry, before each block, so the first block uses `(1, 0, stream, 0)`. Each block gives four
  64-bit outputs, used in order.
- `uniform()` takes the next output `u` and returns `(u >> 11) * 2**-53`, a float in `[0, 1)`,
  as NumPy's `Generator.random()` does.

So `Philox(seed, stream=k)` gives the numbers of NumPy's
`Generator(Philox(key=seed).jumped(k))`. Stream `k` starts `k * 2**128` blocks into the stream of
`seed`, so no two of the first `2**64` streams overlap within `2**128` blocks each.

NumPy's `Philox(seed)`, without `key=`, first hashes the seed through a `SeedSequence`, so its
numbers differ. Baryon uses the seed as the key, which needs no hashing and states the stream in
one line. The reference says so.

### Version 0 has one constructor and one method

- `Philox(seed: int, stream: int = 0)` makes a generator. A negative seed or stream panics,
  as NumPy rejects a negative key.
- `generator.uniform() -> f64` draws the next float in `[0, 1)`. It changes the generator, so
  `generator` must be a variable or an `InOut` parameter, as for any method that takes
  `InOut[Self]`.

More distributions (`normal`, integers in a range) and more generators come with the standard
library (roadmap 1.14), each matching NumPy the same way.

### The compiler builds the type in

- `types` gains `TypeKind::Standard(StandardType)`, with one variant today,
  `StandardType::Philox`. A type of the standard library that cannot yet be written in Baryon is
  a variant there, until roadmap 1.13 lets the library define it.
- A `Philox` is 88 bytes, stored inline: the key (2 words), the counter (4 words), the outputs of
  the current block (4 words) and how many of them are used (1 word). It owns no heap memory, so
  dropping it does nothing.
- A `Philox` moves; it is not a copy type. A copy would draw the same numbers as its original,
  which silently breaks the statistics of a simulation. `generator.copy()` can come later, if a
  program needs to replay a stream on purpose.
- `mir` gains `Builtin::Philox`, which makes a generator from a seed and a stream, and
  `Builtin::Uniform`, which draws from one passed `InOut`. The LLVM backend calls the runtime's
  `baryon_rt_philox_new` and `baryon_rt_philox_uniform`, written in Rust with `u128` products.

## Alternatives considered

- **Philox4x32-10**, the variant cuRAND and GPUs use natively. It needs only 32-bit multiplies,
  but no NumPy run could check it, and Baryon targets CPUs first. A GPU backend can compute the
  64-bit variant with 32-bit multiplies.
- **NumPy's `SeedSequence` seeding.** It would match `Philox(seed)` without `key=`, at the cost
  of reimplementing NumPy's hash. The key is simpler to state and to check.
- **The stream in the key's second word.** Philox is designed for independent keys, but NumPy's
  `jumped` moves the counter, and matching `jumped(k)` gives a Python user a one-line way to
  reproduce stream `k`.
- **A generator written in Baryon.** That needs 128-bit products or a way to call the runtime from
  library code, neither of which exists before roadmap 1.13.

## Consequences

- Every number a Baryon program draws can be checked against NumPy, and a `prange` loop that gives
  each chunk its own stream gives the same result on any number of cores.
- Each `uniform()` is a call into the runtime, which LLVM cannot inline across the static library.
  If a benchmark shows that call matters, the backend can write the rounds inline instead; the
  numbers stay the same.
- `TypeKind::Standard` is a place for each built-in library type until the standard library
  exists. Each one needs the checker, lowering and the backend to know it, so the list should stay
  short.
