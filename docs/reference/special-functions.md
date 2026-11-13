# Special functions

The `special` module holds SciPy's special functions with `scipy.special`'s names, written in
Baryon (roadmap 3.18, [decision 0054](../decisions/0054-scipy-module-names.md)). A program
moved from Python changes `from scipy.special import` to `from special import` and nothing else.

```python
from special import gamma, gammaln, psi


def main():
    print(gamma(5.0))       # 24.0, which is 4!
    print(gammaln(171.5))   # 709.1431630309283, where gamma itself is about 1e308
    print(psi(1.0))         # -0.5772156649015329, minus Euler's constant
```

Every function takes `f64` arguments and returns an `f64`. Arrays are not taken yet: SciPy's
functions apply element by element to arrays, and Baryon's will once the library can say so
(roadmap 3.18.13, idea I.61). Until then, call the function in a loop.

Nothing here calls SciPy, Cephes or the C library's `tgamma`, `lgamma` or `erf`. Each function
is built from the series and recurrences below and the elementary functions `exp`, `log`,
`pow`, `sin` and `cos`.

## Accuracy

Each function is tested against values computed with mpmath to 50 digits, at a few hundred
arguments spread over its range, about 2,000 in all
([the fixtures](../../crates/driver/tests/fixtures/special/README.md)).
The error is counted in units in the last place (ULP): the gap between adjacent `f64` values at
the true result. One ULP is a relative error of about 1.1e-16 to 2.2e-16.

| Function | Tested to | Worst seen | SciPy 1.18's worst at the same arguments |
| --- | --- | --- | --- |
| `gamma` | 8 ULP | 4 ULP | 4 ULP |
| `rgamma` | 8 ULP | 3 ULP | 0 from 171.62 up, where the true value is still above 1e-311 |
| `gammaln` | 8 ULP of the larger of the result and 1 | 1 ULP | 2 ULP |
| `psi` | 8 ULP of the larger of the result and 1 | 4 ULP | 13 ULP |
| `polygamma` | 8 ULP | 6 ULP | 5 ULP |
| `beta` | 8 ULP plus 2 for each unit of the result's logarithm | 202 ULP at `beta(176.6, 301.9)`, about 3e-138 | 5,120 ULP |
| `betaln` | 8 ULP of the larger of the result and 1 | 2 ULP | 1.8 million ULP at `betaln(0.297, 561253.8)` |
| `poch` | 8 ULP | 6 ULP | 72,818 ULP |
| `erf` | 8 ULP | 1 ULP | 1 ULP |
| `erfc` | 8 ULP | 3 ULP | 3 ULP, and 486 at 24.93; 0 from 26.64 up, where the true value is still above 1e-310 |
| `erfcx` | 8 ULP | 1 ULP | 443 ULP at −25.8 |
| `erfinv`, `erfcinv` | 8 ULP | 2 ULP | 3 ULP |
| `ndtr` | 8 ULP | 3 ULP | 0 from −37.68 down, where the true value is still above 1e-310 |
| `ndtri` | 8 ULP | 2 ULP | 2 ULP |
| `log_ndtr` | 8 ULP | 3 ULP | 52 ULP at 9.76 |
| `dawsn` | 8 ULP | 6 ULP | 10 ULP |
| `j0`, `j1` | 16 ULP of the larger of the value and its envelope | 7 ULP | 4 ULP |
| `y0`, `y1` | 16 ULP of the larger of the value and its envelope | 10 ULP | 17 ULP; 35 million at `y1(6.8e7)` |
| `jn` | 16 ULP of the larger of the value and its envelope | 8 ULP | 152 ULP at `jn(30, 0.0013)` |
| `yn` | 32 ULP of the larger of the value and its envelope | 18 ULP at order 30 | 12 ULP |
| `i0`, `i1`, `i0e`, `i1e` | 16 ULP | 8 ULP | 8 ULP |
| `k0`, `k1`, `k0e`, `k1e` | 16 ULP | 4 ULP | 4 ULP |
| `kn` | 16 ULP | 7 ULP | 67 ULP at `kn(30, 3.8)` |

Where a function crosses zero, no formula keeps all of its relative precision there: the
result is a small difference of two numbers near 1. Those functions are held to a precision of
8 ULP of 1 instead, the precision of the terms they are formed from. `beta` above 171 is the
exponential of `betaln`, so each ULP of the logarithm's absolute error becomes its size in
relative error; a logarithm of −316 costs about 200 ULP.

