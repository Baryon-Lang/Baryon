# Integration

*Numerical integration* finds an integral, or the solution of a differential equation, by
computing with numbers rather than with symbols. Baryon's `integrate` module has
`scipy.integrate`'s functions with SciPy's names: `quad` integrates a function, the fixed rules
such as `simpson` integrate samples already taken, and `solve_ivp` solves an ordinary
differential equation from a starting state. This page shows them in programs that build and run
today, as CI checks, then lists what changes when a program moves from SciPy, and how fast it
runs beside SciPy. It assumes you have read [Arrays and numbers](arrays-and-numbers.md).

[Integration](../reference/integration.md) in the reference gives every parameter, the accuracy
and each difference from SciPy in detail.

## 1. Integrating a function: `quad`

`quad(f, a, b)` integrates `f` from `a` to `b` the way SciPy's does, with QUADPACK's adaptive
rules, and returns the integral and an estimate of its error. Either limit may be infinite.
`args=` passes extra arguments to `f` after `x`. Here it checks Maxwell's distribution of the
speeds of gas molecules: the probabilities add up to 1, and the mean speed is a √(8/π).

File `maxwell/main.byn`:

```python
from math import exp, inf, pi, sqrt
from integrate import IntegrationError, quad


def maxwell(v: f64, a: f64) -> f64:
    # Maxwell's distribution of speeds, with a = sqrt(kT / m).
    return sqrt(2.0 / pi) * v * v / (a * a * a) * exp(-v * v / (2.0 * a * a))


def speed_weighted(v: f64, a: f64) -> f64:
    return v * maxwell(v, a)


def main() -> Raises[None, IntegrationError]:
    a = 300.0  # metres per second
    total, error = quad(maxwell, 0.0, inf, args=(a,))
    print("total probability:", total, "+/-", error)
    mean, error = quad(speed_weighted, 0.0, inf, args=(a,))
    print("mean speed:", mean, "exact:", a * sqrt(8.0 / pi))
```

```text
$ baryon run maxwell
total probability: 1.0 +/- 3.272555626498714e-09
mean speed: 478.7307364817192 exact: 478.73073648171925
```

These are SciPy's numbers. `quad` can fail, as when an integral does not converge, so a call
raises `IntegrationError` where SciPy only warns, and `main` says it may raise one.

## 2. Integrating samples: `trapezoid` and `simpson`

Data from an instrument is samples, not a function. `trapezoid` joins them with straight lines,
`simpson` with parabolas, and `cumulative_trapezoid` gives the running total. Here they turn a
car's speed, read every 2 seconds, into the distance it went:

File `distance/main.byn`:

```python
from integrate import cumulative_trapezoid, simpson, trapezoid


def main():
    # A car's speed in metres per second, read every 2 seconds.
    speed = Array([0.0, 4.1, 9.8, 15.2, 19.9, 23.1, 24.8, 25.0, 25.0])
    print("trapezoid:", trapezoid(speed, dx=2.0), "m")
    print("simpson:", simpson(speed, dx=2.0), "m")
    print("so far:", cumulative_trapezoid(speed, dx=2.0))
```

```text
$ baryon run distance
trapezoid: 268.79999999999995 m
simpson: 269.06666666666666 m
so far: [  4.1  18.   43.   78.1 121.1 169.  218.8 268.8]
```

Each value has SciPy's bits, so a test can compare it with SciPy's using `==`.

## 3. A differential equation: `solve_ivp`

`solve_ivp(fun, t_span, y0)` solves dy/dt = fun(t, y) from y(t0) = y0. The state `y` can be an
`f64` for one equation, or a `Fixed` vector or an `Array[f64, 1]` for a system. A `Fixed`
vector is the fastest for a small system. Here a ball falls from 100 m with air drag, and
`t_eval` asks for the state once a second:

File `drag/main.byn`:

```python
from integrate import solve_ivp

State = Fixed[f64, (2,)]


def fall(t: f64, y: State, g: f64, k: f64) -> State:
    # y[0] is the height in metres, y[1] the upward velocity; drag is k v |v|.
    return State(y[1], -g - k * y[1] * abs(y[1]))


def main():
    times = Array([0.0, 1.0, 2.0, 3.0, 4.0])
    result = solve_ivp(fall, (0.0, 4.0), State(100.0, 0.0), t_eval=times, args=(9.81, 0.02))
    print(result.message)
    for j in range(len(result.t)):
        height = result.y[0, j]
        speed = result.y[1, j]
        print(f"t = {result.t[j]:.1f} s: height {height:.3f} m, speed {speed:.3f} m/s")
    print("calls of fall:", result.nfev)
```

```text
$ baryon run drag
The solver successfully reached the end of the integration interval.
t = 0.0 s: height 100.000 m, speed 0.000 m/s
t = 1.0 s: height 95.248 m, speed -9.215 m/s
t = 2.0 s: height 82.521 m, speed -15.717 m/s
t = 3.0 s: height 64.821 m, speed -19.249 m/s
t = 4.0 s: height 44.630 m, speed -20.904 m/s
calls of fall: 50
```

`solve_ivp` takes SciPy's steps, so it calls `fall` 50 times, as SciPy does, and the states agree
with SciPy's to about ten digits. With `dense_output=True` it also keeps a continuous solution,
`result.sol`, to call at any time; the reference shows how.

## 4. Tight tolerances: DOP853

The default method, RK45, is of order 5. For tight tolerances `method="DOP853"`, of order 8,
takes far longer steps. Here a planet goes round ten times, and both methods are asked for
`rtol=1e-10`:

File `orbit/main.byn`:

