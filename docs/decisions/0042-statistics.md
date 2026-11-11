# 0042. Statistics keep NumPy's names and bits, and its functions live in `stats`

Status: Proposed
Date: 2026-11-11
Roadmap: 3.16

## Context

Roadmap 3.16 adds the statistics a physicist or data scientist reaches for first: the mean,
variance and standard deviation, the median and percentiles, a weighted average, a histogram,
and covariance and correlation. Every one has a NumPy spelling that readers of Baryon already
know, and NumPy's results are what their tests compare against.

NumPy spells them two ways. `mean`, `var` and `std` are methods of an array, `a.mean()`, as
`sum`, `min` and `max` already are here. `median`, `percentile`, `quantile`, `average`,
`histogram`, `cov` and `corrcoef` are functions only: NumPy has no `a.median()`. Baryon writes
NumPy's submodules without the `numpy.` prefix, `linalg` and `fft` (decision 0040), but these
functions live at NumPy's top level, so they need a home.

Three homes were considered:

- **A `stats` module**, `from stats import median`. Nothing in it clashes with NumPy, and
  `scipy.stats` has no `median` or `percentile` of its own, so the probability distributions of
  roadmap 3.23, which SciPy keeps in `scipy.stats`, can join the same module later under
  SciPy's names.
- **Built-in names**, `median(a)` with no import, as `sum` is. About eight more names every
  program sees, and a user's own `average` or `histogram` would shadow them.
- **A `statistics` module**, Python's name. Python's `statistics` module gives the same names
  other meanings: `statistics.variance` divides by `n - 1` where `numpy.var` divides by `n`, and
  `statistics.quantiles` uses another method. A program moved from Python would compile and give
  different answers.

## Decision

1. **Methods stay methods.** `a.mean()`, `a.var(ddof=0)` and `a.std(ddof=0)` are array methods,
   as in NumPy, with `ddof` defaulting to 0.
2. **The functions live in `stats`**, with NumPy's names and arguments: `median`, `percentile`,
   `quantile`, `average`, `histogram`, `cov` and `corrcoef`. They are written in Baryon in the
   standard library (decision 0038).
3. **Results have NumPy's bits.** Each function rounds where NumPy's does: sums in the pairwise
   order of `a.sum()`, the mean of the two middle elements as `(x + y) / 2`, and `quantile`'s
   default method, "linear", interpolated with NumPy's two formulas, `a + (b - a) * t` below
   `t = 0.5` and `b - (b - a) * (1 - t)` from there. Tests compare against NumPy and SciPy.
   Where NumPy calls BLAS, as `cov` does through `dot`, the order is the pairwise one of
   `a.dot(b)`, and the last bits can differ from NumPy's machine-dependent ones.
4. **NaN gives NaN, and so does too little data.** A NaN anywhere in the input makes `median`,
   `percentile` and `quantile` NaN, as NumPy's do. Where NumPy answers too little data with NaN
   and a `RuntimeWarning`, the answer here is the same NaN, without the warning: `median` and
   `a.mean()` of an empty array, `a.var()` when `n - ddof` is 0 (infinity when the squares do not
   sum to 0), and `cov` and `corrcoef` of empty or one-element arrays, whose `1 / (n - 1)` scale
   is infinite or NaN, so every entry is NaN.
5. **A wrong argument panics.** A percentile outside 0 to 100, a quantile outside 0 to 1, a
   percentile or quantile of an empty array, weights that sum to zero (an empty `average`
   included), arrays of different lengths in `average`, `cov` and `corrcoef`, and a histogram
   with fewer than one bin or a NaN or infinite element panic, where NumPy raises
   `ValueError`, `IndexError` or `ZeroDivisionError`. They are mistakes in the program, not in
   its data, as `a.min()` of an empty array is, which panics for the same reason.

## Consequences

- `from stats import median, percentile` reads as Python with NumPy's meaning, and a result can be
  checked against NumPy to the last bit.
- Python's `statistics` module is not provided under that name. A program that uses it is told
  the module does not exist, rather than silently getting NumPy's answers.
- The first version takes rank-1 `f64` arrays. Other element types, an `axis` argument, and
  several quantiles at once need function overloading or generic functions over the rank,
  which the library cannot express yet; they come with those features.
- `median` and `quantile` copy the array once and select in place: expected about 3n
  comparisons, the same order of work as NumPy's partition.
