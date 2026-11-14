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
arguments spread over its range, about 18,500 in all
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
| `gammainc`, `gammaincc` | 16 ULP for each unit of the result's logarithm, over `a` below 1 | 465 ULP at a result of 4e-146 | 1,441 ULP; 0 at `gammaincc(401.5, 1695.6)`, where the true value is 8.7e-314 |
| `gammaincinv`, `gammainccinv` | The same | 8 ULP | 109 ULP |
| `betainc`, `betaincc` | 32 ULP for each unit of the result's logarithm, over `a` below 1 | 369 ULP at a result of 1e-237 | 566 ULP for `betainc`; `betaincc` within 1 ULP |
| `betaincinv`, `betainccinv` | The same | 37 ULP at `a` = 0.03 | 1,303 ULP at `betaincinv(280.7, 0.67, 4.5e-144)` |
| `bdtr`, `bdtrc`, `nbdtr`, `nbdtrc` | 32 ULP for each unit of the result's logarithm | 1,185 ULP at a result of 3e-202 | 16,462 ULP at a result of 0.0037 |
| `bdtri`, `nbdtri` | The same | 4 ULP | 3e16 ULP: `bdtri(45, 852, 7.4e-22)` gives 0.9999999999999999 where the answer is 0.159 |
| `pdtr`, `pdtrc`, `chdtr`, `chdtrc`, `gdtr`, `gdtrc` | The same | 1,102 ULP at a result of 1e-297 | 6,633 ULP; 0 at `pdtr(510, 1902.5)`, where the true value is 1.7e-315 |
| `pdtri`, `chdtri` | The same | 7 ULP | 25 ULP |
| `fdtr`, `fdtrc`, `fdtri` | The same | 136 ULP at a result of 1e-35 | 298 ULP |
| `stdtr`, `stdtrit` | The same | 642 ULP at a result of 2e-116 | 320 ULP for `stdtrit`, 190 for `stdtr` |
| `kolmogorov`, `kolmogi` | The same | 395 ULP at a result of 3e-224 | 1 ULP |
| `smirnov`, `smirnovi` | The same | 179 ULP for `smirnovi` at p = 0.9977, where `d` depends on 1 − p | 1 ULP |
| `owens_t` | The same | 34 ULP at a result of 5e-24 | 1 ULP |
| `expit`, `log_expit`, `logit`, `xlogy`, `xlog1py`, `entr`, `rel_entr` | 8 ULP | 2 ULP | 1 ULP |
| `kl_div` | 8 ULP | 3 ULP | 8.8 billion ULP at `kl_div(0.6515198, 0.6515264)`, where x − y cancels |
| `binom`, `comb` | 8 ULP plus 2 for each unit of the result's logarithm | 18 ULP at a result of 5e160 | 8,523 ULP |
| `perm`, `factorial` | 8 ULP | 4 ULP | 7 ULP |
| `ellipk`, `ellipkm1`, `ellipe` | 8 ULP | 3 ULP | 2 ULP |
| `ellipkinc`, `ellipeinc` | 8 ULP | 4 ULP | 4 ULP |
| `elliprc`, `elliprd`, `elliprf`, `elliprg` | 8 ULP | 4 ULP | 7 ULP |
| `elliprj` | 8 ULP | 6 ULP | 116 ULP where `p` is far below the other arguments |
| `ellipj` | 128 ULP, of the larger of the value and 1 for sn, cn and dn | 96 ULP of the amplitude at m = 1 − 4e-11 | 13 billion ULP of sn at the same arguments |
| `eval_legendre`, `eval_chebyt`, `eval_chebyu`, `eval_laguerre`, `eval_genlaguerre`, `eval_jacobi`, `eval_gegenbauer` | 8 ULP for each degree, of the largest value the recurrence passes through | 29 ULP for `eval_gegenbauer` at degree 85 | 439 ULP for `eval_jacobi` at degree 85 |
| `eval_hermite`, `eval_hermitenorm` | The same | 156 ULP at degree 78 | 698 ULP for `eval_hermite` at degree 75 |
| `lpmv` | 64 ULP of the largest value the recurrence passes through | 24 ULP | 154 ULP |
| `sph_harm_y` | 8 ULP for each degree, of the largest value the recurrence passes through | 191 ULP at degree 46 | 148 ULP at the same arguments |
| `roots_legendre`, `roots_chebyt`, `roots_chebyu`, `roots_hermite`, `roots_hermitenorm`, `roots_laguerre`, `roots_genlaguerre`, `roots_jacobi`, `roots_gegenbauer`: nodes | 16 ULP | 7 ULP | 39 ULP for `roots_jacobi` at 150 nodes |
| The same: weights | 32 ULP for each node of the rule | 2,890 ULP for `roots_gegenbauer` at 150 nodes | 1.2 million ULP at the same arguments |
| `lambertw` | 8 ULP of the size of W, for each part | 4 ULP | 2.8e11 ULP: on branch −1 at 7e-10 above −1/e, SciPy's value is off by 6e-5 |

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

## Incomplete gamma and beta functions

