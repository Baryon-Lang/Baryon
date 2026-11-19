# Numerical integration

The `integrate` module is SciPy's `scipy.integrate` written in Baryon
([decision 0054](../decisions/0054-scipy-module-names.md)), with SciPy's names. Today it
holds the fixed rules, which integrate samples already taken at known points (roadmap 3.19.3):

| Function | Result | SciPy |
| --- | --- | --- |
| `trapezoid(y, x=None, dx=1.0)` | The integral of `y` by the trapezoidal rule | `scipy.integrate.trapezoid` |
| `cumulative_trapezoid(y, x=None, dx=1.0, initial=None)` | The running integral, one value per interval | `scipy.integrate.cumulative_trapezoid` |
| `simpson(y, x=None, dx=1.0)` | The integral of `y` by Simpson's rule | `scipy.integrate.simpson` |
| `romb(y, dx=1.0)` | The integral of `2**k + 1` evenly spaced samples by Romberg's method | `scipy.integrate.romb` |

Each takes the samples `y` as an `Array[f64, 1]`. They lie at the points `x`, also an
`Array[f64, 1]`, or `dx` apart when `x` is not given; `romb` takes only `dx`. Each gives the bits
SciPy 1.18 gives for the same samples: every sum adds in NumPy's pairwise order, through
[`a.sum()`](arrays.md#the-order-sum-adds-in), and every other step rounds where SciPy's does, so
a test can compare a result with SciPy's using `==`.

```python
from integrate import simpson, trapezoid


def main():
    # Speed of a cart sampled every 0.5 s, in metres per second.
    v = Array([0.0, 1.2, 2.1, 2.7, 3.0])
    print(f"distance {trapezoid(v, dx=0.5):.3f} m by trapezoids")
    print(f"distance {simpson(v, dx=0.5):.3f} m by Simpson's rule")
```

Each reads its arrays once. `cumulative_trapezoid` stores its result, one `f64` per interval;
`romb` keeps `(k + 1)(k + 2) / 2` numbers for `2**k + 1` samples; the others store nothing.

## The rules

`trapezoid` adds `(x[i + 1] - x[i]) * (y[i + 1] + y[i]) / 2.0` for each interval. Fewer than two
samples give 0.0.

`cumulative_trapezoid` gives the same intervals added one at a time, left to right, as NumPy's
`cumsum` adds: element `i` is the integral from the first sample to sample `i + 1`, so the result
is one shorter than `y`. With `initial=0.0` it starts with that 0.0 and is as long as `y`. SciPy
accepts no other `initial`.

`simpson` covers an odd number of samples with pairs of intervals, each pair fitted by a
parabola, and the spacing may vary from pair to pair. With an even number of samples, the pairs
cover all but the last interval, which Cartwright's correction adds from the last three samples,
as SciPy does since version 1.11. Two samples give the trapezoid and one gives 0.0.

`romb` takes trapezoid sums on halving steps and refines them by Richardson extrapolation. It
needs one plus a power of 2 samples. SciPy's `show=` table is not printed.

## Samples that do not fit

Samples that do not fit a rule are a mistake in the program, so they panic where SciPy raises
`ValueError`, as `linalg` does ([decision 0040](../decisions/0040-linear-algebra-and-fft.md)):
`x` of another length than `y`, an empty `y` for `simpson` or `cumulative_trapezoid`, an
`initial` other than 0.0, or a number of samples for `romb` that is not one plus a power of 2.

```text
panic: assertion failed: n == intervals, "romb needs one plus a power of 2 samples"
```

## Differences from SciPy

- The samples are rank-1. SciPy's `axis=` and arrays of higher rank are not built yet.
- `x` must be an array; SciPy also takes a list.
- Adaptive quadrature (`quad`) and ODE solvers (`solve_ivp`) are not built yet (roadmap 3.19).
