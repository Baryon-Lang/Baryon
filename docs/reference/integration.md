# Numerical integration

The `integrate` module is SciPy's `scipy.integrate` written in Baryon
([decision 0054](../decisions/0054-scipy-module-names.md)), with SciPy's names. It holds
[`quad`](#quad-integrating-a-function), which integrates a function over a finite or infinite
interval (roadmap 3.19.4 and 3.19.5), [`solve_ivp`](#solve_ivp-solving-an-ode), which solves an
ordinary differential equation from its starting value (roadmap 3.19.7), and the fixed rules,
which integrate samples already taken at known points (roadmap 3.19.3):

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

## `solve_ivp`: solving an ODE

`solve_ivp(fun, t_span, y0)` solves the initial value problem `dy/dt = fun(t, y)` with
`y(t_span[0]) = y0`, from `t_span[0]` to `t_span[1]`, as `scipy.integrate.solve_ivp` does with
its default method, RK45. RK45 is the Dormand-Prince pair of explicit Runge-Kutta formulas, of
orders 5 and 4: each step calls `fun` six times, and the difference between the two orders
estimates the step's error. A step whose error is too large is retried shorter; a good step
makes the next one longer. The state `y` may be an `f64`, for one equation, or a `Fixed` vector or
an `Array[f64, 1]`, for a system:

```python
from math import cos
from integrate import solve_ivp

State = Fixed[f64, (2,)]


def spring(t: f64, y: State, omega: f64) -> State:
    # y[0] is the position in metres and y[1] the velocity in metres per second.
    return State(y[1], -omega * omega * y[0])


def main():
    result = solve_ivp(spring, (0.0, 10.0), State(1.0, 0.0), args=(2.0,), rtol=1e-8, atol=1e-10)
    last = len(result.t) - 1
    print(f"{result.message} {len(result.t)} steps, {result.nfev} calls of spring")
    print(f"x(10) = {result.y[0, last]:.8f}, exact {cos(20.0):.8f}")
```

This prints `251 steps, 1604 calls of spring` and `x(10) = 0.40808205, exact 0.40808206`, the
step counts SciPy gives for the same call.

| Parameter | Meaning | Default |
| --- | --- | --- |
| `fun` | The right-hand side, `fun(t, y, *args)`, returning `dy/dt` as the same type as `y` | |
| `t_span` | `(t0, t_bound)`, a `tuple[f64, f64]`; `t_bound < t0` integrates backwards | |
| `y0` | The starting state, an `f64`, a `Fixed` vector of `f64` or an `Array[f64, 1]` | |
| `method` | The integrator, in SciPy's spelling: `"RK45"` or `"DOP853"` (see below) | `"RK45"` |
| `t_eval` | The times to report the state at, an `Array[f64, 1]` within `t_span` and in the run's direction; `None` reports every step | `None` |
| `args` | A tuple passed on to `fun` after `t` and `y` | `()` |
| `first_step` | The size of the first step; `None` picks it by SciPy's rule | `None` |
| `max_step` | The longest step allowed | `inf` |
| `rtol`, `atol` | Each step keeps its error in each element below `atol + rtol * abs(y)`; `rtol` below 2.2e-14 is raised to it | `1e-3`, `1e-6` |

The result is an `OdeResult` with SciPy's 11 fields, read as attributes:

| Field | Value |
| --- | --- |
| `t` | The times of the steps, an `Array[f64, 1]`, from `t0` to where the run stopped; with `t_eval`, the times of `t_eval` the run reached |
| `y` | The state at each time, an `Array[f64, 2]` of shape `(len(y0), len(t))`; an `f64` state has length 1 |
| `nfev` | How many times `fun` was called |
| `njev`, `nlu` | Jacobians computed and LU factorizations made: 0 for RK45 and DOP853 |
| `status`, `success`, `message` | 0, `True` and SciPy's text when the run reached `t_bound`; -1 and `False` when it failed |
| `sol` | The continuous solution: `None`, since `dense_output=` is not built yet (roadmap 3.19.8) |
| `t_events`, `y_events` | `None`, since `events=` is not built yet (roadmap 3.19.9) |

With `t_eval`, the run takes the same steps, and the state at each time asked for comes from
the step that covers it, through RK45's fourth-order interpolant, as in SciPy. A `t_span` with
no length reports none of them, as SciPy does. A time outside `t_span`, or times out of order,
panic with SciPy's message.

`method=` is chosen when compiling
([decision 0065](../decisions/0065-integrate-api.md#2-method-is-a-string-chosen-when-compiling-when-it-is-a-literal)).
Written as a literal, or left out, it picks the integrator at compile time, and only that one is
compiled; a literal that names no method built, such as `"RK54"`, is a compile error that lists
the methods. A method held in a variable is picked at run time: the call then raises
`ValueError` for an unknown name, so it needs a `try` or a `Raises` like any call that raises.
Both methods are then compiled for `fun`.

`method="DOP853"` is Dormand and Prince's method of order 8, SciPy's choice for tight
tolerances. Each step calls `fun` twelve times, twice RK45's six, but its steps are far longer
when `rtol` is small. For the spring above with `rtol=1e-10` and `atol=1e-12`,

```python
result = solve_ivp(
    spring, (0.0, 10.0), State(1.0, 0.0), "DOP853", args=(2.0,), rtol=1e-10, atol=1e-12
)
```

takes 66 steps and 782 calls of `spring` to reach `x(10) = 0.4080820619`, where RK45 takes 635
steps and 3878 calls for `0.4080820617`; the exact value is `0.4080820618`. Both counts are
SciPy's. With `t_eval`, a step that covers a time asked for calls `fun` three more times to
build DOP853's seventh-order interpolant, as SciPy's does.

A run fails, as SciPy's does, when a step would have to be shorter than ten times the spacing of
floats at `t`, as near a singularity: `dy/dt = y * y` from `y(0) = 1` stops just before
`t = 1` with `status` -1 and the message `Required step size is less than spacing between
numbers.`, keeping the steps it took. It never raises. A `max_step` or `first_step` that is not
positive, a negative `atol`, or a `first_step` longer than `t_span` is a mistake in the program
and panics with SciPy's message, where SciPy raises `ValueError`.

`solve_ivp` takes SciPy's steps and calls `fun` as many times. SciPy forms its sums with BLAS,
whose rounding depends on the processor, so a step's size can differ from SciPy's in its last
bits; over a run the results agree with SciPy's to about ten significant digits, fewer near a
singularity. DOP853's error estimate is a sum of thirteen terms that nearly cancel, so its
rounding moves a step's size more: its results agree to about eight digits. Each step keeps
one state; the result stores `len(t)` states.

## Differences from SciPy

- The samples are rank-1. SciPy's `axis=` and arrays of higher rank are not built yet.
- `x` must be an array; SciPy also takes a list.
- `quad` raises `IntegrationError` where SciPy warns, and panics where SciPy raises
  `ValueError`. The panic for too many break points leaves out the two numbers SciPy's message
  has.
- `quad`'s `weight=` is roadmap 3.19.5, and `full_output=` is not built: its third value would
  change the shape of every call (decision 0065).
- `quad`'s `points` is a `list[f64]`; SciPy also takes an array or a tuple.
- `solve_ivp` takes an `f64`, a `Fixed` or an `Array[f64, 1]` state; SciPy also takes a list.
  SciPy's other parameters, `dense_output=`, `events=`, `vectorized=` and an array `atol`, are
  not built yet (roadmap 3.19.8 and 3.19.9; ideas I.64 and I.77), and `method=` takes only
  `"RK45"` and `"DOP853"`.
- `method=` held in a variable raises `ValueError` with a message that lists only the methods
  built, where SciPy's lists every method it has.
- With `t_eval` and an empty `t_span`, `y` is an array of shape `(len(y0), 0)`, where SciPy
  gives an empty list.
- `solve_ivp` raises `rtol` to 2.2e-14 without SciPy's warning, and panics where SciPy raises
  `ValueError`.
