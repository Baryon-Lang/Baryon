# Arrays and numbers

Numeric code in Baryon is built on `Array`, an n-dimensional array with NumPy's names and
NumPy's results, and on the libraries around it: `math`, `random`, `stats`, `linalg` and `fft`.
This page shows each in a program that builds and runs today, as CI checks. It assumes you have
read the [tour of the language](language-tour.md).

## 1. Arrays

`Array[f64, 2]` is a two-dimensional array of `f64`s: the element type and the number of
dimensions, called the *rank*, are part of the type, and the lengths are not. Arrays are stored
row by row, as NumPy and C store them.

File `arrays/main.byn`:

```python
def main():
    # Build arrays: zeros of a shape, or from a list.
    n = 5
    x = Array.zeros(n)
    for i in range(n):
        x[i] = f64(i) / f64(n - 1)
    weights = Array([1.0, 2.0, 3.0, 2.0, 1.0])

    # Whole-array arithmetic, as NumPy's, with no loop written.
    y = 2.0 * x + 1.0
    print(y)
    print((weights * y).sum() / weights.sum(), y.mean(), y.std())

    # Slices, masks and comparisons.
    print(y[1:4], y[::-1])
    print(y[y > 2.0])
    y[y > 2.5] = 2.5
    print(y)

    # A table: rank 2, with broadcasting and the matrix product.
    grid = Array.zeros((2, 3))
    grid[0, :] = 1.0
    grid += Array([0.0, 10.0, 20.0])
    print(grid)
    print(grid.sum(axis=0), grid.shape)
    print(grid @ Array([1.0, 1.0, 1.0]))

    # Sorting positions and distinct values.
    counts = Array([3, 1, 3, 2, 1])
    print(counts.argsort(), counts.unique(), counts.nunique())
```

```text
$ baryon run arrays
[1.  1.5 2.  2.5 3. ]
2.0 2.0 0.7071067811865476
[1.5 2.  2.5] [3.  2.5 2.  1.5 1. ]
[2.5 3. ]
[1.  1.5 2.  2.5 2.5]
[[ 1. 11. 21.]
 [ 0. 10. 20.]]
[ 1. 21. 41.] (2, 3)
[33. 30.]
[1 4 3 0 2] [3 1 2] 3
```

A NumPy user can read every line, and each prints what NumPy prints. The differences:

- **`Array.zeros(n)`** is `numpy.zeros(n)`: `Array.` takes the place of `numpy.`, and the element
  type is `f64` unless the context or a second argument says otherwise,
  `Array.zeros(n, i32)`.
- **A slice is a view**: `y[1:4]` reads `y`'s memory and copies nothing. It can be printed,
  passed to a function or used in arithmetic, but not yet kept in a name. A slice that runs past
  the end panics, where Python would shorten it. `y[mask]` and `y[positions]` make new arrays.
- **Every index is checked.** An index out of range panics with the index and the length, in
  debug and release builds alike; the compiler removes the check from loops where it can prove
  the index is in range.
- **`unique` keeps the order of first appearance**, as pandas's does, where NumPy's sorts.

`Fixed[f64, (3,)]` is the other array type: a small array whose length is part of its type, such
as a 3-vector, stored without an allocation. [Arrays](../reference/arrays.md) covers both, with
every method and what it costs.

## 2. Loops that run fast

A `for` loop over an array compiles to machine code, as in C or Fortran, so a loop is never
something to avoid. Two loop forms go further: `prange` splits a loop across every core, and
`simd` asks for the loop to be vectorized.

File `loops/main.byn`:

```python
def spring_energy(positions: Array[f64, 1], stiffness: f64) -> f64:
    """The energy of springs of rest length 1 between neighbours, summed on every core."""
    total = 0.0
    for i in prange(len(positions) - 1):
        stretch = positions[i + 1] - positions[i] - 1.0
        total += 0.5 * stiffness * stretch * stretch
    return total


def norm_squared(values: Array[f64, 1]) -> f64:
    """The sum of squares, vectorized, with the same bits on every machine."""
    total = 0.0
    for i in simd(range(len(values))):
        total += values[i] * values[i]
    return total


def main():
    n = 1000001
    positions = Array.zeros(n)
    for i in range(n):
        positions[i] = 1.01 * f64(i)
    print(f"{spring_energy(positions, 100.0):.3f}")
    print(norm_squared(Array([3.0, 4.0])))
```

```text
$ baryon run loops
5000.000
25.0
```

- **`prange`** takes the same arguments as `range`. The compiler sees that `total` is a sum and
  makes it a *reduction*: each core adds its own part, and the parts are combined in a fixed
  order, so the result is the same on every run and every machine. A loop where two iterations
  could write the same element is a compile error, so a parallel loop cannot race.
- **`simd`** adds a float sum in eight fixed lanes, so it too gives the same bits everywhere.
- **`--release`** turns on the optimizer: `baryon run --release loops` takes longer to build and
  runs up to ten times faster. Use it for anything you time.