## The gamma family

| Function | Result | SciPy |
| --- | --- | --- |
| `gamma(x)` | Γ(x), which is (x − 1)! at whole numbers | `scipy.special.gamma` |
| `rgamma(x)` | 1/Γ(x), finite where Γ overflows | `scipy.special.rgamma` |
| `gammaln(x)` | log\|Γ(x)\| | `scipy.special.gammaln` |
| `loggamma(x)` | log Γ(x) for x ≥ 0, NaN below | `scipy.special.loggamma` of a real argument |
| `gammasgn(x)` | The sign of Γ(x), 1.0 or −1.0 | `scipy.special.gammasgn` |
| `psi(x)`, `digamma(x)` | ψ(x) = Γ′(x) / Γ(x) | `scipy.special.psi` |
| `polygamma(n, x)` | The `n`th derivative of ψ, with `n` an `int` | `scipy.special.polygamma` |
| `beta(a, b)` | B(a, b) = Γ(a) Γ(b) / Γ(a + b) | `scipy.special.beta` |
| `betaln(a, b)` | log\|B(a, b)\| | `scipy.special.betaln` |
| `poch(z, m)` | The rising factorial Γ(z + m) / Γ(z) | `scipy.special.poch` |

How each is computed:

- **Near 1 and 2**, from −0.5 to 2.5, 1/Γ(x) − 1 is summed from its Taylor series around 1 or
  2, about 22 terms. `gamma` is 1 over 1 plus it, and `gammaln` is minus its `log1p`, so
  `gammaln` keeps its relative precision near its zeros at 1 and 2.
- **From 2.5 to 10** the recurrence Γ(x + 1) = x Γ(x) brings the argument down into that
  range, and whole numbers up to 23 give the exact factorial.
- **From 10 up**, Stirling's series with ten Bernoulli terms. `gamma` raises x to the power
  x/2 − 1/4 twice, so neither power overflows before e^(−x) scales it down.
- **Below 0**, the reflection formula Γ(x) Γ(−x) = −π / (x sin(πx)). The sine is taken of x
  reduced modulo 2 first, so it is exact at whole and half numbers and keeps its precision far
  from 0, where `sin(pi * x)` loses every digit.
- **`psi`** sums its Taylor series around its zero at 1.4616 between 1 and 2, so it keeps its
  relative precision there, and uses the recurrence ψ(x + 1) = ψ(x) + 1/x to reach that range
  from below 10, and its asymptotic series above.
- **`polygamma`** is (−1)ⁿ⁺¹ n! ζ(n + 1, x), with the Hurwitz zeta function summed by the
  Euler–Maclaurin formula.
- **A function of a sum**, such as Γ(a + b) in `beta` or Γ(z + m) in `poch`, adds the error of
  rounding `a + b`, found exactly by Knuth's two-sum, times the derivative. Without it the
  rounding of the sum would cost up to ψ(a + b) (a + b) ULP.
- **`betaln` and `poch` with large arguments** take the difference of two log-gamma values
  from Stirling's series directly, as a `log1p`, so `betaln(1.0, 1e300)` and `poch(1e10, 0.5)`
  keep their digits instead of subtracting two numbers near 700.

### Poles, zeros and infinities

At the points where Γ has a pole or the result has no limit, each function gives what SciPy
gives, so a program's NaNs and infinities land where they did:

| Call | Result |
| --- | --- |
| `gamma(0.0)`, `gamma(-0.0)` | `inf`, `-inf` |
| `gamma(-1.0)`, `gamma(-inf)` | `nan` |
| `gamma(172.0)` | `inf`: Γ overflows above 171.62 |
| `rgamma(-2.0)`, `rgamma(inf)`, `rgamma(-inf)` | `0.0` |
| `gammaln(0.0)`, `gammaln(-1.0)`, `gammaln(inf)` | `inf` |
| `gammaln(-inf)` | `-inf` |
| `loggamma(-0.5)` | `nan` |
| `gammasgn(-1.0)` | `nan` |
| `psi(0.0)`, `psi(-0.0)` | `-inf`, `inf` |
| `psi(-1.0)` | `nan` |
| `polygamma(n, 0.0)` | `inf` for odd `n`, `-inf` for even |
| `polygamma(-1, x)` | `nan` |
| `beta(0.0, 1.0)`, `beta(-1.0, 2.0)` | `inf` |
| `beta(-1.0, -1.0)` | `-inf`, SciPy's limit along whole numbers |
| `poch(3.0, inf)` | `inf` |