| Function | Result | SciPy |
| --- | --- | --- |
| `gammainc(a, x)` | The regularized lower incomplete gamma function, P(a, x) = γ(a, x) / Γ(a) | `scipy.special.gammainc` |
| `gammaincc(a, x)` | Its complement, Q(a, x) = 1 − P(a, x), with its own precision in the tail | `scipy.special.gammaincc` |
| `gammaincinv(a, p)`, `gammainccinv(a, q)` | The `x` with P(a, x) = p, or with Q(a, x) = q | `scipy.special.gammaincinv`, `gammainccinv` |
| `betainc(a, b, x)` | The regularized incomplete beta function, Iₓ(a, b) | `scipy.special.betainc` |
| `betaincc(a, b, x)` | Its complement, 1 − Iₓ(a, b), with its own precision in the tail | `scipy.special.betaincc` |
| `betaincinv(a, b, p)` | The `x` with Iₓ(a, b) = p | `scipy.special.betaincinv` |
| `betainccinv(a, b, q)` | The `x` with 1 − Iₓ(a, b) = q, with its own precision when `q` is tiny | `scipy.special.betainccinv` |

P(a, x) is the probability that a gamma variable with shape `a` and scale 1 is below `x`, and
Iₓ(a, b) that a beta variable with shapes `a` and `b` is. They are the distribution functions
of the chi-squared, Poisson, Student's t and F distributions (roadmap 3.18.12).

```python
from special import betainc, gammainc, gammaincc, gammaincinv


def main():
    print(gammainc(2.0, 1.0))           # 0.2642411176571153, which is 1 − 2/e
    print(gammaincc(100.0, 200.0))      # 1.8438936497115686e-15, where 1 − P would be 0
    print(gammaincinv(2.0, 0.5))        # 1.6783469900166605, the median
    print(betainc(2.0, 3.0, 0.4))       # 0.5247999999999999
```

How each is computed:

- **P and Q** share a prefix xᵃ e⁻ˣ / Γ(a). Below x = a + 1, P is the series
  Σ xⁿ / ((a + 1)⋯(a + n)) times the prefix; above, Q is Legendre's continued fraction,
  evaluated by Lentz's method. The other is 1 minus the one computed. For `a` below 1 and `x`
  below 2, where P is near 1, both come from a series in which 1 − P has no cancellation.
- **Iₓ(a, b)** is the continued fraction of Abramowitz and Stegun 26.5.8 times the prefix
  xᵃ (1 − x)ᵇ / (a B(a, b)), below the mean (a + 1)/(a + b + 2); above it, the same fraction of
  the mirrored shapes gives the complement. For a shape below 1 with `x` close to its own end,
  where the value is near 1, a power series in `x` gives both the value and its complement.
- For shapes from 10 up, the prefixes are formed from log r − (r − 1) with r the ratio of `x`
  to the peak, so no two large logarithms cancel near the peak.
- **The inverses** start from Wilson and Hilferty's normal approximation or from the
  leading power of the series, and refine by Newton's method on the logarithm of the smaller
  of the value and its complement, so a target of 1e-300 is reached as surely as 0.5.
  `betaincinv` keeps a bracket and bisects whenever a Newton step leaves it.

### Accuracy in the tails

Each value is an exponential of a sum of logarithms, such as a log x − x − log Γ(a), times a
series. A ULP of absolute error in a sum of size L becomes L ULP of relative error in its
exponential, so a value of 1e-146, whose logarithm is −336, is held to 16 × 337 ULP; SciPy
loses the same digits there. A shape `a` below 1 divides the tolerance by `a`: the inverses
raise their target to the power 1/a, which multiplies its relative error by 1/a.

### At the ends

As SciPy gives: `gammainc(a, 0.0)` is 0 and `gammainc(a, inf)` 1; `gammainc(0.0, x)` is 1 for
`x` above 0; `gammaincinv(a, 1.0)` and `gammainccinv(a, 0.0)` are ∞; `betainc(a, b, 0.0)` is 0
and `betainc(a, b, 1.0)` 1; `betainc(0.0, b, x)` is 1 and `betainc(a, 0.0, x)` 0 for `x`
strictly between 0 and 1. A negative shape, an `x` outside the domain or a target outside
0 to 1 gives NaN.

## Distribution functions

The distribution functions of seven distributions, each a change of variables into the
incomplete gamma or beta function, as SciPy's `scipy.stats` builds on them. Counts `k` and `n`
are rounded down to whole numbers, as SciPy does.

| Function | Result | Computed as |
| --- | --- | --- |
| `bdtr(k, n, p)`, `bdtrc(k, n, p)` | The probability of at most `k`, or more than `k`, successes in `n` trials that each succeed with probability `p` | I₁₋ₚ(n − k, k + 1) and Iₚ(k + 1, n − k) |
| `bdtri(k, n, y)` | The `p` with `bdtr(k, n, p)` = y | |
| `nbdtr(k, n, p)`, `nbdtrc(k, n, p)` | The probability of at most `k`, or more than `k`, failures before the `n`th success | Iₚ(n, k + 1) and its complement |
| `nbdtri(k, n, y)` | The `p` with `nbdtr(k, n, p)` = y | |
| `pdtr(k, m)`, `pdtrc(k, m)` | The probability of at most `k`, or more than `k`, events of a Poisson process with mean `m` | Q(k + 1, m) and P(k + 1, m) |
| `pdtri(k, y)` | The `m` with `pdtr(k, m)` = y | |
| `chdtr(v, x)`, `chdtrc(v, x)` | The chi-squared distribution with `v` degrees of freedom, and its complement | P(v/2, x/2) and Q(v/2, x/2) |
| `chdtri(v, y)` | The `x` with `chdtrc(v, x)` = y | |
| `fdtr(a, b, x)`, `fdtrc(a, b, x)` | The F distribution with `a` and `b` degrees of freedom, and its complement | I_w(a/2, b/2) with w = ax/(b + ax) |
| `fdtri(a, b, y)` | The `x` with `fdtr(a, b, x)` = y | |
| `gdtr(a, b, x)`, `gdtrc(a, b, x)` | The gamma distribution with rate `a` and shape `b`, and its complement | P(b, ax) and Q(b, ax) |
| `stdtr(df, t)` | Student's t distribution with `df` degrees of freedom | ½ I_w(df/2, ½) with w = df/(df + t²) below 0 |
| `stdtrit(df, p)` | The `t` with `stdtr(df, t)` = p | |
| `kolmogorov(y)`, `kolmogi(p)` | The limit, for large samples, of the probability that the Kolmogorov-Smirnov statistic √n Dₙ exceeds `y`, and its inverse | Theta series |
| `smirnov(n, d)`, `smirnovi(n, p)` | The probability that the one-sided statistic Dₙ⁺ of a sample of `n` is at least `d`, and its inverse | Birnbaum and Tingey's exact sum |
| `owens_t(h, a)` | Owen's T function, (1/2π) ∫₀ᵃ exp(−h²(1 + x²)/2) / (1 + x²) dx, the bivariate normal probability over a wedge | Gauss-Legendre quadrature |