[Parallel and vector loops](../reference/parallel-loops.md) has the rules, including how to add
into a shared array from a parallel loop with `accumulate=`.

## 3. `math`, exact sums and random numbers

File `numbers/main.byn`:

```python
from math import exp, f64x2, fsum, isnan, sqrt
from random import Philox


def main():
    # The math functions give IEEE 754 results, as NumPy's do: no exception for sqrt(-1).
    print(exp(1.0), isnan(sqrt(-1.0)), 1.0 / 0.0)

    # fsum adds without losing a digit; f64x2 carries about 32 significant digits.
    tiny = Array([1.0, 1e-16, 1e-16, 1e-16])
    print(tiny.sum(), fsum(tiny))
    print(f64x2(2.0, 0.0).sqrt().to_str())

    # Reproducible random numbers: a seed and a stream fix every number drawn.
    generator = Philox(42, stream=0)
    inside = 0
    trials = 100000
    for _ in range(trials):
        a = generator.uniform()
        b = generator.uniform()
        if a * a + b * b < 1.0:
            inside += 1
    print(f"pi is about {4.0 * f64(inside) / f64(trials):.3f}")
```

```text
$ baryon run numbers
2.718281828459045 True inf
1.0 1.0000000000000002
1.414213562373095048801688724210e+00
pi is about 3.152
```

- **Float division by zero is infinity**, and `sqrt(-1.0)` is NaN, as in NumPy and IEEE 754.
  Python raises an exception for both; numeric code expects the IEEE result.
- **`fsum`** is Python's `math.fsum`: the exact sum, rounded once. **`f64x2`** is a
  double-double float, for the few lines that need more digits than an `f64` has, such as the
  residual of an ill-conditioned solve.
- **`Philox`** draws NumPy's numbers bit for bit, and each stream is independent, so each
  `prange` chunk can have its own. The [Monte Carlo example](../examples/monte_carlo_pi.byn)
  does that.

[Math functions](../reference/arrays.md#math-functions) lists every function, and
[Random numbers](../reference/arrays.md#random-numbers) says which NumPy calls `Philox` matches.

## 4. Linear algebra, Fourier transforms and statistics

`linalg`, `fft` and `stats` keep NumPy's function names and give NumPy's results, written in
Baryon itself: no BLAS, LAPACK or C library is needed.

File `science/main.byn`:

```python
from math import cos, pi
import linalg
from linalg import LinAlgError
from fft import rfft, rfftfreq
from stats import median, percentile


def main() -> Raises[None, LinAlgError]:
    # Solve a linear system, as numpy.linalg.solve does.
    a = Array.zeros((3, 3))
    a[0, :] = Array([4.0, 1.0, 0.0])
    a[1, :] = Array([1.0, 4.0, 1.0])
    a[2, :] = Array([0.0, 1.0, 4.0])
    b = Array([1.0, 2.0, 3.0])
    print(linalg.solve(a, b))
    print(linalg.eigvalsh(a))

    # Find the frequency in a signal with a real FFT.
    n = 64
    samples = Array.zeros(n)
    for j in range(n):
        samples[j] = cos(2.0 * pi * 5.0 * f64(j) / f64(n))
    spectrum = rfft(samples)
    frequencies = rfftfreq(n, 1.0 / f64(n))
    peak = 0
    for k in range(len(spectrum)):
        if abs(spectrum[k]) > abs(spectrum[peak]):
            peak = k
    print("peak at", frequencies[peak], "cycles")

    # Order statistics, with NumPy's bits.
    data = Array([2.0, 9.0, 4.0, 7.0, 5.0])
    print(median(data), percentile(data, 90.0))
```

```text
$ baryon run science
[0.17857143 0.28571429 0.67857143]
[2.58578644 4.         5.41421356]
peak at 5.0 cycles
5.0 8.2
```

- **`linalg.solve` can raise `LinAlgError`** for a singular matrix, as NumPy's does, so `main`
  declares it.
- **`rfft`** gives complex numbers, `complex128`; `abs` of one is its magnitude.
- `linalg` also has `lu_factor`, `lu_solve`, `det`, `inv`, `cholesky`, `qr`, `eigh`, `eig`, `svd`
  and more; `fft` has `fft`, `ifft`, `irfft` and their two-dimensional forms; `stats` has
  `quantile`, `average`, `histogram`, `cov` and `corrcoef`.

[Linear algebra and FFT](../reference/linear-algebra.md) and [Statistics](../reference/statistics.md)
list every function, with its cost and where it differs from NumPy.

## Where to go next

[Data, files and plots](data-files-and-plots.md) reads and writes data and draws it. The
[example programs](../examples/README.md) put these pieces together: a heat equation, molecular
dynamics with `prange`, conjugate gradient, Runge-Kutta and Monte Carlo.
