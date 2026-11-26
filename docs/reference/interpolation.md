# Interpolation

The `interpolate` module is SciPy's `scipy.interpolate` written in Baryon, with SciPy's names
([decision 0054](../decisions/0054-scipy-module-names.md),
[decision 0069](../decisions/0069-interpolate-api.md)). It holds `CubicSpline`, the twice
continuously differentiable cubic through given points, and `PPoly`, the piecewise polynomial
every spline is (roadmap 3.21.2). NumPy's linear interpolation is
[`Array.interp`](arrays.md#interpolation).

| Class | What it is | SciPy |
| --- | --- | --- |
| `CubicSpline(x, y, axis=0, bc_type="not-a-knot", extrapolate=None)` | The cubic spline through the points `(x[i], y[i])` | `scipy.interpolate.CubicSpline` |
| `PPoly(c, x, extrapolate=None)` | A piecewise polynomial from its coefficients and breakpoints | `scipy.interpolate.PPoly` |

An interpolator is built by calling its class with the data, as in SciPy: its
[`__init__`](types.md#__init__) computes every coefficient, so the program never sees a half-built
one. It is then called on one `f64` to give one `f64`:

```python
from interpolate import CubicSpline


def main():
    # Height of a thrown ball, in metres, at five times, in seconds.
    t = Array([0.0, 0.5, 1.0, 1.5, 2.0])
    h = Array([1.5, 4.7, 5.4, 3.6, 0.7])
    ball = CubicSpline(t, h)
    print(f"height at 0.8 s: {ball(0.8):.3f} m")
    print(f"speed at 0.8 s: {ball(0.8, 1):.3f} m/s")
    print(f"mean height over 2 s: {ball.integrate(0.0, 2.0) / 2.0:.3f} m")
```

Every coefficient, value, derivative and integral has the bits SciPy 1.18 gives on Linux x86_64,
so a test can compare with SciPy using `==`. The one exception is
[three points with `"not-a-knot"`](#three-points), which agree to the last bit or two.

## `CubicSpline`

`CubicSpline(x, y)` finds the slope at each point by solving a tridiagonal system, LAPACK's
`gtsv` as SciPy calls it, and then each interval's cubic from its two values and two slopes.
`x` must increase strictly, and `x` and `y` are `Array[f64, 1]`s of the same length, at least 2.
Building costs about 20 operations a point and four arrays of `len(x)` numbers.

`bc_type` names the conditions at the two ends:

| `bc_type` | At each end |
| --- | --- |
| `"not-a-knot"`, the default | The third derivative is continuous at the second and second-to-last points, so the first two intervals are one cubic, and so are the last two |
| `"clamped"` | The slope is 0 |
| `"natural"` | The second derivative is 0 |
| `"periodic"` | The slope and second derivative at the end match those at the start; `y[0]` must equal `y[-1]` |

Two points give the straight line through them, unless clamped or natural. `axis` is accepted
for SciPy's signature and has no effect, since `y` has one dimension.

A spline is a `PPoly` of order 4. It embeds one ([embedded structs](types.md#embedded-structs)),
so its breakpoints `cs.x`, its coefficients `cs.c` and `PPoly`'s methods are the spline's own.

## `PPoly`

`PPoly(c, x)` holds the breakpoints `x` and one polynomial for each interval between them.
Column `i` of `c`, an `Array[f64, 2]` of shape `(k, len(x) - 1)`, is the polynomial on
`[x[i], x[i + 1]]`, highest power first, in `s = t - x[i]`:

```text
p(t) = c[0, i] * s**(k - 1) + c[1, i] * s**(k - 2) + ... + c[k - 1, i]
```

`PPoly` copies `c` and `x`. Its methods, which `CubicSpline` shares:

| Method | Result |
| --- | --- |
| `p(t, nu=0, extrapolate=None)` | The polynomial's `nu`-th derivative at `t` |
| `p.derivative(nu=1)` | The `nu`-th derivative, a `PPoly` of order `k - nu` |
| `p.antiderivative(nu=1)` | The `nu`-th antiderivative, a `PPoly` of order `k + nu`, zero at `x[0]` and continuous |
| `p.integrate(a, b, extrapolate=None)` | The integral from `a` to `b`, negative when `b < a` |

A point at a breakpoint belongs to the interval on its right, and the last breakpoint to the
last interval. A call costs a bisection of the breakpoints and `k` multiply-adds.

## Outside the breakpoints

With `extrapolate=True`, the default, a point before `x[0]` or after `x[-1]` is read from the
first or last polynomial. With `extrapolate=False` it gives NaN, and so does an integral with a
bound outside. A `CubicSpline` with `bc_type="periodic"` and no `extrapolate` wraps a point
outside around by the period `x[-1] - x[0]`, and integrates over whole periods. Passing
`extrapolate` to a call or to `integrate` overrides the interpolator's own choice for that one
call.

## Three points

Three points with `"not-a-knot"` at both ends make the two end conditions the same equation,
so SciPy fits the one parabola through the points by solving a 3 × 3 system with LAPACK. SciPy's
LAPACK is OpenBLAS, whose kernels round through fused multiply-adds. Baryon solves the system by
Gaussian elimination in [`linalg.solve`](linear-algebra.md)'s order, so these coefficients agree
with SciPy's to the last bit or two, not bit for bit.

## Input that does not fit

Input that does not fit is a mistake in the program, so it panics with SciPy's message where
SciPy raises `ValueError` ([decision 0069](../decisions/0069-interpolate-api.md) point 5):

- fewer than two points, or `x` and `y` of different lengths;
- a value in `x` or `y` that is NaN or infinite;
- an `x` that does not strictly increase;
- an unknown `bc_type`;
- `bc_type="periodic"` with `y[0]` and `y[-1]` more than about `1e-15` apart;
- for `PPoly`, fewer than two breakpoints, `c` with no rows or with other than `len(x) - 1`
  columns, or breakpoints that go down;
- a negative `nu` in a call.

## Differences from SciPy

- A call takes one `f64` and gives one `f64`. SciPy also takes an array and gives an array, and
  gives a 0-d array for one number. Calling on an array waits for functions that apply element
  by element (idea I.61).
- `y` has one dimension, so one spline interpolates one quantity.
- `bc_type` takes its four names. SciPy's `((order, value), (order, value))` form, which sets a
  first or second derivative at each end, waits for strings in unions.
- `PPoly` takes increasing breakpoints only. SciPy also takes decreasing ones.
- `extrapolate` is a `bool` or `None`. SciPy's `extrapolate="periodic"` comes only through
  `bc_type="periodic"`.
- A derivative or antiderivative of a `CubicSpline` is a `PPoly`, where SciPy gives a
  `CubicSpline`. Both evaluate the same way.
- SciPy's `PPoly.solve`, `roots`, `extend`, `from_spline` and `from_bernstein_basis` are not
  built yet.
