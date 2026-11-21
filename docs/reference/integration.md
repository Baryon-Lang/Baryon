# Numerical integration

The `integrate` module is SciPy's `scipy.integrate` written in Baryon
([decision 0054](../decisions/0054-scipy-module-names.md)), with SciPy's names. It holds
[`quad`](#quad-integrating-a-function), which integrates a function over a finite or infinite
interval (roadmap 3.19.4 and 3.19.5), and the fixed rules, which integrate samples already taken
at known points (roadmap 3.19.3):

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

## `quad`: integrating a function

`quad(func, a, b, args=(), epsabs=1.49e-8, epsrel=1.49e-8, limit=50, points=None)` gives the
integral of `func` from `a` to `b` and an estimate of its absolute error, as the tuple
`(integral, error)`. It runs QUADPACK's QAGS, translated step for step from its Fortran, which
SciPy 1.18 runs too, so both the integral and the error have SciPy's bits.

QAGS applies the 21-point Gauss-Kronrod rule to the interval, then keeps halving the piece with
the largest error estimate until the total error is at most
max(`epsabs`, `epsrel` × |integral|), or `limit` pieces are used. When the halving converges
slowly, as near a singularity such as 1/√x at 0, Wynn's epsilon algorithm extrapolates the
sequence of estimates. `func` is never called at `a` or `b`, so an integrand that is infinite at
an end, such as 1/√x from 0, is fine.

`func` takes the point `x: f64` first and returns an `f64`. Extra parameters come after it and
are passed as the tuple `args`, one element per parameter, as in SciPy; they may be of any
types, through a [variadic type parameter](types.md#extra-arguments-for-a-callback). `func` is
inlined into `quad`, so the call costs nothing
([decision 0026](../decisions/0026-functions-as-arguments.md)).

```python
import math
from integrate import quad, IntegrationError


def gaussian(x: f64, mu: f64, sigma: f64) -> f64:
    z = (x - mu) / sigma
    return math.exp(-0.5 * z * z) / (sigma * math.sqrt(2.0 * math.pi))


def bump(x: f64) -> f64:
    return abs(x - 0.3)


def main() -> Raises[None, IntegrationError]:
    area, error = quad(gaussian, -1.0, 1.0, (0.0, 1.0))
    print(f"{area:.6f} +- {error:.1e}")        # 0.682689 +- 7.6e-15
    print(quad(bump, 0.0, 1.0, points=[0.3]))   # (0.29, 3.219646771412954e-15)
```

`points` lists places inside the interval where `func` is hard to integrate: a jump, a kink or
a singularity. QAGP, QAGS's variant, then starts from the pieces between them. Points outside
the interval, and repeats, are dropped, as SciPy drops them. There must be fewer points than
`limit`.

`a > b` integrates backwards and gives the negated integral; `a == b` gives `(0.0, 0.0)` without
calling `func`. `quad` calls `func` at most 21 × (2 × `limit` − 1) times and keeps 6 × `limit`
numbers.

### Infinite intervals

`a` may be `-inf` and `b` may be `inf`, from `math`. `quad` then runs QUADPACK's QAGI, as SciPy
does: it maps the infinite interval onto (0, 1] with x = `a` + (1 − t)/t, or the mirror image for
`-inf`, and applies the same halving with the 15-point Gauss-Kronrod rule to the mapped
integrand. On (−∞, ∞) each point t adds `func` at x and at −x. `limit=1` uses 15 points of
`func`, or 30 on the whole line, and each halving 30 more, or 60.

```python
from math import inf
from integrate import quad, IntegrationError


def lorentz(x: f64) -> f64:
    return 1.0 / (1.0 + x * x)


def main() -> Raises[None, IntegrationError]:
    print(quad(lorentz, 0.0, inf))     # (1.5707963267948966, 2.5777915205519274e-10)
    print(quad(lorentz, -inf, inf))    # (3.141592653589793, 5.155583041103855e-10)
```

`func` must fall off fast enough for the integral to exist; one that does not, such as 1/x,
raises `IntegrationError`. `points` cannot be used with an infinite limit: as in SciPy, the call
panics.

### When `quad` fails

Where SciPy warns with an `IntegrationWarning` and still returns a value, `quad` raises
`IntegrationError`, whose `message` is SciPy's text
([decision 0065](../decisions/0065-integrate-api.md) point 3). A large error estimate is easy to
miss, and Baryon has no warnings. Each of QUADPACK's five failures raises it:

| Failure | `message` begins |
| --- | --- |
| `limit` pieces used | `The maximum number of subdivisions (50) has been achieved.` |
| Roundoff stops the tolerance being reached | `The occurrence of roundoff error is detected` |
| Very bad behavior somewhere in the interval | `Extremely bad integrand behavior occurs` |
| The extrapolation does not converge | `The algorithm does not converge.` |
| The integral is probably divergent | `The integral is probably divergent, or slowly convergent.` |

Note that `limit=1` always raises the first, even for an integrand the first rule integrates
exactly, as SciPy always warns.

Arguments that make no sense panic, where SciPy raises `ValueError` (decision 0065): `limit`
below 1, `epsabs <= 0` with `epsrel` below max(50 × 2.2e-16, 5e-29), a break point outside the
interval with too small a `limit`, or as many break points as `limit`.

```text
panic: assertion failed: limit >= 1, "Invalid 'limit' argument. There must be at least one subinterval"
```

## Differences from SciPy

- The samples are rank-1. SciPy's `axis=` and arrays of higher rank are not built yet.
- `x` must be an array; SciPy also takes a list.
- `quad` raises `IntegrationError` where SciPy warns, and panics where SciPy raises
  `ValueError`. The panic for too many break points leaves out the two numbers SciPy's message
  has.
- `quad`'s `weight=` is roadmap 3.19.5, and `full_output=` is not built: its third value would
  change the shape of every call (decision 0065).
- `quad`'s `points` is a `list[f64]`; SciPy also takes an array or a tuple.
- ODE solvers (`solve_ivp`) are not built yet (roadmap 3.19).