```python
from math import pi, sqrt
from integrate import solve_ivp

State = Fixed[f64, (4,)]


def kepler(t: f64, y: State) -> State:
    # A planet around a sun of GM = 1: position (y[0], y[1]), velocity (y[2], y[3]).
    r = sqrt(y[0] * y[0] + y[1] * y[1])
    r3 = r * r * r
    return State(y[2], y[3], -y[0] / r3, -y[1] / r3)


def main():
    start = State(1.0, 0.0, 0.0, 1.2)  # an ellipse with eccentricity 0.44
    period = 2.0 * pi / (2.0 / 1.0 - 1.2 * 1.2) ** 1.5
    for method in ["RK45", "DOP853"]:
        try:
            r = solve_ivp(kepler, (0.0, 10.0 * period), start, method, rtol=1e-10, atol=1e-12)
            # After ten whole orbits the planet is back at x = 1, y = 0.
            last = len(r.t) - 1
            miss = sqrt((r.y[0, last] - 1.0) ** 2 + r.y[1, last] ** 2)
            print(f"{method}: {r.nfev} calls of kepler, missed the start by {miss:.1e}")
        except ValueError as error:
            print(error.message)
```

```text
$ baryon run orbit
RK45: 12884 calls of kepler, missed the start by 2.1e-07
DOP853: 5798 calls of kepler, missed the start by 2.8e-07
```

DOP853 gets as close with fewer than half the calls. Both counts and both misses are SciPy's.
`method` is a string, as in SciPy. Written as a literal, it picks the method when compiling;
held in a variable, as here, it is picked when running, and an unknown name raises
`ValueError`, which is why the call is in a `try`.

## 5. Coming from SciPy

A program moves by changing `from scipy.integrate import` to `from integrate import`. Each
function has SciPy's name and takes its arguments in SciPy's order. The differences:

| In SciPy | In Baryon | Why |
| --- | --- | --- |
| `solve_ivp(f, span, [1.0, 0.0])` with a list | An `f64`, a `Fixed` vector or an `Array[f64, 1]` | In a measured RK4 loop, an `Array` state was 8.4 times slower than a `Fixed` one ([decision 0065](../decisions/0065-integrate-api.md)) |
| `quad` warns when it cannot reach the tolerance | Raises `IntegrationError` | A warning is easy to miss; the error carries SciPy's message |
| A bad argument, such as `max_step=0`, raises `ValueError` | Panics | A bad argument is a bug in the program, not an error to handle |
| `result["t"]` or `result.t` | `result.t` | Results have SciPy's attributes but are not dicts |
| `events=[hit_ground]` | Not built yet | Its shape waits for a decision (roadmap 3.19.9) |
| `sol(np.array([0.5, 1.0]))` | `sol(0.5)`, one time a call | Idea I.79 |
| `method="Radau"`, `"BDF"`, `"LSODA"`, `"RK23"` | `"RK45"` and `"DOP853"` | Radau and BDF are idea I.65, LSODA idea I.66; RK23 is not planned yet |
| `quad(f, a, b, weight="cos", wvar=w)`, `dblquad`, `nquad` | Not built yet | Roadmap 3.19.5 and 3.19.6 |
| `trapezoid(y, axis=0)` on a 2-D array | Rank-1 samples only | Not built yet |

The values are SciPy's: `quad` and the fixed rules to the bit, `solve_ivp` to about ten digits
with RK45 and eight with DOP853, because SciPy's sums go through BLAS, whose rounding depends on
the processor. Beyond agreeing with SciPy, the answers are right: a test checks `quad` on seven
hard integrals, singular, oscillating and infinite, and `solve_ivp` on the van der Pol oscillator
against values computed by mpmath at 40 digits.

## 6. How fast

Both columns are seconds for the whole workload on one thread, the fastest of three runs, on a
Linux x86-64 cloud machine: Baryon in a `--release` build, and SciPy 1.18.1 with NumPy. A ratio
above 1 means Baryon was faster.

| Workload | Baryon (s) | SciPy (s) | SciPy / Baryon |
| --- | --- | --- | --- |
| 100,000 calls of `quad` on exp(−a x) sin²x over [0, 10] | 0.147 | 2.758 | 18.8 |
| 10,000 calls of `quad` on exp(−a x²) cos x over [0, ∞) | 0.034 | 0.553 | 16.0 |
| `simpson` of 10 million samples, making them included | 0.179 | 0.150 | 0.8 |
| `trapezoid` of the same samples | 0.298 | 0.150 | 0.5 |
| 100 runs of `solve_ivp`, van der Pol to t = 100, RK45 | 0.019 | 4.667 | 240.8 |
| The same 100 runs with DOP853 at `rtol=1e-10` | 0.061 | 8.781 | 143.1 |

Where the work is calling a Python function many times, Baryon is 16 to 240 times faster: SciPy
calls the integrand or the right-hand side through the Python interpreter at every point, and
`solve_ivp`'s stepping loop is Python too, where Baryon compiles the function into the loop that
calls it. Both take the same steps: the 100 RK45 runs call the right-hand side 883,136 times in
each. Where the work is one pass over a large array, NumPy's compiled loops are as fast or
faster: `trapezoid` is 2 times slower than NumPy's, idea I.80 in the
[roadmap](../../ROADMAP.md). [`bench/integrate`](../../bench/integrate/README.md) holds both
programs, to run on your own machine.

## Where to go next

- [Integration](../reference/integration.md) in the reference gives every parameter, the
  accuracy, dense output and every difference from SciPy.
- [Special functions](special-functions.md) covers `special`, with Gauss quadrature.
- [What builds today](../reference/status.md) says what else the compiler builds.