Each SciPy name is `scipy.special`'s.

```python
from special import bdtr, chdtri, stdtrit


def main():
    print(bdtr(3, 10, 0.5))     # 0.171875, the chance of at most 3 heads in 10 tosses
    print(chdtri(3.0, 0.05))    # 7.814727903251179, the 5% critical value with 3 degrees
    print(stdtrit(3.0, 0.975))  # 3.182446305283708, the two-sided 95% t value
```

Each complement is computed from its own side, never as 1 minus the function, so a p-value of
1e-200 keeps its digits. The variable that a distribution passes to the beta function is formed
with its complement beside it: for the F distribution, w = ax/(b + ax) and 1 − w = b/(b + ax),
each from one division. The inverses solve for whichever of w and 1 − w is smaller, so a large
F or t value keeps its precision too.

### At the ends

As SciPy gives: a probability outside 0 to 1, a negative count or `k` above `n` in `bdtr` gives
NaN; `bdtr(k, n, 0.0)` is 1 and `bdtrc(-1, n, p)` is 1; `pdtri(k, 0.0)` and `chdtri(v, 0.0)`
are ∞; `stdtr(inf, t)` is the normal distribution's `ndtr(t)`. Two differ from SciPy 1.18:
`pdtri(k, 1.0)` is 0, where SciPy gives NaN, since `pdtr(k, m)` is 1 only at m = 0; and
`stdtrit(df, 0.0)` is −∞, where SciPy gives ∞.

`kolmogorov` sums whichever of its two theta series falls fast: √(2π)/y Σ exp(−(2k − 1)²π²/(8y²))
for the distribution below 1, and 2 Σ (−1)ᵏ⁻¹ exp(−2k²y²) for the complement from 1 up. `smirnov` is
exact for every `n`, never an asymptotic formula: each of its n(1 − d) terms is a binomial
probability formed by Loader's saddle-point method, so `smirnov(1000000, 0.001)` costs about a
million exponentials. Above n = 10¹⁰, where the sum would take minutes, `smirnov` and `smirnovi`
give NaN; SciPy switches to an asymptotic series there. `owens_t` integrates over at most 12/h,
where the integrand has fallen below exp(−72) of its peak, with 20 Gauss-Legendre points; above `a`
= 1 it uses T(h, a) = ½(Q(h) + Q(ah)) − Q(h) Q(ah) − T(ah, 1/a), with Q the normal tail.

SciPy is within a ULP on these five; the functions here meet the same rule as the others.

## Everyday helpers

| Function | Result | SciPy |
| --- | --- | --- |
| `expit(x)` | The logistic sigmoid 1 / (1 + e⁻ˣ) | `scipy.special.expit` |
| `log_expit(x)` | log(expit(x)), finite where `expit` underflows | `scipy.special.log_expit` |
| `logit(p)` | log(p / (1 − p)), the inverse of `expit` | `scipy.special.logit` |
| `xlogy(x, y)`, `xlog1py(x, y)` | x log y and x log(1 + y), 0 when `x` is 0 | `scipy.special.xlogy`, `xlog1py` |
| `entr(x)` | The entropy term −x log x | `scipy.special.entr` |
| `rel_entr(x, y)` | The relative entropy term x log(x / y) | `scipy.special.rel_entr` |
| `kl_div(x, y)` | The Kullback-Leibler term x log(x / y) − x + y | `scipy.special.kl_div` |
| `binom(n, k)` | The binomial coefficient for real `n` and `k` | `scipy.special.binom` |
| `comb(n, k, exact=False, repetition=False)` | The number of ways to choose `k` of `n` | `scipy.special.comb` |
| `perm(n, k, exact=False)` | The number of ordered choices, n! / (n − k)! | `scipy.special.perm` |
| `factorial(n, exact=False)` | n!, and Γ(n + 1) away from whole numbers | `scipy.special.factorial` |

```python
from special import comb, expit, kl_div


def main():
    print(expit(2.0))                       # 0.8807970779778823
    print(comb(10, 3))                      # 120.0
    print(kl_div(1.0, 1.000001))            # 4.999996665846501e-13
```

`kl_div` keeps its digits when `x` and `y` are close, where the result is about
(x − y)²/(2y) and the three terms of its formula cancel: it is formed from t = (x − y)/y, whose
numerator is exact there. `binom` multiplies its factors for a whole `k` below 1,000, so whole
results are exact up to 2⁵³, and otherwise corrects each argument of Γ for the rounding of its
sum.