Two differ from SciPy, because SciPy's answer is not the function's value:

- `rgamma(-1e-320)` is `-1e-320`, where SciPy gives `-0.0`.
- `rgamma(x)` from 171.62 to about 178 is the true value, above 1e-308, where SciPy gives 0.

## The error functions

| Function | Result | SciPy |
| --- | --- | --- |
| `erf(x)` | The error function, (2/√π) ∫₀ˣ exp(−t²) dt | `scipy.special.erf` |
| `erfc(x)` | 1 − erf(x), with its full precision in the tail | `scipy.special.erfc` |
| `erfcx(x)` | exp(x²) erfc(x), finite where erfc underflows | `scipy.special.erfcx` |
| `erfinv(y)` | The `x` with erf(x) = y | `scipy.special.erfinv` |
| `erfcinv(q)` | The `x` with erfc(x) = q | `scipy.special.erfcinv` |
| `ndtr(x)` | The standard normal cumulative distribution, Φ(x) | `scipy.special.ndtr` |
| `ndtri(p)` | The `x` with Φ(x) = p | `scipy.special.ndtri` |
| `log_ndtr(x)` | log Φ(x), finite far into the lower tail | `scipy.special.log_ndtr` |
| `dawsn(x)` | Dawson's integral, exp(−x²) ∫₀ˣ exp(t²) dt | `scipy.special.dawsn` |

```python
from special import erfc, ndtr, ndtri


def main():
    print(erfc(10.0))       # 2.088487583762545e-45, where 1 - erf(10.0) is 0
    print(ndtr(1.96))       # 0.9750021048517795
    print(ndtri(0.975))     # 1.959963984540054
```

How each is computed:

- **`erf` below 0.5** sums its Taylor series in x², 14 terms, so it keeps its relative precision
  near 0.
- **`erfcx` from 0.5 to 3** sums the Taylor series of erfcx itself around the middle of each
  half-unit interval, 16 to 19 terms; **from 3 up**, Laplace's continued fraction, 40 levels at
  3 and 10 from 8 up.
- **`erfc` from 0.5 up** is exp(−x²) erfcx(x). The square x² is carried as two `f64`s, its
  rounded value and the rounding error found with `fma`, so the rounding does not reach the
  exponential; a rounded square alone would cost 2x² ULP, about 1,400 at x = 26. `erf` above
  0.5 is 1 − erfc(x), and below 0 both use their symmetry.
- **`ndtr`** is erfc(−x/√2) / 2 with −x/√2 carried as two `f64`s in the same way, so the lower
  tail keeps its digits down to the underflow near −38.5.
- **`erfinv`** refines a first guess by Halley's method. Above |y| = 0.5 it is
  `erfcinv(1 − |y|)`, where the subtraction is exact. **`erfcinv`** below 0.5 solves
  −x² + log erfcx(x) = log q by Newton's method, which stays finite for the smallest `q`.
  `ndtri(p)` is −√2 erfcinv(2p).
- **`log_ndtr`** is log1p(−Φ(−x)) above 0, log Φ(x) down to −20, and log(erfcx(u) / 2) − u²
  with u = −x/√2 below, so it is finite where Φ underflows.
- **`dawsn`** below 8 is the continued fraction x / (1 + 2x² / (3 − 4x² / (5 + ⋯))), from a
  depth of 24 levels plus 12 for each unit of |x|, and from 8 out its asymptotic series
  (1/(2x)) Σ (2k − 1)!! / (2x²)ᵏ.

At the ends of their ranges they give what SciPy gives: `erf(±inf)` is ±1, `erfc(inf)` 0,
`erfcx(-inf)` ∞, `erfinv(±1.0)` ±∞, `erfcinv(0.0)` ∞ and `erfcinv(2.0)` −∞, `ndtri(0.0)` −∞
and `ndtri(1.0)` ∞, `log_ndtr(-inf)` −∞, and NaN outside each function's domain.

