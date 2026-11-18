# Special functions

A *special function* is a function that turns up so often in physics and statistics that it has
a name: the gamma function that extends the factorial, the error function behind the normal
distribution, the Bessel functions of a vibrating drum. Baryon's `special` module has them with
`scipy.special`'s names. This page shows them in programs that build and run today, as CI
checks, and then lists what changes when a program moves from SciPy. It assumes you have read
[Arrays and numbers](arrays-and-numbers.md).

[Special functions](../reference/special-functions.md) in the reference lists every function,
its accuracy and each difference from SciPy in detail.

## 1. Probability

`ndtr` is the normal distribution's cumulative probability Φ(x), the chance that a standard
normal value falls below `x`, and `ndtri` is its inverse. `stdtr` is the same for Student's t,
with the degrees of freedom first. `gammaln` is the logarithm of the gamma function, so
log n! is `gammaln(n + 1.0)`, finite long after n! itself overflows.

File `probability/main.byn`:

```python
from math import exp, log
from special import gammaln, ndtr, ndtri, stdtr


def main():
    # How many standard deviations hold the middle 95% of a normal distribution?
    z = ndtri(0.975)
    print("z:", z)
    print("check:", ndtr(z) - ndtr(-z))

    # Student's t with 9 degrees of freedom: the chance that |T| < 2.262.
    print("t:", stdtr(9.0, 2.262) - stdtr(9.0, -2.262))

    # Exactly 520 heads in 1000 tosses of a fair coin. The binomial coefficient
    # 1000! / (520! 480!) overflows an f64, so work with its logarithm.
    n = 1000.0
    k = 520.0
    log_p = gammaln(n + 1.0) - gammaln(k + 1.0) - gammaln(n - k + 1.0) - n * log(2.0)
    print("P(520 heads):", exp(log_p))
```

```text
$ baryon run probability
z: 1.959963984540054
check: 0.95
t: 0.9499871544975453
P(520 heads): 0.011340982371196602
```

Every value is the one SciPy prints.

## 2. A pendulum: elliptic integrals

A pendulum's period is 2π √(L/g) only for small swings. For an amplitude θ₀ it is
4 √(L/g) K(m), where K is the *complete elliptic integral of the first kind* and
m = sin²(θ₀/2). `ellipk(m)` takes m, the parameter, as SciPy's does, not the modulus k = √m.

File `pendulum/main.byn`:

```python
from math import pi, radians, sin, sqrt
from special import ellipk


def period(length: f64, amplitude: f64) -> f64:
    """The period in seconds of a pendulum `length` metres long swinging out to `amplitude`
    radians: T = 4 √(L/g) K(m), with m = sin²(amplitude / 2)."""
    g = 9.80665
    m = sin(amplitude / 2.0) ** 2
    return 4.0 * sqrt(length / g) * ellipk(m)


def main():
    small = 2.0 * pi * sqrt(1.0 / 9.80665)
    for degrees in [5.0, 45.0, 90.0, 170.0]:
        t = period(1.0, radians(degrees))
        print(degrees, "degrees:", round(t, 4), "s, ratio", round(t / small, 4))
```

```text
$ baryon run pendulum
5.0 degrees: 2.0074 s, ratio 1.0005
45.0 degrees: 2.0866 s, ratio 1.04
90.0 degrees: 2.3682 s, ratio 1.1803
170.0 degrees: 4.8944 s, ratio 2.4394
```

At 5° the small-swing formula is off by 0.05%; at 90° by 18%.

## 3. Diffraction: Bessel functions

Light through a circular aperture spreads into the *Airy pattern*: a bright disc and fainter
rings. Its brightness at x = k a sin θ, where a is the aperture's radius, is (2 J₁(x)/x)², with
J₁ the Bessel function of the first kind of order 1. The first dark ring is the first zero of
J₁, which sets how fine a detail a telescope can resolve.

File `diffraction/main.byn`:

```python
from math import pi
from special import j1


def intensity(x: f64) -> f64:
    """The brightness of the Airy pattern at x = k a sin θ, relative to its centre."""
    if x == 0.0:
        return 1.0
    return (2.0 * j1(x) / x) ** 2


def first_dark_ring() -> f64:
    """The first zero of J₁ above 0, found by bisection between 3 and 4.5."""
    low = 3.0
    high = 4.5
    for _ in range(60):
        middle = (low + high) / 2.0
        if j1(low) * j1(middle) <= 0.0:
            high = middle
        else:
            low = middle
    return (low + high) / 2.0


def main():
    for x in [0.0, 1.0, 2.0, 3.0]:
        print("I(", x, ") =", round(intensity(x), 6))
    zero = first_dark_ring()
    print("first dark ring at x =", zero)
    print("so sin θ = 1.22 λ / D, where 1.22 is", round(zero / pi, 4))
```

```text
$ baryon run diffraction
I( 0.0 ) = 1.0
I( 1.0 ) = 0.774578
I( 2.0 ) = 0.332612
I( 3.0 ) = 0.051094
first dark ring at x = 3.831705970207512
so sin θ = 1.22 λ / D, where 1.22 is 1.2197
```

`j0`, `j1`, `y0`, `y1`, `i0`, `i1`, `k0` and `k1` are the whole-order functions; `jv`, `yv`,
`iv` and `kv` take any real order first, as in `jv(2.5, x)`. SciPy's `jn_zeros`, which would
replace the bisection, is not built yet.

## 4. Gauss quadrature

`roots_legendre(n)` gives the nodes and weights of n-point Gauss–Legendre quadrature: the sum
of `weights[i] * f(nodes[i])` is the integral of f from −1 to 1, exact for any polynomial of
degree below 2n. For another interval, map the nodes onto it and scale the weights.

File `quadrature/main.byn`:

```python
from math import pi, sin
from special import roots_legendre


def main():
    # Five-point Gauss-Legendre quadrature of sin(x) from 0 to π. The nodes are for
    # [-1, 1], so map each to x = (π/2)(t + 1), which scales the weights by π/2.
    nodes, weights = roots_legendre(5)
    total = 0.0
    for i in range(len(nodes)):
        total += weights[i] * sin(pi / 2.0 * (nodes[i] + 1.0))
    print("integral:", pi / 2.0 * total)
    print("error:", pi / 2.0 * total - 2.0)
```

```text
$ baryon run quadrature
integral: 2.0000001102844713
error: 1.1028447133298869e-07
```

Five evaluations of sin give seven correct digits. The nodes and weights come back as two
`Array[f64, 1]`s, where SciPy gives two NumPy arrays.

## 5. Coming from SciPy

A program moves by changing `from scipy.special import` to `from special import`. Each function
has SciPy's name, takes its arguments in SciPy's order and gives SciPy's value at the same
argument, to the accuracy the reference states. The differences:

| In SciPy | In Baryon | Why |
| --- | --- | --- |
| `gamma(np.array([1.0, 2.5]))` applies to each element | Takes one `f64`; call it in a loop | Array arguments come with roadmap 3.18.13 |
| `jv(1.5, 2 + 1j)` with a complex argument | Real arguments only | Complex Bessel functions are roadmap 3.18.6 |
| `gamma(n)` with `n` an `int` variable | `gamma(f64(n))` | Baryon converts a number only when asked; a literal `gamma(5)` works ([Types](../reference/types.md#conversions)) |
| `with special.errstate(...)`, `special.geterr()` | Not there | Baryon has no warnings. A value out of the domain gives NaN, as SciPy's does |
| `roots_legendre(0)` raises `ValueError` | Panics | A bad count is a bug in the program, not an error to handle |
| NaN, ∞ or no answer at the ends of some ranges: `hyp1f1(2, 3, inf)` never returns, `airy(104.0)` gives NaN for Bi | The limit, such as ∞ or 0 | Each case is listed under "At the ends" in the reference |
| Values off by more than 64 ULP in some cases | Within the stated accuracy everywhere it is tested | Each function is checked against values recorded with mpmath; the [accuracy table](../reference/special-functions.md#accuracy) gives SciPy's worst error beside Baryon's |
| `airy(x)` returns four arrays | Returns a `tuple[f64, f64, f64, f64]` | Ai, Ai′, Bi and Bi′, in SciPy's order |

The reference's [Not built yet](../reference/special-functions.md#not-built-yet) section lists
the functions still to come.

## 6. How fast

Both columns are seconds for 10 million calls on one thread, the fastest of three runs, on a
Linux x86-64 cloud machine: Baryon calling the function in a loop in a `--release` build, and
SciPy 1.18.1 applying it to one NumPy array of the same 10 million arguments. A ratio above 1
means SciPy was faster.

| Function | Arguments | Baryon (s) | SciPy (s) | Baryon / SciPy |
| --- | --- | --- | --- | --- |
| `gamma(x)` | 0.1 to 20 | 0.21 | 0.13 | 1.6 |
| `gammaln(x)` | 0.1 to 100 | 0.17 | 0.11 | 1.5 |
| `digamma(x)` | 0.1 to 100 | 0.16 | 0.12 | 1.3 |
| `erf(x)` | −5 to 5 | 0.20 | 0.14 | 1.4 |
| `erfc(x)` | −5 to 25 | 0.20 | 0.13 | 1.5 |
| `ndtri(p)` | 0 to 1 | 0.50 | 0.13 | 3.8 |
| `j0(x)` | 0 to 100 | 0.38 | 0.30 | 1.3 |
| `jv(2.5, x)` | 0 to 100 | 2.07 | 3.45 | 0.6 |
| `kv(1.3, x)` | 0.01 to 50 | 2.73 | 2.57 | 1.1 |
| `airy(x)` | −20 to 20 | 7.09 | 12.85 | 0.6 |
| `gammainc(3.7, x)` | 0 to 20 | 1.58 | 0.95 | 1.7 |
| `betainc(2.5, 3.5, x)` | 0 to 1 | 3.58 | 2.61 | 1.4 |
| `stdtr(5.0, x)` | −10 to 10 | 3.16 | 1.56 | 2.0 |
| `ellipk(m)` | −10 to 1 | 0.17 | 0.18 | 0.9 |
| `ellipe(m)` | −10 to 1 | 0.16 | 0.18 | 0.9 |
| `expi(x)` | −50 to 50 | 1.58 | 2.57 | 0.6 |
| `zeta(x)` | 1.01 to 50 | 1.31 | 0.34 | 3.9 |
| `eval_legendre(10, x)` | −1 to 1 | 0.24 | 0.27 | 0.9 |
| `hyp1f1(1.5, 2.5, x)` | −50 to 50 | 1.81 | 7.19 | 0.3 |
| `hyp2f1(0.5, 1.5, 2.5, x)` | −10 to 0.99 | 5.98 | 5.13 | 1.2 |

Baryon is faster for `hyp1f1`, `jv`, `airy` and `expi`, level for `kv`, `ellipk`, `ellipe` and
`eval_legendre`, and slower for the rest, by up to 3.9 times, for `zeta` and `ndtri`. The slower
functions use general methods held to the accuracy above, such as series and iterations, where
SciPy's Cephes fits one short rational or polynomial formula to each range of the argument. Roadmap
3.18.17 speeds them up, slowest first, with formulas fitted for Baryon where they keep the accuracy,
and with shorter routes to the same answer:
`ellipk`, `ellipe`, `erf`, `erfc` and `j0` took 0.6, 1.5, 0.5, 0.4 and 1.2 seconds before it;
`ndtri`, which now starts from a fitted value, 3.7; `zeta`, which now stops summing once the
rest cannot change the result, 7.0; `gamma`, which now sums its series in four chains at once,
0.33; `hyp1f1`, which below 0 now goes straight to Kummer's transformation, whose terms do not
cancel, 20.5; and `hyp2f1`, which below 0 now tries Pfaff's transformation first, 13.5.
[`bench/special`](../../bench/special/README.md) holds both programs, to run on your own machine.

## Where to go next

- [Special functions](../reference/special-functions.md) in the reference lists every function,
  its tested range and accuracy, and every difference from SciPy.
- [Arrays and numbers](arrays-and-numbers.md) covers `math`, `stats`, `linalg` and `fft`.
- [What builds today](../reference/status.md) says what else the compiler builds.