`exact=True` in `comb`, `perm` and `factorial` counts in integer arithmetic and returns the
count as an `f64`, where SciPy returns a Python `int` of any size: exact up to 2⁵³, and a count
past the `int` range, about 9.2e18, is an overflow like any other.

`logsumexp(a)` and `softmax(a)` take a rank-1 `Array[f64, 1]`, as `scipy.special.logsumexp` and
`softmax` do without their `axis`, `b` and `return_sign` keywords. Each takes the largest
element out first, so nothing overflows: `logsumexp(Array([1000.0, 1000.0]))` is
1000.6931471805599, where the plain formula would give ∞. `logsumexp` of an empty array is −∞.

## Exponential integrals, zeta and the dilogarithm

| Function | Result | SciPy |
| --- | --- | --- |
| `exp1(x)` | The exponential integral E₁(x) = ∫ₓ^∞ e⁻ᵗ / t dt | `scipy.special.exp1` |
| `expi(x)` | The exponential integral Ei(x), the principal value of ∫₋∞ˣ eᵗ / t dt | `scipy.special.expi` |
| `expn(n, x)` | The generalized exponential integral Eₙ(x) = ∫₁^∞ e^(−xt) / tⁿ dt, for a whole `n` | `scipy.special.expn` |
| `sici(x)` | The sine and cosine integrals, `(Si(x), Ci(x))` | `scipy.special.sici` |
| `shichi(x)` | The hyperbolic sine and cosine integrals, `(Shi(x), Chi(x))` | `scipy.special.shichi` |
| `fresnel(x)` | The Fresnel integrals, `(S(x), C(x))` in SciPy's order | `scipy.special.fresnel` |
| `zeta(x)`, `zeta(x, q)` | The Riemann zeta function ζ(x), or the Hurwitz zeta function ζ(x, q) | `scipy.special.zeta` |
| `zetac(x)` | ζ(x) − 1, with its own precision where ζ is near 1 | `scipy.special.zetac` |
| `spence(x)` | Spence's function ∫₁ˣ log t / (1 − t) dt, which is Li₂(1 − x) | `scipy.special.spence` |

```python
from special import exp1, fresnel, spence, zeta


def main():
    print(exp1(1.0))            # 0.21938393439552029
    s, c = fresnel(1.0)
    print(s, c)                 # 0.4382591473903548 0.7798934003768228
    print(zeta(2.0))            # 1.6449340668482264, which is π²/6
    print(spence(2.0))          # -0.8224670334241132
```

How each is computed:

- **E₁ and Eₙ** are power series up to 1 and e⁻ˣ times a continued fraction above, evaluated
  backwards from its tail so its rounding errors shrink. **Ei** is −E₁(−x) below 0, the series
  γ + log x + Σ xᵏ / (k k!) up to 40, all of whose terms are then positive, and its asymptotic
  series eˣ/x Σ k!/xᵏ above.
- **Si and Ci** are power series up to 2; above, Ci(x) − i Si(x) + iπ/2 = E₁(ix), the same
  continued fraction with a complex argument. **Shi and Chi** are power series of positive
  terms up to 2, and (Ei(x) ± E₁(x))/2 above.
- **S and C** are power series below 1.5; above, C + iS comes from the continued fraction of
  the complementary error function.
- **ζ(x)** is an Euler–Maclaurin sum from 0 up, and the reflection formula below 0, with
  log 2π carried as two `f64`s so that large negative arguments keep their digits. ζ(x, q) is
  the same sum from `q`. `zetac` sums from 2 for large `x`, so 1 is never subtracted from a
  number near 1.
- **Spence's function** is the dilogarithm Li₂(1 − x), summed as a series in −log(1 − y) from
  ½ to 2, where 1 − x is exact; below ½ and above 2, identities of the dilogarithm bring the
  argument into that range. So its zero at 1 keeps its relative precision.

All nine are held to 16 ULP of the mpmath value, at the 1,375 cases of
`integral_functions.txt`. Ei near its zero at 0.3725 and Chi near its zero at 0.5238 are held
to 16 ULP of 1, the size of the terms they are formed from, and Ci, which oscillates as
sin(x)/x does, to 16 ULP of the larger of its value and 1/x.

### At the ends

As SciPy gives: `exp1(0.0)` is ∞ and `exp1` of a negative argument NaN, where E₁ is complex;
`expn(n, 0.0)` is ∞ for `n` of 0 or 1 and 1/(n − 1) above; below 0, `Ci` and `Chi` are the real
parts SciPy gives, Ci(|x|) and Chi(|x|); `zeta(1.0)` is ∞, and the Hurwitz `zeta(x, q)` is NaN
for `x` below 1; `spence` is NaN below 0 and at ∞.

## The Lambert W function

| Function | Result | SciPy |
| --- | --- | --- |
| `lambertw(z, k=0, tol=1e-8)` | W_k(z), the solution w of w eʷ = z on branch `k`, a `complex128` | `scipy.special.lambertw` |

W turns up wherever an unknown sits both outside and inside an exponential: the time a
population with delayed growth takes to reach a size, the current through a diode and a
resistor in series, the Wien displacement constant. Every branch `k` is a whole number; branch
0 is the principal one. For real `z` from −1/e up, branch 0 is real and at least −1, and for
real `z` from −1/e to 0, branch −1 is real and at most −1; all other values are complex. A real
argument converts to `complex128`, and the value is always a `complex128`, as in SciPy: take
`.real` for a real branch.