## Bessel functions of whole order

| Function | Result | SciPy |
| --- | --- | --- |
| `j0(x)`, `j1(x)`, `jn(n, x)` | Bessel functions of the first kind, Jₙ(x) | `scipy.special.j0`, `j1`, `jn` |
| `y0(x)`, `y1(x)`, `yn(n, x)` | Bessel functions of the second kind, Yₙ(x), for x > 0 | `scipy.special.y0`, `y1`, `yn` |
| `i0(x)`, `i1(x)` | Modified Bessel functions of the first kind, Iₙ(x) | `scipy.special.i0`, `i1` |
| `i0e(x)`, `i1e(x)` | Iₙ(x) e^−\|x\|, finite where Iₙ overflows | `scipy.special.i0e`, `i1e` |
| `k0(x)`, `k1(x)`, `kn(n, x)` | Modified Bessel functions of the second kind, Kₙ(x), for x > 0 | `scipy.special.k0`, `k1`, `kn` |
| `k0e(x)`, `k1e(x)` | Kₙ(x) eˣ, finite where Kₙ underflows | `scipy.special.k0e`, `k1e` |

The order `n` of `jn`, `yn` and `kn` is an `int`. Bessel functions of any real order, `jv`,
`yv`, `iv` and `kv`, come in roadmap 3.18.5.

```python
from special import j0, jn


def main():
    # The first zero of J₀, rounded to f64: the true value there is -6.1e-17.
    print(j0(2.404825557695773))    # 0.0, within a ULP of the envelope
    print(jn(5, 2.0))               # 0.007039629755871685
```

How each is computed:

- **J₀ and J₁** sum their power series up to |x| = 2, where the first term is the largest.
  From 2 to 25, Miller's backward recurrence Jₖ₋₁ = (2k/x) Jₖ − Jₖ₊₁ runs down from an order
  about x/2 + 15 + √x above the one wanted, where the starting guess no longer matters, and the
  ladder is scaled so that J₀ + 2 Σ J₂ₖ = 1. From 25 up, Hankel's asymptotic expansion, whose
  phase is formed from `cos(x)` and `sin(x)` of the exact argument.
- **Y₀ and Y₁** use their logarithmic series up to 2, Neumann's series over the same ladder of J
  from 2 to 25, and Hankel's expansion beyond.
- **`jn`** recurs upward from J₀ and J₁ while the order is below |x|, where that is stable, and
  runs Miller's recurrence above. **`yn`** and **`kn`** recur upward from orders 0 and 1, which
  is stable for Y and K at every order.
- **I₀ and I₁** sum their power series, all of whose terms are positive, below 20, and their
  asymptotic series above; `i0` and `i1` are the scaled functions times e^|x|.
- **K₀ and K₁** use their logarithmic series up to 0.5. From 0.5 to 20 they are integrals,
  Kₙ(x) eˣ = ∫₀^∞ exp(−2x sinh²(t/2)) cosh(nt) dt, by the trapezoid rule with steps of 0.1:
  the integrand is analytic and falls faster than exponentially, so the rule's error is about
  exp(−π²/0.1), far below a ULP, in 18 to 80 points. Above 20, the asymptotic series.

### Accuracy near zeros

J and Y oscillate, and near each zero no formula keeps the value's relative precision: it is
the small difference of two terms of the size of the envelope √(2/(πx)). Away from 0 they are
tested to a few ULP of the larger of the value and the envelope, which is what SciPy reaches
too. Close to 0, J₀, J₁ and their series are relative.

### At the ends

`y0`, `y1`, `yn`, `k0`, `k1` and `kn` are −∞ or ∞ at 0 and NaN below 0, as SciPy gives. At
infinity each function gives its limit, 0 for J and Y and ∞ for I, where SciPy 1.18 gives NaN
for `j0`, `j1`, `jn`, `y0`, `i0` and `i1`.

## Not built yet

The rest of `scipy.special` comes in the sub-rows of roadmap 3.18: the incomplete gamma and beta
functions, Bessel functions of real and complex order, Airy, Struve and Kelvin functions, hypergeometric and
elliptic functions, orthogonal polynomials, exponential integrals and zeta, and the
distribution functions. `loggamma` of a complex argument comes with the complex Bessel
functions (3.18.6). See [What builds today](status.md).
