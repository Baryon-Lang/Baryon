# Statistics

The mean, variance and standard deviation are array methods, `a.mean()`, `a.var()` and
`a.std()`, described in [Means and variances](arrays.md#means-and-variances). The `stats` module
holds the rest of NumPy's statistics functions, with NumPy's names and results
([decision 0042](../decisions/0042-statistics.md)):

| Function | Result | NumPy |
| --- | --- | --- |
| `median(a)` | The middle value of `a` sorted, or the mean of the two middle values | `numpy.median(a)` |
| `quantile(a, q)` | The value below which a fraction `q` of `a` lies, `q` from 0 to 1 | `numpy.quantile(a, q)` |
| `percentile(a, q)` | The same with `q` in percent, from 0 to 100 | `numpy.percentile(a, q)` |
| `average(a, weights)` | The mean of `a` weighted by `weights` | `numpy.average(a, weights=weights)` |
| `histogram(a, bins)` | The counts of `a` in `bins` equal bins, and the bins' edges | `numpy.histogram(a, bins)` |
| `cov(x, y)` | The 2 by 2 covariance matrix of `x` and `y` | `numpy.cov(x, y)` |
| `corrcoef(x, y)` | The 2 by 2 matrix of correlation coefficients of `x` and `y` | `numpy.corrcoef(x, y)` |

Each takes rank-1 `Array[f64, 1]`s, and each gives the bits NumPy gives for the same arrays: it
rounds in the same places, not just to the same precision, so a test can compare a result with
NumPy's using `==`. The one exception is the last bit of `cov` and `corrcoef`, where NumPy's
order depends on the machine; see [`cov` and `corrcoef`](#cov-and-corrcoef).

```python
from stats import median, percentile


def summary(times: Array[f64, 1]):
    """The middle and the slow tail of a set of run times, in seconds."""
    print(f"median {median(times):.3f} s, 95th percentile {percentile(times, 95.0):.3f} s")
```

## `median`

`median(a)` sorts nothing: it copies `a` once and moves the middle element into place, an
expected 3n comparisons, so it costs about as much as two passes over the array. `a` itself is
unchanged. For an even length the result is `(x + y) / 2` of the two middle values, NumPy's
formula, so the median of `[1.0, 2.0, 3.0, 4.0]` is `2.5`.

A NaN anywhere in `a` makes the median NaN, as in NumPy, since a NaN has no place in the sorted
order. An empty array's median is NaN too; NumPy also warns.

## `quantile` and `percentile`

`quantile(a, q)` uses NumPy's default method, "linear": it takes the element at the fractional
position `p = (n - 1) * q` of `a` sorted, and interpolates between the elements at `floor(p)` and
`floor(p) + 1`. With `t` the fraction of `p` past `floor(p)`, and `x` and `y` those two elements,
the result is `x + (y - x) * t` when `t < 0.5` and `y - (y - x) * (1 - t)` otherwise, the two
formulas NumPy uses so that the result never strays outside `[x, y]`. `quantile(a, 0.0)` is the
smallest element and `quantile(a, 1.0)` the largest.

`percentile(a, q)` is `quantile(a, q / 100.0)`, with `q / 100.0` rounded first as NumPy rounds
it, so `percentile(a, 95.0)` has NumPy's bits.

A NaN anywhere in `a` makes the result NaN. A `q` outside its range, or an empty `a`, panics,
where NumPy raises an exception: both are mistakes in the program rather than in its data, as
`a.min()` of an empty array is.

```text
panic: assertion failed: q >= 0.0 and q <= 100.0, "percentile needs q from 0 to 100"
```

## `average`

`average(a, weights)` is `(a * weights).sum() / weights.sum()`, each sum added in NumPy's pairwise
order, so it has the bits of `numpy.average(a, weights=weights)`. Neither sum stores
`a * weights`. The arrays must have the same length, and the weights must not sum to zero; either
mistake panics. For the plain mean, use `a.mean()`.

```python
from stats import average


def center_of_mass(x: Array[f64, 1], m: Array[f64, 1]) -> f64:
    """The center of mass of point masses `m` at positions `x`."""
    return average(x, m)
```

## `histogram`

`histogram(a, bins)` returns a tuple `(counts, edges)`: `counts` is an `Array[int, 1]` of `bins`
counts, and `edges` an `Array[f64, 1]` of the `bins + 1` bin edges, evenly spaced from the
smallest element of `a` to the largest. Bin `k` counts the elements from `edges[k]` up to but not
including `edges[k + 1]`; the last bin also counts the largest element, so every element is
counted once.

The edges are NumPy's `linspace`: `edges[k]` is `k * step + first`, rounded twice, and the last
edge is exactly the largest element. An element's bin is computed by NumPy's arithmetic and then
checked against the edges, as NumPy checks it, so the counts and the edges equal NumPy's
exactly. When every element is equal, the range is widened by 0.5 on each side; an empty `a`
gets the range 0 to 1 and zero counts. A NaN or infinite element, or `bins` less than 1, panics,
where NumPy raises `ValueError`. It makes two passes over `a`.

```python
from stats import histogram


def show_distribution(energies: Array[f64, 1]):
    """One line per bin: its range and how many energies fall in it."""
    counts, edges = histogram(energies, 10)
    for k in range(10):
        print(f"{edges[k]:8.3f} to {edges[k + 1]:8.3f}: {counts[k]}")
```

## `cov` and `corrcoef`

`cov(x, y)` is the 2 by 2 matrix with the sample variances of `x` and `y` on its diagonal and
their sample covariance off it, dividing by `n - 1`, as `numpy.cov(x, y)`. Each entry is the dot
product of the two arrays' deviations from their means, added in the pairwise order of
[`a.dot(b)`](arrays.md#methods), times `1 / (n - 1)`, which is how NumPy scales. `corrcoef(x, y)`
divides each entry by the standard deviation of its row and then of its column, and limits the
result to -1 to 1, as `numpy.corrcoef(x, y)` does.

NumPy computes the dot products with BLAS, whose order of additions depends on the machine and
the library, so its last bit can differ from one computer to the next. Here the order is fixed,
so a result has the same bits everywhere and agrees with NumPy's to about 15 significant digits.
Arrays of different lengths panic; fewer than two elements give NaN or infinity, as NumPy's
do, and a constant array has no correlation, so `corrcoef` gives NaN.

```python
from stats import corrcoef


def correlation(x: Array[f64, 1], y: Array[f64, 1]) -> f64:
    """Pearson's correlation coefficient of two series of the same length."""
    return corrcoef(x, y)[0, 1]
```

## Not built yet

The first version takes rank-1 `f64` arrays. NumPy's `axis=` argument, several quantiles in
one call, its other quantile methods, and arrays of other element types are not built: they need
functions that accept arrays of any rank or element type, which the standard library cannot
write yet. `histogram`'s other ways to choose bins, such as a list of edges or `"auto"`,
`range=`, `weights=` and `density=`, and `cov` and `corrcoef` of more than two variables are not
built either.