```python
from special import lambertw


def main():
    w = lambertw(1.0)
    print(w.real)                         # 0.5671432904097838, the omega constant
    print(lambertw(-0.25, -1).real)       # -2.1532923641103494
    print(lambertw(complex(1.0, 2.0), 3)) # (-2.1072075066264553+18.27108543113687j)
```

The value is found as SciPy finds it: a first guess from a Padé approximant near 0, a series
near the branch point −1/e, or the asymptotic log z + 2πik − log(log z + 2πik) elsewhere, then
Halley's method until a step is at most `tol` times the value. Each step triples the number of
correct digits, so the value is within a few ULP when `tol` stops it.

Within 0.00184 of −1/e, on branch 0 and on branch −1 for real `z`, the iteration cannot reach
full precision: its residual w eʷ − z is a difference of two numbers near −1/e. There
`lambertw` sums 16 terms of the series in p = ±√(2e(z + 1/e)) instead, with z + 1/e computed
exactly, and keeps every digit. **This differs from SciPy**, whose iteration loses up to half
the digits there and, on branch −1 at 7e-10 above −1/e, gives −1.0000000060 where the answer is
−1.0000633.

`lambertw` is held to 8 ULP of |W| in each part, at the 1,050 cases of `lambertw.txt`:
branches −3 to 3 and 40, real arguments on both real branches from the branch point out to
1e300, and complex arguments from 1e-10 to 1e10 in size.

### At the ends

As SciPy gives: `lambertw(0.0)` is 0, and 0 on any other branch is −∞; NaN in either part
gives `z` back; an infinite real part gives `z` + 2πik i, with π more for −∞. Where 100 steps of
Halley's method do not settle, the value is NaN in both parts, where SciPy warns and gives NaN.

## Elliptic integrals and Jacobi's elliptic functions

| Function | Result | SciPy |
| --- | --- | --- |
| `ellipk(m)` | The complete elliptic integral of the first kind, K(m) = ∫₀^(π/2) dθ / √(1 − m sin²θ) | `scipy.special.ellipk` |
| `ellipkm1(p)` | K(1 − p), for `p` near 0 | `scipy.special.ellipkm1` |
| `ellipe(m)` | The complete elliptic integral of the second kind, E(m) = ∫₀^(π/2) √(1 − m sin²θ) dθ | `scipy.special.ellipe` |
| `ellipkinc(phi, m)` | The incomplete integral of the first kind, F(φ, m) = ∫₀^φ dθ / √(1 − m sin²θ) | `scipy.special.ellipkinc` |
| `ellipeinc(phi, m)` | The incomplete integral of the second kind, E(φ, m) = ∫₀^φ √(1 − m sin²θ) dθ | `scipy.special.ellipeinc` |
| `ellipj(u, m)` | Jacobi's elliptic functions `(sn, cn, dn, ph)`, ph being the amplitude φ with F(φ, m) = u | `scipy.special.ellipj` |
| `elliprf(x, y, z)` | Carlson's R_F(x, y, z) = ½ ∫₀^∞ dt / √((t + x)(t + y)(t + z)) | `scipy.special.elliprf` |
| `elliprd(x, y, z)` | Carlson's R_D(x, y, z) = (3/2) ∫₀^∞ dt / ((t + z) √((t + x)(t + y)(t + z))) | `scipy.special.elliprd` |
| `elliprj(x, y, z, p)` | Carlson's R_J(x, y, z, p) = (3/2) ∫₀^∞ dt / ((t + p) √((t + x)(t + y)(t + z))) | `scipy.special.elliprj` |
| `elliprc(x, y)` | Carlson's R_C(x, y) = R_F(x, y, y) | `scipy.special.elliprc` |
| `elliprg(x, y, z)` | Carlson's R_G(x, y, z), the mean of √(x sin²θ cos²φ + y sin²θ sin²φ + z cos²θ) over the sphere | `scipy.special.elliprg` |

The parameter is m = k², as in SciPy, not the modulus k. A pendulum swinging to angle θ₀ has
period 4√(L/g) K(sin²(θ₀/2)); an ellipse with semi-axes a ≥ b has perimeter 4a E(1 − b²/a²).

```python
from math import sin
from special import ellipe, ellipj, ellipk


def main():
    swing = 2.0                                    # radians, about 115 degrees
    print(4.0 * ellipk(sin(swing / 2.0) ** 2))     # 8.349752926918494, the period over √(L/g)
    print(4.0 * 3.0 * ellipe(1.0 - (2.0 / 3.0) ** 2))  # 15.865439589290586, the perimeter
    sn, cn, dn, ph = ellipj(0.5, 0.3)
    print(sn * sn + cn * cn)                       # 1.0
```

How each is computed:

- **Carlson's integrals** use Carlson's duplication theorem (Carlson 1995): each step moves the
  arguments four times closer together without changing the integral, and a fifth-order Taylor
  series finishes once they agree to about 1e-16. `elliprj` writes each step's 1 + e as
  2√p (p + λ)/d. The usual form subtracts two nearly equal numbers when `p` is far below the
  other arguments. That is where SciPy's `elliprj` loses 116 ULP; this one stays within 6.
  `elliprg` is ½[z R_F − (x − z)(y − z) R_D/3 + √(xy/z)] with z the middle argument, so the
  three terms never cancel. For `y` below 0, `elliprc` is Cauchy's principal value. For `p`
  below 0, so is `elliprj` (Carlson 1995, equation 4.3).
- **The complete integrals**: K(m) = R_F(0, 1 − m, 1) and E(m) = 2 R_G(0, 1 − m, 1). `ellipkm1`
  takes `p` directly, so K stays exact as `p` goes to 1e-300.
- **The incomplete integrals** reduce φ to [−π/2, π/2] plus a multiple of π, which adds a
  multiple of 2K or 2E. F is sin φ R_F(cos²φ, Δ², 1), with Δ² = 1 − m sin²φ formed as
  cos²φ + (1 − m) sin²φ so it never cancels. For m from 0 up, E uses DLMF 19.25.10, whose
  terms all have one sign. The textbook form subtracts two large numbers near m = 1 and
  φ = π/2.
- **`ellipj`** takes the arithmetic-geometric mean of 1 and √(1 − m), then descending Landen
  steps back to the amplitude (Abramowitz and Stegun 16.4). SciPy switches to rough
  approximations near m = 0 and m = 1. Those lose up to 13 billion ULP at m = 1 − 4e-11, where
  this loses 96 ULP of the amplitude. The first Landen step there takes the arcsine of a number
  near 1, which magnifies its rounding.

The integrals are held to 8 ULP of the mpmath value, and `ellipj` to 128 ULP, at the 1,860 cases
of `elliptic_functions.txt`. Each of sn, cn and dn is measured against the larger of its value
and 1, since all three come from the sine and cosine of the amplitude.

### At the ends

As SciPy gives: `ellipk(1.0)` is ∞, `ellipe(1.0)` is 1, and both are NaN above 1. At m = −∞,
`ellipk` is 0 and `ellipe` is ∞. `ellipkinc` and `ellipeinc` are NaN for every `m` above 1,
even where m sin²φ is below 1 and the integral is real; that matches SciPy. They are ∞ with
the sign of φ when φ is infinite. `ellipj` is NaN in all four values for `m` outside [0, 1] or
an infinite `u`. Carlson's integrals are NaN for a negative argument, ∞ where two arguments are
0, and 0 where one is ∞. `elliprc(x, 0.0)` and `elliprj(x, y, z, 0.0)` are NaN, as SciPy
gives, although the integrals are infinite.

## Orthogonal polynomials and spherical harmonics

| Function | Result | Orthogonal on, with weight | SciPy |
| --- | --- | --- | --- |
| `eval_legendre(n, x)` | The Legendre polynomial Pₙ(x) | [−1, 1], 1 | `scipy.special.eval_legendre` |
| `eval_chebyt(n, x)` | The Chebyshev polynomial of the first kind Tₙ(x) = cos(n acos x) | [−1, 1], 1/√(1 − x²) | `scipy.special.eval_chebyt` |
| `eval_chebyu(n, x)` | The Chebyshev polynomial of the second kind Uₙ(x) | [−1, 1], √(1 − x²) | `scipy.special.eval_chebyu` |
| `eval_hermite(n, x)` | The physicists' Hermite polynomial Hₙ(x) | (−∞, ∞), e^(−x²) | `scipy.special.eval_hermite` |
| `eval_hermitenorm(n, x)` | The probabilists' Hermite polynomial Heₙ(x) | (−∞, ∞), e^(−x²/2) | `scipy.special.eval_hermitenorm` |
| `eval_laguerre(n, x)` | The Laguerre polynomial Lₙ(x) | [0, ∞), e⁻ˣ | `scipy.special.eval_laguerre` |
| `eval_genlaguerre(n, alpha, x)` | The generalized Laguerre polynomial Lₙ^(α)(x), for α above −1 | [0, ∞), xᵅ e⁻ˣ | `scipy.special.eval_genlaguerre` |
| `eval_jacobi(n, alpha, beta, x)` | The Jacobi polynomial Pₙ^(α,β)(x) | [−1, 1], (1 − x)ᵅ (1 + x)ᵝ | `scipy.special.eval_jacobi` |
| `eval_gegenbauer(n, alpha, x)` | The Gegenbauer polynomial Cₙ^(α)(x) | [−1, 1], (1 − x²)^(α − ½) | `scipy.special.eval_gegenbauer` |
| `lpmv(m, v, x)` | The associated Legendre function Pᵥᵐ(x), with the Condon–Shortley phase (−1)ᵐ | | `scipy.special.lpmv` |
| `sph_harm_y(n, m, theta, phi)` | The spherical harmonic Yₙᵐ(θ, φ), θ the polar angle, a `complex128` | The sphere | `scipy.special.sph_harm_y` |

The degree `n` of the `eval_` functions is an `int`. **This differs from SciPy**, which also
takes a float degree through the hypergeometric function; that comes with roadmap 3.18.8. For
the same reason `lpmv` takes a whole degree `v`, as an `f64`. For now it gives NaN when `v` is
not a whole number. `sph_harm_y` gives the value alone. SciPy's `diff_n` keyword, which adds
derivatives, is not built.

```python
from math import pi
from special import eval_hermite, eval_legendre, sph_harm_y


def main():
    print(eval_legendre(2, 0.5))         # -0.125, which is (3x² − 1)/2
    print(eval_hermite(4, 1.0))          # -20.0
    y = sph_harm_y(2, 1, pi / 3.0, 0.0)
    print(y.real)                        # -0.33452327177864466
```

Each is its three-term recurrence in the degree, run up from degrees 0 and 1. On each
polynomial's interval of orthogonality that recurrence is stable. Near one of the polynomial's
zeros, though, the value is a small difference of large terms, so the error is a few ULP of the
largest value the recurrence passes through, not of the value. The tests measure it that way,
allowing 8 ULP for each degree.

`sph_harm_y` carries the normalized functions √((2n + 1)/(4π) (n − m)!/(n + m)!) Pₙᵐ(cos θ)
through their own recurrence, starting from sinᵐθ. No factorial is ever formed, so degrees in
the thousands neither overflow nor underflow before the value does. `lpmv` runs the
recurrence of Pᵥᵐ itself, as SciPy does. Its value overflows to ±∞ between orders 140 and
160, where SciPy gives NaN.

All eleven are held to their tolerances at the 1,487 cases of `orthogonal_polynomials.txt`.

### At the ends

As SciPy gives:

- `eval_legendre(-n - 1, x)` is Pₙ(x).
- `eval_chebyt(-n, x)` is Tₙ(x), and `eval_chebyu(-n, x)` is −Uₙ₋₂(x).
- `eval_hermite` and `eval_hermitenorm` are NaN for a negative degree. `eval_laguerre`,
  `eval_genlaguerre`, `eval_jacobi` and `eval_gegenbauer` are 0 there.
- `eval_genlaguerre` is NaN for α of −1 or below.
- `eval_gegenbauer` is 0 at α = 0, even at degree 0.
- `eval_legendre` at x = ±∞ is ±∞ to the power n.
- `lpmv` is NaN for |x| above 1. For |m| above v, it is 0 when m is positive and NaN when m is
  negative.
- `sph_harm_y` is 0 for |m| above n.

## Gauss quadrature

An n-point Gauss rule turns an integral into a weighted sum, ∫ f(x) w(x) dx ≈ Σ wᵢ f(xᵢ), and is
exact when f is a polynomial of degree below 2n. Each function returns the nodes xᵢ, in
increasing order, and the weights wᵢ, as two `Array[f64, 1]` of length n. Integration
(roadmap 3.19) builds on them.

| Function | Integral | SciPy |
| --- | --- | --- |
| `roots_legendre(n)` | ∫₋₁¹ f(x) dx | `scipy.special.roots_legendre` |
| `roots_chebyt(n)` | ∫₋₁¹ f(x)/√(1 − x²) dx | `scipy.special.roots_chebyt` |
| `roots_chebyu(n)` | ∫₋₁¹ f(x) √(1 − x²) dx | `scipy.special.roots_chebyu` |
| `roots_hermite(n)` | ∫ f(x) e^(−x²) dx over the real line | `scipy.special.roots_hermite` |
| `roots_hermitenorm(n)` | ∫ f(x) e^(−x²/2) dx over the real line | `scipy.special.roots_hermitenorm` |
| `roots_laguerre(n)` | ∫₀^∞ f(x) e⁻ˣ dx | `scipy.special.roots_laguerre` |
| `roots_genlaguerre(n, alpha)` | ∫₀^∞ f(x) xᵅ e⁻ˣ dx, for α above −1 | `scipy.special.roots_genlaguerre` |
| `roots_jacobi(n, alpha, beta_)` | ∫₋₁¹ f(x) (1 − x)ᵅ (1 + x)ᵝ dx, for α and β above −1 | `scipy.special.roots_jacobi` |
| `roots_gegenbauer(n, alpha)` | ∫₋₁¹ f(x) (1 − x²)^(α − ½) dx, for α above −½ | `scipy.special.roots_gegenbauer` |

```python
from math import exp, pi, sqrt
from special import roots_hermite, roots_legendre


def main():
    x, w = roots_legendre(5)
    total = 0.0
    for i in range(5):
        total += w[i] * exp(x[i])
    print(total)                         # 2.3504023864628256, e − 1/e to 9 digits

    x, w = roots_hermite(3)
    moment = 0.0
    for i in range(3):
        moment += w[i] * x[i] ** 4
    print(moment, 3.0 * sqrt(pi) / 4.0)  # 1.329340388179137 1.329340388179137: exact
```

The third argument of `roots_jacobi` is named `beta_`, so that inside the function the name
`beta` still means the beta function. A call that passes it by position, as almost every call
does, reads as SciPy's. SciPy's `mu=True`, which also returns the integral of the weight, is
not built.

**How they work.** The nodes are the eigenvalues of the family's Jacobi matrix, the symmetric
tridiagonal matrix of its three-term recurrence (Golub and Welsch, 1969), found by the
implicit QL method in about 30n² operations. Two steps of Newton's method on the polynomial
then sharpen each node, as SciPy does. Laguerre's polynomials are evaluated there in the
difference form SciPy's `eval_genlaguerre` uses, which keeps the relative precision of nodes
near 0. Each weight is μ₀ / Σ qₖ(xᵢ)², where the qₖ are the orthonormal polynomials and μ₀ is
the integral of the weight function. That is a sum of positive terms, so even a weight of
1e-300 keeps its relative precision. Where the qₖ would pass the range of `f64`, far out in
the tail of a rule of a few hundred nodes, the recurrence rescales itself by 2⁵⁰⁰, so a weight
underflows to 0 only where the true weight does. Rules for even weight functions have nodes
exactly opposite in pairs and weights equal in pairs, as SciPy makes them. Chebyshev's two
rules have closed forms.

**Weights differ from SciPy's in the last digits.** SciPy rescales its weights so that they sum
to μ₀ exactly. These are left as computed, each to its own precision, so their sum can differ from μ₀
in the last few digits (by 2.5e-13 of it for `roots_gegenbauer(150, -0.4)`), but each weight
is far closer to the true one: at 150 nodes SciPy's are off by up
to 1.2 million ULP, and these by under 3,000. A weight's error comes mostly from its node's
rounding to `f64`. Near the end of the interval a weight changes fast with its node, and the
outermost node of a large rule lies close to the end, so the allowance grows with n. The tests
hold the weights to 32 ULP for each node of the rule, and the nodes to 16 ULP.

**SciPy switches to an asymptotic formula for Hermite rules above 150 nodes.** These keep the
eigenvalue method at every size. It costs O(n²), about 0.1 s at
1,000 nodes. At 400 Hermite or Laguerre nodes, where SciPy's Laguerre rule is all NaN, these
match mpmath.

All nine are held to their tolerances at the 3,100 cases of `gauss_quadrature.txt`, from 1 to
400 nodes.

### At the ends

- A rule of fewer than 1 node, an α or β of `roots_jacobi` of −1 or below, an α of
  `roots_genlaguerre` below −1, or an α of `roots_gegenbauer` below −½ stops the program with
  SciPy's message, where SciPy raises ValueError.
- At exactly α = −1, `roots_genlaguerre` gives NaN nodes and weights, as SciPy gives. At exactly
  α = −½, `roots_gegenbauer` does too, except that the middle node of an odd rule is 0, as SciPy
  gives.
- `roots_gegenbauer` at α = 0 is `roots_chebyt`, as SciPy gives.
- The middle node of an odd `roots_chebyu` rule is exactly 0. SciPy gives 6.1e-17 there,
  cos(π/2) in `f64`.

## Files under `special`

`special` is one public file, `<std>/special.byn`, that re-exports each function from an internal
file of its family, as `from special_gamma import gamma as gamma`
([decision 0062](../decisions/0062-standard-library-reexports.md)). A program cannot import the
internal files, and a program's own `special_gamma.byn` is never hidden by them. This table is
for those working on the library. A panic inside a function names the file it is in.

| File | Holds |
| --- | --- |
| `special_common` | The whole-number tests `is_integer` and `is_odd`, `floor_count`, which rounds a count down, `sinpi` and `cospi`, and the constants more than one family uses |
| `special_gamma` | `gamma`, `rgamma`, `gammasgn`, `gammaln`, `loggamma`, `psi`, `digamma`, `polygamma`, the Hurwitz zeta function `polygamma` sums, and `factorial`, which `polygamma` uses |
| `special_beta` | `beta`, `betaln` and `poch` |
| `special_error` | `erf`, `erfc`, `erfcx`, `erfinv`, `erfcinv`, `ndtr`, `ndtri`, `log_ndtr` and `dawsn` |
| `special_bessel_jy` | `j0`, `j1`, `jn`, `y0`, `y1` and `yn` |
| `special_bessel_ik` | `i0`, `i1`, `i0e`, `i1e`, `k0`, `k1`, `k0e`, `k1e` and `kn` |
| `special_incomplete_gamma` | `gammainc`, `gammaincc`, `gammaincinv` and `gammainccinv` |
| `special_incomplete_beta` | `betainc`, `betaincc`, `betaincinv` and `betainccinv` |
| `special_distributions` | `bdtr`, `bdtrc`, `bdtri`, `nbdtr`, `nbdtrc`, `nbdtri`, `pdtr`, `pdtrc`, `pdtri`, `chdtr`, `chdtrc`, `chdtri`, `fdtr`, `fdtrc`, `fdtri`, `gdtr`, `gdtrc`, `stdtr` and `stdtrit` |
| `special_kolmogorov` | `kolmogorov`, `kolmogi`, `smirnov`, `smirnovi` and `owens_t` |
| `special_helpers` | `expit`, `log_expit`, `logit`, `xlogy`, `xlog1py`, `entr`, `rel_entr`, `kl_div`, `binom`, `comb`, `perm`, `logsumexp` and `softmax` |
| `special_integrals` | `exp1`, `expi`, `expn`, `sici`, `shichi` and `fresnel` |
| `special_zeta` | `zeta`, `zetac` and `spence` |
| `special_lambertw` | `lambertw` |
| `special_orthogonal` | `eval_legendre`, `eval_chebyt`, `eval_chebyu`, `eval_hermite`, `eval_hermitenorm`, `eval_laguerre`, `eval_genlaguerre`, `eval_jacobi`, `eval_gegenbauer`, `lpmv` and `sph_harm_y` |
| `special_carlson` | `elliprc`, `elliprd`, `elliprf`, `elliprg` and `elliprj`, and `carlson_rf`, `carlson_rd` and `carlson_rg`, the same integrals without the checks of their arguments |
| `special_elliptic` | `ellipk`, `ellipkm1`, `ellipe`, `ellipkinc`, `ellipeinc` and `ellipj` |
| `special_quadrature` | `roots_legendre`, `roots_chebyt`, `roots_chebyu`, `roots_hermite`, `roots_hermitenorm`, `roots_laguerre`, `roots_genlaguerre`, `roots_jacobi` and `roots_gegenbauer` |

A new function goes in the file of its family, and a name of `special` gets its line in
`special.byn`. A family file kept under about 500 lines is split again by responsibility.

## Not built yet

The rest of `scipy.special` comes in the sub-rows of roadmap 3.18: Bessel functions of real and
complex order, Airy, Struve and Kelvin functions, and hypergeometric functions. Carlson's integrals of complex arguments are not
built. `loggamma` of a complex argument comes with the complex Bessel functions (3.18.6). See [What
builds today](status.md).
