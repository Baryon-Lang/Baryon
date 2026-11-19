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
`pow`, `sin` and `cos`. [Special functions](../guide/special-functions.md) in the guide shows
them in programs, tabulates the changes a program moving from SciPy needs, and times 20 of them
against SciPy on 10 million arguments each.

## Accuracy

Each function is tested against values computed with mpmath to 50 digits, at a few hundred
arguments spread over its range, about 32,500 in all
([the fixtures](../../crates/driver/tests/fixtures/special/README.md)).
The error is counted in units in the last place (ULP): the gap between adjacent `f64` values at
the true result. One ULP is a relative error of about 1.1e-16 to 2.2e-16.

| Function | Tested to | Worst seen | SciPy 1.18's worst at the same arguments |
| --- | --- | --- | --- |
| `gamma` | 8 ULP | 5 ULP | 4 ULP |
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
| `j0`, `j1` | 16 ULP of the larger of the value and its envelope | 3 ULP | 4 ULP |
| `y0`, `y1` | 16 ULP of the larger of the value and its envelope | 5 ULP | 17 ULP; 35 million at `y1(6.8e7)` |
| `jn` | 16 ULP of the larger of the value and its envelope | 6 ULP | 152 ULP at `jn(30, 0.0013)` |
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
| `pdtri`, `chdtri` | The same | 9 ULP | 25 ULP |
| `fdtr`, `fdtrc`, `fdtri` | The same | 136 ULP at a result of 1e-35 | 298 ULP |
| `stdtr`, `stdtrit` | The same | 642 ULP at a result of 2e-116 | 320 ULP for `stdtrit`, 190 for `stdtr` |
| `kolmogorov`, `kolmogi` | The same | 395 ULP at a result of 3e-224 | 1 ULP |
| `smirnov`, `smirnovi` | The same | 179 ULP for `smirnovi` at p = 0.9977, where `d` depends on 1 − p | 1 ULP |
| `owens_t` | The same | 34 ULP at a result of 5e-24 | 1 ULP |
| `expit`, `log_expit`, `logit`, `xlogy`, `xlog1py`, `entr`, `rel_entr` | 8 ULP | 2 ULP | 1 ULP |
| `kl_div` | 8 ULP | 3 ULP | 8.8 billion ULP at `kl_div(0.6515198, 0.6515264)`, where x − y cancels |
| `binom`, `comb` | 8 ULP plus 2 for each unit of the result's logarithm | 18 ULP at a result of 5e160 | 8,523 ULP |
| `perm`, `factorial` | 8 ULP | 4 ULP | 7 ULP |
| `ellipk`, `ellipkm1`, `ellipe` | 8 ULP | 2 ULP | 2 ULP |
| `ellipkinc`, `ellipeinc` | 8 ULP | 4 ULP | 4 ULP |
| `elliprc`, `elliprd`, `elliprf`, `elliprg` | 8 ULP | 4 ULP | 7 ULP |
| `elliprj` | 8 ULP | 6 ULP | 116 ULP where `p` is far below the other arguments |
| `ellipj` | 128 ULP, of the larger of the value and 1 for sn, cn and dn | 96 ULP of the amplitude at m = 1 − 4e-11 | 13 billion ULP of sn at the same arguments |
| `jv`, `yv`, `jvp`, `yvp`, `spherical_jn`, `spherical_yn` | 256 ULP, of the modulus √(J² + Y²) past the turning point x = \|v\| | 106 ULP at `yv(214.4, 22.6)` | 4,723 ULP at `jv(57.7, 845.8)` |
| `iv`, `ive`, `kv`, `kve`, `ivp`, `kvp`, `spherical_in`, `spherical_kn` | 256 ULP | 96 ULP at `kv(234.1, 60.6)` | 442 ULP; NaN for `ive` and `kve` from about 1e10 |
| `airy`, `airye` | 64 ULP, of the envelope √(Ai² + Bi²) or √(Ai′² + Bi′²) below 0 | 36 ULP at x = −11.9 | 394,000 ULP at `airy(-8408.2)`; NaN from x = 103.9 for Bi and past about 1e8 for all |
| `ber`, `bei`, `ker`, `kei`, `berp`, `beip`, `kerp`, `keip`, `kelvin` | 32 ULP of the size of the complex value each is a part of | 13 ULP | 4.4 million ULP at `ker(9.70)` |
| `struve`, `modstruve` | 128 ULP, of the larger of H and the modulus √(J² + Y²) past the turning point x = \|v\| | 57 ULP at `struve(-1.56, 26.3)` | 13,138 ULP at `modstruve(0.28, 13.6)` |
| `hyp0f1` | 16 ULP, of the largest of \|f\| and \|t ∂f/∂t\| for each argument t | 13 ULP at `hyp0f1(-8.88, -162.2)` | 2,469 ULP at `hyp0f1(0.40, -35.7)`; NaN at `hyp0f1(-237.8, -965.8)`, where the value is 60.1 |
| `hyp1f1` | 64 ULP, of the same | 55 ULP at `hyp1f1(0.00037, -123.7, -115.2)` | 3e43 at `hyp1f1(0.00033, -92.95, -213.5)`, where the value is 0.99 |
| `hyperu` | 512 ULP, of the same | 385 ULP at `hyperu(0.017, -146.0, 0.0019)` | 3e-256 at `hyperu(143.0, -27.0, 2.42)`, where the value is 3e-284; NaN at `hyperu(84.8, -14.7, 1.04)` |
| `hyp2f1` | 128 ULP, of the same | 68 ULP at `hyp2f1(-13.0, -0.381, 0.0145, 0.995)` | 1.6e22 ULP at `hyp2f1(-18.000000001, 19.0, 1.445, 0.970)`, 3.6 million where the value is 0.035 |
| `pbdv`, `pbvv`, with their derivatives | 32 ULP, of the largest of \|f\|, \|v ∂f/∂v\| and \|x ∂f/∂x\| | 10 ULP at the derivative of `pbdv(1.62, 3.31)` | 2.4e270 ULP at the derivative of `pbvv(4.0, -9.32)`, where the value is 151,288; NaN at `pbdv(54.4, -47.3)` |
| `mathieu_a`, `mathieu_b` | 16 ULP, of the larger of \|a\| and \|q ∂a/∂q\| | 1 ULP | 4e15 ULP at `mathieu_a(22, -265.6)`, 92.3 where the value is 569.5 |
| `mathieu_cem`, `mathieu_sem`, with their derivatives | 16 ULP, of the largest of \|f\|, \|q ∂f/∂q\|, \|x ∂f/∂x\| and the sum of the sizes of the Fourier terms | 7 ULP at the derivative of `mathieu_cem(1, 125.9, -175.8)` | 1.8e15 ULP at `mathieu_cem(22, -265.6, 90)`, −1.5e-5 where the value is −1.64 |
| `pbwa`, with its derivative | 32 ULP, of the largest of \|f\|, \|a ∂f/∂a\| and \|x ∂f/∂x\| | 0 ULP: every case rounds as mpmath's value does | 20,104 ULP at `pbwa(3.50, 2.55)`; NaN past \|x\| = 5 |
| `pro_cv`, `obl_cv`, `pro_cv_seq`, `obl_cv_seq` | 64 ULP, of the larger of \|λ\| and \|c ∂λ/∂c\| | 22 ULP at `pro_cv(3, 3, 164.7)` | 34 ULP at c above 0; for c below 0 the value at c = 0 |
| `pro_ang1`, `obl_ang1` and their `_cv` forms, with their derivatives | 16 ULP, of the largest of \|S\|, \|c ∂S/∂c\|, \|x ∂S/∂x\| and the sum of the sizes of the Legendre terms | 8 ULP at `obl_ang1(4, 16, 7.53, 0.950)` | 2.5e40 ULP at `pro_ang1(2, 22, 158.7, -0.103)`, −2.9e27 where the value is −26.0; a crash at c above 170 |
| `pro_rad1`, `pro_rad2`, `obl_rad1`, `obl_rad2` and their `_cv` forms, with their derivatives | 128 ULP, of the largest of \|R\|, \|c ∂R/∂c\|, \|x ∂R/∂x\|, the modulus of the two kinds, the sum of the sizes of the Bessel terms and \|R\| times the normalizing sum's cancellation | 54 ULP at the derivative of `obl_rad2(7, 8, 30.6, 0)` | 1.6e23 ULP at the derivative of `obl_rad2(3, 14, 3.39, 0.984)`, −5.9e14 where the value is 1.9e6 |
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
  half-unit interval, 16 to 19 terms; **from 3 up**, (1 + z S(z)) / (x√π) with z = 1/x²
  and S a rational function of degree 6 fitted with mpmath, which replaced Laplace's continued
  fraction of 10 to 40 levels (roadmap 3.18.17).
- **`erfc` from 0.5 up** is exp(−x²) erfcx(x). The square x² is carried as two `f64`s, its
  rounded value and the rounding error found with `fma`, so the rounding does not reach the
  exponential; a rounded square alone would cost 2x² ULP, about 1,400 at x = 26. `erf` above
  0.5 is 1 − erfc(x), and below 0 both use their symmetry.
- **`ndtr`** is erfc(−x/√2) / 2 with −x/√2 carried as two `f64`s in the same way, so the lower
  tail keeps its digits down to the underflow near −38.5.
- **`erfinv`** refines a first guess by Halley's method. Above |y| = 0.5 it is
  `erfcinv(1 − |y|)`, where the subtraction is exact. **`erfcinv`** below 0.5 solves
  −x² + log erfcx(x) = log q by Newton's method, which stays finite for the smallest `q`.
- **`ndtri`** starts from a rational function fitted with mpmath: of 0.180625 − q² times
  q = p − ½ for |q| up to 0.425, and of t = √(−log p) beyond, in two pieces split at t = 5,
  with p taken as 1 − p above ½. One Halley step on erf(x/√2) = 2q in the centre, or one Newton
  step on −v² + log erfcx(v) = log 2p in the tails, takes the fit's 6 ULP to 2.
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

The order `n` of `jn`, `yn` and `kn` is an `int`. The Bessel functions of any real order,
`jv`, `yv`, `iv` and `kv`, are in [the next section](#bessel-functions-of-real-order).

```python
from special import j0, jn


def main():
    # The first zero of J₀, rounded to f64: the true value there is -6.1e-17.
    print(j0(2.404825557695773))    # 0.0, within a ULP of the envelope
    print(jn(5, 2.0))               # 0.007039629755871685
```

How each is computed:

- **J₀ and J₁** sum their power series up to |x| = 2, where the first term is the largest.
  From 2 to 5, Miller's backward recurrence Jₖ₋₁ = (2k/x) Jₖ − Jₖ₊₁ runs down from an order
  about x/2 + 15 + √x above the one wanted, where the starting guess no longer matters, and the
  ladder is scaled so that J₀ + 2 Σ J₂ₖ = 1. From 5 up, Hankel's form √(2/(πx)) (P cos χ −
  Q sin χ), with P and xQ rational functions of 1/x² fitted with mpmath (roadmap 3.18.17) and
  the phase formed from `cos(x)` and `sin(x)` of the exact argument.
- **Y₀ and Y₁** use their logarithmic series up to 2, Neumann's series over the same ladder of J
  from 2 to 5, and Hankel's form beyond.
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

## Bessel functions of real order

| Function | Result | SciPy |
| --- | --- | --- |
| `jv(v, x)`, `yv(v, x)` | Bessel functions of the first and second kinds, Jᵥ(x) and Yᵥ(x), of any real order | `scipy.special.jv`, `yv` |
| `iv(v, x)`, `kv(v, x)` | Modified Bessel functions Iᵥ(x) and Kᵥ(x) | `scipy.special.iv`, `kv` |
| `jve(v, x)`, `yve(v, x)` | Jᵥ(x) e^(−\|Im x\|) and the same for Y: for a real x, `jv` and `yv` | `scipy.special.jve`, `yve` |
| `ive(v, x)`, `kve(v, x)` | Iᵥ(x) e^(−\|x\|) and Kᵥ(x) eˣ, finite where I overflows or K underflows | `scipy.special.ive`, `kve` |
| `jvp(v, x, n=1)`, `yvp`, `ivp`, `kvp` | The n-th derivative with respect to x | `scipy.special.jvp`, `yvp`, `ivp`, `kvp` |
| `spherical_jn(n, x, derivative=False)`, `spherical_yn` | Spherical Bessel functions jₙ(x) = √(π/(2x)) Jₙ₊½(x) and yₙ(x), or their derivatives | `scipy.special.spherical_jn`, `spherical_yn` |
| `spherical_in(n, x, derivative=False)`, `spherical_kn` | Modified spherical Bessel functions iₙ(x) and kₙ(x), or their derivatives | `scipy.special.spherical_in`, `spherical_kn` |

The order `v` is an `f64` of either sign; the spherical functions' `n` is an `int`. The argument
is a real `f64`: the same names of a complex argument come in roadmap 3.18.6.

```python
from special import jv, kv, spherical_jn


def main():
    print(jv(2.5, 3.0))             # 0.41271003220971597
    print(kv(0.5, 2.0))             # 0.11993777196806145, which is √(π/4) e⁻²
    print(spherical_jn(0, 1.0))     # 0.8414709848078964, within a ULP of sin(1)
```

**How they work.** For an order of 0 or more, by the method of Temme and Steed (as Numerical
Recipes §6.7 sets it out): a continued fraction gives Jᵥ′/Jᵥ, the recurrence carries it down
to an order μ within ½ of 0, Temme's series (below x = 2) or Steed's complex continued fraction
(above) gives Y there, the Wronskian fixes the scale, and the recurrence carries Y back up.
Three other methods take over where they are more accurate:

- Below x² = 6(v + 1) for J, and 100(v + 1) for I, the power series, which keeps each value's
  relative precision where the recurrences lose up to 20,000 ULP.
- From x = 30 and above v²/4, Hankel's asymptotic expansion, with its phase formed from cos x
  and sin x of the exact argument. For I and K the same holds above v².
- From x = 30 and above v, below v²/4, Hankel's expansion at the small order μ and μ + 1, then
  the recurrence up to v, which is stable for both J and Y while the order is below x. The
  continued fraction would take about x steps there and lose up to 100,000 ULP.

A negative order uses the reflection formulas J₋ᵥ = cos(vπ) Jᵥ − sin(vπ) Yᵥ,
Y₋ᵥ = sin(vπ) Jᵥ + cos(vπ) Yᵥ, I₋ᵥ = Iᵥ + (2/π) sin(vπ) Kᵥ and K₋ᵥ = Kᵥ, with a cosine and sine
of vπ that are exact at whole and half-whole orders. `jvp` and the other derivatives take the
method's own derivative for n = 1 and the sum 2⁻ⁿ Σ (−1)ᵏ C(n, k) Jᵥ₋ₙ₊₂ₖ (DLMF 10.6.7) above it,
as SciPy does. The spherical functions are √(π/(2x)) times J, Y, I or K of order n + ½.

**Accuracy.** J and Y oscillate past the turning point x = |v|, so there they are tested
against the modulus √(J² + Y²), the envelope they oscillate under, and elsewhere against
themselves. The worst errors, about 100 ULP, come at orders above 200, where the recurrences
of Steed's method run for v steps; up to order 60 these are within about 60 ULP, and often
closer to the true value than SciPy: `jv(2.5, 3.0)` is correctly rounded here and 2 ULP off in
SciPy, and `spherical_jn(2, 1.5)` 1 ULP here and 9 there. Each factor of those recurrences is
formed afresh rather than carried down by steps of 1/x, and 1/Γ(v + 1) is taken as
(1/Γ(v))/v, since v + 1 can round and Γ is steep enough there to turn that into v ψ(v) ULP.

The orders recorded in the tests reach 250, and the arguments 1e8. A larger order works, at a
cost of about v + x steps; above about 170 the power series gives way to the recurrence.

### At the ends

Each function gives its limit where SciPy 1.18 gives NaN or a wrong infinity:

- At x = 0 and a negative order v that is not whole, Jᵥ and Iᵥ are ±∞ with the sign of
  1/Γ(v + 1), the sign of their leading term. SciPy gives +∞ for `iv(-1.5, 0.0)`, where the
  limit is −∞, and NaN for `jve` and `ive`.
- `yv(v, 0.0)` is −∞ for v of 0 or more, and for a negative v it is −∞ times the sign of
  cos(vπ), and 0 when −v is half-whole, since Yᵥ is then ±J₋ᵥ. SciPy gives an infinity there.
  `yve` is `yv` at 0, where SciPy gives NaN.
- `ive` and `kve` stay finite for every finite x: `ive(0.0, 1e10)` is 3.99e-6, where SciPy
  gives NaN from about 1e10.
- At x = ∞, J, Y and the scaled functions are 0 and I is ∞, where SciPy gives NaN for J and Y
  of non-whole order. At an infinite order, `yv(inf, x)` is −∞ and `kv(±inf, x)` is ∞, where
  SciPy gives NaN; `jv(-inf, x)` and `iv(-inf, x)` are NaN, where SciPy gives 0, since the
  reflection formulas have no limit there.
- `spherical_yn(n, 0.0, True)` is ∞ for every n, where SciPy gives NaN from n = 1, and
  `spherical_in(n, inf, True)` is ∞, where SciPy gives NaN from n = 3.

As SciPy gives: NaN for a negative x unless v is whole, where Jᵥ(−x) = (−1)ᵛ Jᵥ(x) and the same
for I; NaN for Y and K of a negative x; and `kv(v, 0.0)` = ∞. The spherical functions of a
negative argument follow their parity, jₙ(−x) = (−1)ⁿ jₙ(x), yₙ(−x) = (−1)ⁿ⁺¹ yₙ(x) and
iₙ(−x) = (−1)ⁿ iₙ(x), and `spherical_kn` continues SciPy's closed form
(π/2) e⁻ˣ/x Σₖ (n + k)!/(k! (n − k)!) (2x)⁻ᵏ there, as −π iₙ(|x|) − (−1)ⁿ kₙ(|x|). They are
NaN for a negative n. A negative derivative order stops the program with SciPy's message,
where SciPy raises ValueError.

## Airy functions

| Function | Result | SciPy |
| --- | --- | --- |
| `airy(x)` | The tuple (Ai(x), Ai′(x), Bi(x), Bi′(x)) of the Airy functions, the solutions of w″ = x w that fall and grow above 0 and oscillate below it | `scipy.special.airy` |
| `airye(x)` | The same scaled: Ai eᶻ, Ai′ eᶻ, Bi e⁻ᶻ and Bi′ e⁻ᶻ with ζ = ⅔ x^{3/2} above 0, finite where Ai underflows and Bi overflows; below 0 the scale is 1 | `scipy.special.airye` |

```python
from special import airy, airye


def main():
    ai, ai_slope, bi, bi_slope = airy(1.0)
    print(ai)                       # 0.13529241631288147
    ai, ai_slope, bi, bi_slope = airye(10.0)
    print(bi)                       # 0.3183401053367345, where Bi itself is 4.6e8
```

**How they work.** From x = −2 to 1, the Maclaurin series of Ai and Bi and their derivatives.
Elsewhere they are Bessel functions of order ⅓ and ⅔ at ζ = ⅔|x|^{3/2} (DLMF 9.6): Kᵥ and Iᵥ,
scaled, above 0, where every term has one sign, and Jᵥ and Yᵥ below it, from the methods of the
[Bessel functions of real order](#bessel-functions-of-real-order). ζ is formed as two `f64`s
and the part rounding would drop is put back by a first-order step, so the phase of the
oscillation below 0 and the exponent above it are as exact as x itself. Rounding ζ to one `f64`
would lose up to ζ ULP: 1,900 at x = −200.

**Accuracy.** Below 0 each value is tested against its envelope, √(Ai² + Bi²) for Ai and Bi and
√(Ai′² + Bi′²) for the derivatives, since it crosses zero; above 0 against itself. The worst
is 36 ULP, from x = −8 to −12, where Steed's method passes the turning point; SciPy loses up to
394,000 ULP far below 0.

### At the ends

Where SciPy gives NaN, these give the limits and the true values:

- Bi and Bi′ are ∞ from x = 104, where they overflow, and Ai and Ai′ fall to 0; SciPy gives
  NaN for Bi and Bi′ from x = 103.9, and for all four past |x| = 1e8 or so.
- `airye` below 0 is `airy`, since ζ of a negative x is imaginary and the scale e^{−|Re ζ|} is 1;
  SciPy gives NaN for its first two values there.
- At x = ∞, `airy` is (0, 0, ∞, ∞) and `airye` (0, −∞, 0, ∞); at x = −∞, Ai and Bi are 0 and
  Ai′ and Bi′, which oscillate without bound, NaN. SciPy gives NaN at both.
- Past |x| = 1.5e205, where ζ overflows, `airye` above 0 is the first term of each asymptotic
  expansion, which is exact there, and below 0 every value is NaN, since the phase is lost.

## Kelvin functions

| Function | Result | SciPy |
| --- | --- | --- |
| `ber(x)`, `bei(x)` | The Kelvin functions ber x + i bei x = I₀(x e^{iπ/4}) | `scipy.special.ber`, `bei` |
| `ker(x)`, `kei(x)` | The Kelvin functions ker x + i kei x = K₀(x e^{iπ/4}) | `scipy.special.ker`, `kei` |
| `berp(x)`, `beip(x)`, `kerp(x)`, `keip(x)` | Their derivatives | `scipy.special.berp`, `beip`, `kerp`, `keip` |
| `kelvin(x)` | All eight at once, as four `complex128`: (ber + i bei, ker + i kei, ber′ + i bei′, ker′ + i kei′) | `scipy.special.kelvin` |

They describe, among other things, the current in a wire carrying alternating current, where
the skin effect pushes it to the surface.

```python
from special import ber, kelvin


def main():
    print(ber(2.0))                 # 0.7517341827138083
    be, ke, be_slope, ke_slope = kelvin(2.0)
    print(ke.imag)                  # -0.20240006776470443, which is kei(2.0)
```

**How they work.** They are modified Bessel functions of order 0 on the ray at 45°: up to
x = 2 their power series give I₀, I₁, K₀ and K₁ there, and above it Steed's continued fraction
gives K₀ eᶻ and K₁ eᶻ, a second continued fraction gives I₁/I₀, and the Wronskian gives I₀ e⁻ᶻ,
all in complex arithmetic. The factor e^{±z} is applied last: its size in two halves, so a value
overflows only when it is too large itself, and its phase x/√2 carried as two `f64`s. The
power series alone would lose a factor of e^{0.3x} to cancellation.

**Accuracy.** Each part is tested against the size of the complex value it belongs to, since
each part crosses zero while the whole does not. The worst is 13 ULP; SciPy loses up to 4.4
million ULP of `ker` near x = 9.7, where its methods meet.

### At the ends

As SciPy gives: ber and bei are even and their derivatives odd, so a negative x takes the
value at |x|; ker and kei and their derivatives are NaN below 0; at 0 the values are 1, 0, ∞,
−π/4, 0, 0, −∞ and 0. From x = 1,004 ber, bei and their derivatives overflow to infinities with
the sign of their phase, and ker and kei fall to 0. At x = ∞, where SciPy gives NaN for all,
ker, kei and their derivatives are 0, and ber, bei and theirs, which oscillate without bound,
NaN.

## Struve functions

| Function | Result | SciPy |
| --- | --- | --- |
| `struve(v, x)` | The Struve function Hᵥ(x), the solution of Bessel's equation with the right-hand side (x/2)^{v−1}/(√π Γ(v + ½)) that is small at 0 | `scipy.special.struve` |
| `modstruve(v, x)` | The modified Struve function Lᵥ(x) = −i e^{−ivπ/2} Hᵥ(ix) | `scipy.special.modstruve` |

They appear in the radiation of a vibrating piston and in the impedance of acoustic and
electromagnetic apertures.

```python
from special import modstruve, struve


def main():
    print(struve(0.0, 1.0))         # 0.5686566270482879
    print(modstruve(0.0, 1.0))      # 0.7102431859378907
```

**How they work.** Each has a power series (DLMF 11.2.1 and 11.2.2), used where its terms do
not cancel: always for L above order −3/2, whose terms are then all positive, and for H while x
is small. Elsewhere:

- H below its order, x < v, is the Bessel series √(x/(2π)) Σ (x/2)ᵏ/(k! (k + ½)) J_{k+v+½}(x)
  (DLMF 11.4.19), whose terms are all positive there. Its J come from Miller's algorithm, the
  recurrence run down from a high order, fixed by Neumann's sum, also of positive terms.
- H elsewhere is Yᵥ plus the Laplace integral 2(x/2)ᵛ/(√π Γ(v + ½)) ∫₀^∞ e^{−xt} (1 + t²)^{v−½} dt
  (DLMF 11.5.2), whose integrand is smooth and of one sign, by double-exponential quadrature.
- L from x = 40 and above its order is I₋ᵥ(x) plus its asymptotic series (DLMF 11.6.2).

1/Γ(v + 3/2) and the other gamma functions of a shifted order are taken at the exact sum, with
the rounding of v + 3/2 put back by a step of ψ: Γ is so steep that this rounding alone would
cost about v ψ(v) ULP.

**Accuracy.** Past the turning point x = |v|, H is tested against the larger of itself and the
modulus √(Jᵥ² + Yᵥ²), since there it is Y plus a smooth part and crosses zero; before it, and L
everywhere, against itself. The tests reach orders from −10 to 100 and arguments up to 1e9. The
worst is 57 ULP, near zeros of H at negative orders; SciPy loses up to 13,138 ULP.

### At the ends

- At x = 0 the value is the first term of the series: 0 above order −1, 2/π at −1, and below
  it an infinity with the sign of 1/Γ(v + 3/2), as SciPy gives. At the orders −3/2, −5/2 and so
  on that term is 0 and the function is ±J₋ᵥ(0) = 0, where SciPy gives ∞ or NaN.
- At x = ∞, H is 0 below order 1, 2/π at 1 and ∞ above, where SciPy gives ∞ for every order,
  and L is ∞, where SciPy gives NaN at order 0. SciPy's H also loses its precision past about
  x = 1e9: `struve(0.5, 1e10)` is 1e10 there, where the value is about 8e-6.
- As SciPy gives: a negative x is NaN unless v is whole, where Hᵥ(−x) = (−1)^{v+1} Hᵥ(x) and the
  same for L; at v = ∞ both are 0, and at v = −∞ H is NaN and L 0.

## Hypergeometric functions

| Function | Result | SciPy |
| --- | --- | --- |
| `hyp0f1(b, x)` | The confluent hypergeometric limit function ₀F₁(; b; x) = Σ xᵏ/((b)ₖ k!) | `scipy.special.hyp0f1` |
| `hyp1f1(a, b, x)` | Kummer's function M(a, b, x) = ₁F₁(a; b; x) = Σ (a)ₖ xᵏ/((b)ₖ k!) | `scipy.special.hyp1f1` |
| `hyperu(a, b, x)` | Tricomi's function U(a, b, x), the solution of Kummer's equation that behaves as x⁻ᵃ for large x | `scipy.special.hyperu` |
| `hyp2f1(a, b, c, x)` | Gauss's hypergeometric function F(a, b; c; x) = ₂F₁(a, b; c; x) = Σ (a)ₖ (b)ₖ xᵏ/((c)ₖ k!) | `scipy.special.hyp2f1` |

(b)ₖ = b (b + 1) ⋯ (b + k − 1) is the rising factorial. M and U solve Kummer's equation
x w″ + (b − x) w′ − a w = 0, which describes the hydrogen atom's radial wave functions, the
Coulomb wave functions and the quantum harmonic oscillator; ₀F₁ is a Bessel function in another
form. Gauss's F solves x(1 − x) w″ + (c − (a + b + 1)x) w′ − ab w = 0, and holds most of the
elementary functions and the orthogonal polynomials as special cases: log(1 + x) is
x F(1, 1; 2; −x). All four take real `f64` arguments; U needs x at least 0, and F is real only
up to x = 1, past which it is a polynomial or complex.

```python
from special import hyp0f1, hyp1f1, hyp2f1, hyperu


def main():
    print(hyp0f1(1.5, -2.25))       # 0.04704000268662233, sin(3)/3
    print(hyp1f1(1.0, 2.0, 1.0))    # 1.7182818284590455, e − 1
    print(hyperu(1.0, 1.0, 1.0))    # 0.5963473623231939, e E₁(1)
    print(hyp2f1(1.0, 1.0, 2.0, -3.0))  # 0.4620981203732966, log(4)/3
```

**How they work.** No one method holds over the whole plane of a, b and x, so each value takes
the first of several that keeps its digits:

- ₀F₁ is its series where the terms barely cancel, which is always for b and x above 0, and
  elsewhere the Bessel function it equals, Γ(b) (√x)^{1−b} I_{b−1}(2√x) above 0 and the same
  with J below (DLMF 10.39.9 and 10.16.9). The argument 2√|x| is carried as two `f64`s, and
  the part rounding would drop is put back by a step along the Bessel function's slope.
- M is its series where the terms barely cancel; the same series in double-double arithmetic
  (`math.f64x2`) where they cancel by less than 10¹⁴; the asymptotic expansion for large |x|
  (DLMF 13.7.2), where the term it drops is below e⁻⁴⁰ of the one it keeps; and, for a below 0
  and x above 0, the recurrences: b is raised above x, a is taken down its recurrence, which is
  stable there, and b brought back down by its own (DLMF 13.3.1 and 13.3.2). A negative x is
  sent to a positive one by Kummer's transformation M(a, b, x) = eˣ M(b − a, b, −x): at once
  for b above 0 and at least a, where the new series' terms are all positive, and otherwise
  where M's own series cancels.
- U is its asymptotic expansion for large x (DLMF 13.7.3); the polynomial it is for a whole a at
  most 0 (DLMF 13.2.7); for a at least 1, its integral 1/Γ(a) ∫₀^∞ t^{a−1} (1 + t)^{b−a−1}
  e^{−xt} dt (DLMF 13.4.4) by double-exponential quadrature centred on the integrand's peak; for
  a whole b, the series with a logarithm (DLMF 13.2.9); otherwise two of Kummer's M
  (DLMF 13.2.42); and last the same integral through x^{1−b} U(a − b + 1, 2 − b, x), or the
  recurrence in a run down from two values by quadrature.
- F is the series of whichever of four equal forms cancels least: F itself, Euler's (1 − x)^{c−a−b}
  F(c − a, c − b; c; x), and Pfaff's two in x/(x − 1) (DLMF 15.8.1), in `f64` or double-double. The
  first that barely cancels is taken; below 0 Pfaff's are tried first. Near x = 1, and far below 0
  through Pfaff's form, it is a series in 1 − x (DLMF 15.8.4), which has two parts that grow as 1/ε
  where c − a − b is ε from a whole number: there the parts are joined term by term, and at ε = 0
  the series with a logarithm (DLMF 15.8.10) takes over. Below −1 it can also be a series in 1/x
  (DLMF 15.8.2). A polynomial is summed in x, 1 − x or 1/x, whichever cancels least. Each derived
  parameter, such as a − b + 1, is kept as an exact sum of two `f64`s, so a factor (a − b + 1 + k)
  near 0 keeps its digits.

**Accuracy.** A small change in a parameter can move these functions far, so each case is
measured against the largest of |f| and |t ∂f/∂t| for each argument t: the error that rounding
the arguments to `f64` alone would make, which no method can beat. The tests reach parameters
from −300 to 1,000 for ₀F₁, from −150 to 150 for M and U and from −20 to 20 for F, and
arguments to ±3e4.

| Function | Tested where | Worst | Cases SciPy misses by more than 64 ULP |
| --- | --- | --- | --- |
| `hyp0f1` | b from −300 | 13 ULP | 39 of 398, some by giving ∞ |
| `hyp1f1` | \|a\| up to 100, \|x\| up to 1,000 | 55 ULP | 37 of 1,204 |
| `hyperu` | a from −35 | 385 ULP | 194 of 803, some by giving ∞ |
| `hyp2f1` | \|a\|, \|b\|, \|c\| up to 20 | 68 ULP | 143 of 1,243 |

U's worst cases, from 64 to 385 ULP, are at b below about −30 with |a| below 1, where the series
it is summed from cancel by up to 64 times. Outside the tested ranges these can lose digits:

- ₀F₁ overflows to ∞ for b below about −300 where x is large, since I_{b−1} does.
- M loses digits where |a| is above 100 and x is below about −380, and can give NaN for x
  below −1,000. For b within 0.01 of 0 and x below −30 it can be off by about 1,000 ULP.
- U loses digits for a below −35, and for a below 0 and not whole with b above a + 1 and
  within 0.001 of a whole number, where Kummer's two M cancel and the recurrence in a is
  unstable.
- F holds over most of the range to |a|, |b|, |c| of 50, but where all its forms cancel there,
  usually with c − a − b or b − a near a whole number, it can lose its digits or give NaN: 10 of
  1,200 trial cases, half with parameters to 50 and half to 10, did, all with a parameter above
  25. SciPy misses 149 of the same cases.

### At the ends

- ₀F₁: a b of 0 or a negative whole number is a pole, NaN as SciPy gives; an infinite b gives 1.
  At x = ∞ the value is ∞ with the sign of Γ(b), and at x = −∞, where the function oscillates
  as |x|^{1/4 − b/2}, 0 for b above ½ and NaN otherwise, where SciPy gives NaN. Where the value
  overflows this gives ∞, where SciPy gives 0: `hyp0f1(1.0, 1e6)`.
- M: a b of 0 or a negative whole number is ∞, as SciPy gives, unless a is a negative whole
  number no further from 0, where the series is a polynomial that stops first. An infinite b
  gives 1 and an infinite a NaN. At x = ±∞ this gives the limit, ∞ or 0 with its sign, where
  SciPy gives NaN or, for `hyp1f1(2.0, 3.0, inf)`, does not return.
- U: a negative x, or an infinite a or b, is NaN, as SciPy gives. At x = 0, Γ(1 − b)/Γ(a − b + 1)
  for b below 1 and ∞ from 1 up, as SciPy gives, except for a whole a at most 0, where U is a
  polynomial and this gives its value: `hyperu(-2.0, 3.0, 0.0)` is 12, where SciPy gives ∞. At
  x = ∞ this gives the limit, 0 for a above 0 and ∞ below, where SciPy gives NaN.
- F: past x = 1 it is complex unless a or b is a whole number at most 0, where it is a
  polynomial and this gives its value, as SciPy gives; elsewhere this gives NaN, where SciPy
  gives ∞. At x = 1 it is Γ(c)Γ(c − a − b)/(Γ(c − a)Γ(c − b)) for c − a − b above 0 and ∞
  otherwise, as SciPy gives. A c of 0 or a negative whole number is ∞ unless a or b ends the
  series first, as SciPy gives. An infinite c gives 1, and an infinite a or b NaN. At x = ±∞ a
  polynomial gives ∞ with the sign of its last term, and F gives 0 at −∞ for a and b above 0,
  where SciPy gives NaN.

## Parabolic cylinder functions

| Function | Result | SciPy |
| --- | --- | --- |
| `pbdv(v, x)` | Weber's function D_v(x) and its derivative D_v′(x), as a tuple | `scipy.special.pbdv` |
| `pbvv(v, x)` | The second solution V_v(x), V(a, x) of DLMF 12.2 with a = −v − ½, and its derivative V_v′(x), as a tuple | `scipy.special.pbvv` |
| `pbwa(a, x)` | W(a, x) of DLMF 12.14, a solution of w″ + (x²/4 − a) w = 0, and its derivative W′(a, x), as a tuple, for \|a\| up to 5 and \|x\| up to 10 | `scipy.special.pbwa` |

Both solve w″ − (x²/4 + a) w = 0, the equation of the quantum harmonic oscillator and of
waves near a parabolic boundary. D_v decays as x grows and V_v grows; at a whole v = n,
D_n(x) is e^(−x²/4) Heₙ(x), the oscillator's n-th state. W(a, x) solves the equation with the
sign of x²/4 turned, w″ + (x²/4 − a) w = 0, which describes a wave meeting a parabolic barrier
of height a: it oscillates where x²/4 is above a and grows or decays where it is below. Each
tuple holds the value first and the derivative second, in SciPy's order.

```python
from special import pbdv, pbvv, pbwa


def main():
    d, slope = pbdv(1.0, 2.0)
    print(d)                    # 0.7357588823428848, 2/e
    print(slope)                # -0.3678794411714422, −1/e
    print(pbvv(0.5, 1.0)[0])    # 0.22003508652565454
    print(pbwa(1.0, 0.5)[0])    # 0.4679105355315861
```

**How they work.** From x of 0 up, D_v(x) is 2^(v/2) e^(−x²/4) U(−v/2, ½, x²/2), Tricomi's
function of `hyperu` (DLMF 12.7.14), and V_v(x) is V(a, 0) u₁ + V′(a, 0) u₂ (DLMF 12.4.2), with
u₁ and u₂ the even and odd solutions in `hyp1f1` (DLMF 12.7.12 and 12.7.13). Past x = 37, where
e^(x²/2) in u₁ and u₂ overflows, V_v is its asymptotic expansion (DLMF 12.9.2). Below 0 both
come from the same two at |x| (DLMF 12.2.15 and 12.2.18), with Γ's reflection formula folded
in so that no pole is reached at a whole v: D_v(−x) = cos(πv) D_v(x) + π/Γ(−v) V_v(x) and
V_v(−x) = −sin(πv)/Γ(1 + v) D_v(x) − cos(πv) V_v(x). The derivatives come from the
recurrences D_v′ = x/2 D_v − D_{v+1} and V_v′ = V_{v−1} − x/2 V_v (DLMF 12.8.3 and 12.8.9).
e^(−x²/4) is taken with x² carried as two `f64`s, so that it is rounded once.

W(a, ±x) is 2^(−3/4) (√(G₁/G₃) y₁(x) ∓ √(2G₃/G₁) y₂(x)) (DLMF 12.14.4 to 12.14.9), with y₁
and y₂ the even and odd solutions of its power series and G₁ = |Γ(¼ + ia/2)|,
G₃ = |Γ(¾ + ia/2)|. Where a and x are both above 0, W can be 10¹⁵ times smaller than the two
parts, so the series and the Γ ratio are carried in `f64x2`, about 30 digits, and the parts
are subtracted there. The Γ ratio moves both Γ up by 24 and uses the expansion of
log Γ(u + ½) − log Γ(u) in odd powers of 1/u, whose coefficients come from Bernoulli
polynomials. That costs about 8 µs a call, three times SciPy's.

**Accuracy.** As for the hypergeometric functions, each case is measured against the largest of
|f|, |v ∂f/∂v| and |x ∂f/∂x|, the error that rounding v and x alone would make. The tests reach
orders and arguments from −60 to 60. The worst is 10 ULP. SciPy misses 289 of the 1,750 cases
by more than 64 ULP, some by 10²⁷⁰ times the value or by giving NaN. `pbwa` rounds as mpmath
does at all 600 of its cases, a from −5 to 5 and x from −10 to 10. SciPy gives NaN past |x| = 5,
and within it misses 58 of 390 cases by more than 64 ULP, by 20,104 ULP at worst, where it
keeps 11 digits of the value.

### At the ends

- At x = ∞, D_v and its derivative are 0 and V_v and its derivative ∞. At x = −∞, D_v is ∞
  with the sign of 1/Γ(−v), or 0 at a whole v from 0 up, and V_v is ∞ with the sign of
  −cos(πv), or 0 where v is a half-odd number; each derivative has the opposite sign. SciPy
  gives NaN at both ends.
- A NaN or an infinite v gives NaN in both places, as SciPy gives.
- `pbwa` gives NaN for |a| above 5 or |x| above 10, its domain, where SciPy gives NaN past 5
  for both. Past |x| = 10 its power series would need more than 30 digits; the asymptotic
  expansions that would take over are not built.
- Where the value passes the largest `f64` it is ∞, and where it falls below the smallest it is
  0: `pbdv(1.0, 100.0)` is 0, the value being 2e-1084.

## Mathieu functions

| Function | Result | SciPy |
| --- | --- | --- |
| `mathieu_a(m, q)` | The characteristic value a_m(q) of the even function ce_m, for a whole m from 0 up | `scipy.special.mathieu_a` |
| `mathieu_b(m, q)` | The characteristic value b_m(q) of the odd function se_m, for a whole m from 1 up | `scipy.special.mathieu_b` |
| `mathieu_cem(m, q, x)` | ce_m(x, q) and its derivative, as a tuple, with x in degrees | `scipy.special.mathieu_cem` |
| `mathieu_sem(m, q, x)` | se_m(x, q) and its derivative, as a tuple, with x in degrees | `scipy.special.mathieu_sem` |

Mathieu's equation y″ + (a − 2q cos 2x) y = 0 (DLMF 28.2) describes waves in an elliptic
membrane or waveguide, a pendulum whose pivot is shaken, and the motion of an ion in a Paul
trap. Its solutions of period π or 2π exist only for the characteristic values of a, one for
each order m: a_m(q) for the even solution ce_m and b_m(q) for the odd solution se_m. At q = 0
they are cos mx and sin mx, and a_m and b_m are m². The angle x is in degrees and the
derivative per radian, as SciPy has them.

```python
from special import mathieu_a, mathieu_cem


def main():
    print(mathieu_a(2.0, 1.5))    # 4.746779468115387
    value, slope = mathieu_cem(2.0, 1.5, 30.0)
    print(value)                  # 0.7944735999023059
    print(slope)                  # -1.1494447465718143
```

**How they work.** ce_m and se_m are Fourier series in cos kx or sin kx, with k running over the
even or the odd numbers as m does. Their coefficients satisfy a three-term recurrence (DLMF
28.4.5 to 28.4.8), so a characteristic value is an eigenvalue of a symmetric tridiagonal matrix,
one matrix for each parity of m and kind, and the coefficients are its eigenvector. Bisection
on the count of eigenvalues below a point finds the eigenvalue of rank m to within ¼, and
Rayleigh quotient iteration finishes it. A twisted factorization of the matrix less the value
gives the coefficients, as LAPACK's MRRR does, each step dividing by a pivot that is not small.
They are scaled as SciPy scales them, so that the square of each function integrates to π over a
period.

The sign follows DLMF 28.2.29: ce_m(0) and se_m′(0) are above 0. For q above 0 the functions
gather about x = 90° and are exponentially small at 0, so a sum for ce_m(0) cancels to nothing.
There the sign is set at 90° instead, where the sum is largest and the count of zeros between
0° and 90° fixes it for every q.

**Accuracy.** mpmath has no Mathieu functions, so the reference values are the same eigenvalues
and eigenvectors found at 60 digits (the script is in the fixtures' README). The characteristic
values are measured against the larger of |a| and |q ∂a/∂q|, and the functions against the
largest of |f|, |q ∂f/∂q|, |x ∂f/∂x| and the sum of the sizes of their Fourier terms. Where a
function is exponentially small, away from where it gathers, those terms cancel, and the
function keeps the precision of the largest. The tests reach orders to 40 and q from −1,000 to
1,000. The worst error is 1 ULP for the characteristic values and 7 for the functions. SciPy
misses 34 of the 1,254 cases by more than 64 ULP, and gives `mathieu_a(22, -265.6)` as 92.3
where it is 569.5. A call costs about 7 µs, against SciPy's 1 µs, most of it in the bisection.

### Radial functions and coefficients

| Function | Result | SciPy |
| --- | --- | --- |
| `mathieu_modcem1(m, q, z)` | The even radial function of the first kind, Mc⁽¹⁾_m(z, q), and its derivative in z, as a tuple | `scipy.special.mathieu_modcem1` |
| `mathieu_modcem2(m, q, z)` | The even radial function of the second kind, Mc⁽²⁾_m(z, q), and its derivative | `scipy.special.mathieu_modcem2` |
| `mathieu_modsem1(m, q, z)` | The odd radial function of the first kind, Ms⁽¹⁾_m(z, q), and its derivative | `scipy.special.mathieu_modsem1` |
| `mathieu_modsem2(m, q, z)` | The odd radial function of the second kind, Ms⁽²⁾_m(z, q), and its derivative | `scipy.special.mathieu_modsem2` |
| `mathieu_even_coef(m, q)` | The Fourier coefficients of ce_m, A₀, A₂, … or A₁, A₃, …, as an `Array[f64, 1]` as long as SciPy's | `scipy.special.mathieu_even_coef` |
| `mathieu_odd_coef(m, q)` | The Fourier coefficients of se_m, B₂, B₄, … or B₁, B₃, … | `scipy.special.mathieu_odd_coef` |

The radial functions solve the modified equation w″ − (a − 2q cosh 2z) w = 0 (DLMF 28.20),
which Mathieu's equation becomes at x = iz, with the same characteristic value. In an elliptic
membrane they carry the wave from the centre outward, as the angular functions carry it around:
Mc⁽¹⁾ and Ms⁽¹⁾ are regular at the focal line z = 0, like the Bessel function J, and Mc⁽²⁾ and
Ms⁽²⁾ are the second solutions, like Y. They are given for q above 0 only, as SciPy gives them.

```python
from special import mathieu_even_coef, mathieu_modcem1


def main() -> Raises[None, ValueError]:
    value, slope = mathieu_modcem1(2.0, 1.5, 0.7)
    print(value)                             # 0.5011385205497332
    print(mathieu_even_coef(2.0, 1.5)[1])    # 0.9061913477138984
```

**How they work.** Each radial function is a sum, over the angular function's Fourier
coefficients, of products of Bessel functions of h e^(−z) and h e^z, with h = √q (DLMF 28.24.1
to 28.24.4). The products are centred on the largest coefficient, as DLMF advises, so that
the terms do not cancel there. J comes from `jn` at each order and Y from the upward
recurrence, which is stable for Y. The coefficient functions return the same coefficients
`mathieu_cem` and `mathieu_sem` sum, as many as SciPy returns.

**Accuracy.** The reference values are the same sums at 60 digits, over the 60-digit
coefficients, with mpmath's Bessel functions. Each is measured against the largest of |f|,
|q ∂f/∂q|, |z ∂f/∂z| and the sum of the sizes of its Bessel products, and a coefficient against
the largest coefficient. The tests reach orders to 12, q from 0.001 to 200 and z from −1 to 3.
The worst error is 16 ULP, for `mathieu_modsem1` near z = 0, where the rounding of h e^(∓z)
moves the two products of each term apart; the coefficients are within 3 ULP. SciPy misses 124
of the 894 cases by more than 64 ULP, and gives `mathieu_modsem1(10, 0.00135, 0.362)` as
−1.4e-10 where it is 4.5e-23. A call costs about 15 µs, against SciPy's 5 µs.

### At the ends

- An order that is not a whole number, or below 0 for `mathieu_a`, `mathieu_cem` and the
  `modcem` functions or below 1 for the others, gives NaN, as SciPy gives. So do a NaN argument
  and an infinite x.
- The radial functions give NaN for q at most 0, as SciPy gives. For z below 0 the first kind
  follows its parity, Mc⁽¹⁾ even and Ms⁽¹⁾ odd, and the second kind is the same series, as in
  SciPy.
- `mathieu_even_coef` and `mathieu_odd_coef` raise `ValueError` for an order their kind does
  not allow and for q below 0, as SciPy does.

## Spheroidal wave functions

| Function | Result | SciPy |
| --- | --- | --- |
| `pro_cv(m, n, c)` | The prolate characteristic value λ_mn(c), for whole m and n with 0 ≤ m ≤ n | `scipy.special.pro_cv` |
| `obl_cv(m, n, c)` | The oblate characteristic value λ_mn(c) | `scipy.special.obl_cv` |
| `pro_cv_seq(m, n, c)` | λ_mm(c), λ_m(m+1)(c), …, λ_mn(c), as an `Array[f64, 1]` | `scipy.special.pro_cv_seq` |
| `obl_cv_seq(m, n, c)` | The oblate λ_mm(c), …, λ_mn(c) | `scipy.special.obl_cv_seq` |
| `pro_ang1(m, n, c, x)` | The prolate angular function S_mn(c, x) and its derivative in x, as a tuple, for −1 < x < 1 | `scipy.special.pro_ang1` |
| `obl_ang1(m, n, c, x)` | The oblate angular function and its derivative | `scipy.special.obl_ang1` |
| `pro_ang1_cv(m, n, c, cv, x)` | `pro_ang1` with the characteristic value given, so that it is not found again for each x | `scipy.special.pro_ang1_cv` |
| `obl_ang1_cv(m, n, c, cv, x)` | `obl_ang1` with the characteristic value given | `scipy.special.obl_ang1_cv` |
| `pro_rad1(m, n, c, x)` | The prolate radial function of the first kind R⁽¹⁾_mn(c, x) and its derivative in x, for x > 1 and c > 0 | `scipy.special.pro_rad1` |
| `pro_rad2(m, n, c, x)` | The prolate radial function of the second kind R⁽²⁾_mn(c, x) and its derivative | `scipy.special.pro_rad2` |
| `obl_rad1(m, n, c, x)` | The oblate radial function of the first kind and its derivative, for x ≥ 0 and c > 0 | `scipy.special.obl_rad1` |
| `obl_rad2(m, n, c, x)` | The oblate radial function of the second kind and its derivative | `scipy.special.obl_rad2` |
| `pro_rad1_cv`, `pro_rad2_cv`, `obl_rad1_cv`, `obl_rad2_cv` `(m, n, c, cv, x)` | The radial functions with the characteristic value given | `scipy.special.pro_rad1_cv` and the rest |

The wave equation separates in prolate spheroidal coordinates, which fit a cigar, an elongated
nucleus or a pair of charges, and in oblate ones, which fit a disc or a flattened planet. The
angular part is the spheroidal wave equation ((1 − x²) w′)′ + (λ − γ²x² − m²/(1 − x²)) w = 0
(DLMF 30.2.1), with γ² = c² for prolate spheroids and −c² for oblate ones, and c half the focal
distance times the wavenumber. Its solutions finite at x = ±1 exist only for the characteristic
values λ_mn(c), one for each whole n ≥ m. At c = 0 they are the associated Legendre functions
P^m_n(x) and λ is n(n + 1). As in SciPy, P^m_n has no Condon–Shortley phase (−1)^m, so it is
`lpmv(m, n, x)` times (−1)^m.

```python
from special import obl_cv, pro_ang1, pro_cv


def main():
    print(pro_cv(1.0, 2.0, 2.5))    # 8.53103224320361
    print(obl_cv(1.0, 2.0, 2.5))    # 3.1669885590311315
    value, slope = pro_ang1(1.0, 2.0, 2.5, 0.3)
    print(value)                    # 0.8264370864845382
    print(slope)                    # 2.2715029151580763
```

**How they work.** Each angular function is a series Σ d_r P^m_{m+r}(x), with r running over
the even or the odd numbers as n − m does, and its coefficients satisfy a three-term recurrence
(DLMF 30.8, Flammer's 3.1.4). Scaled, the recurrence is a symmetric tridiagonal matrix, and the
characteristic value is its eigenvalue of rank (n − m)/2 among those of the same parity, found
as the Mathieu values are: bisection to within ¼, Rayleigh quotient iteration, and a twisted
factorization for the coefficients. The functions are scaled as Flammer scales them, and SciPy
after him: S_mn(c, 0) = P^m_n(0) when n − m is even and S′_mn(c, 0) = P^m_n′(0) when it is
odd.

That scaling needs the series at 0, and for oblate functions at large c the series cancels
there: they gather near x = ±1 and are exponentially small at 0, so the sum loses a factor of
10¹⁸ at c = 60. Baryon finds the scale instead at a point x₀ where the Legendre series does not
cancel, and carries it back to 0 with the power series of the solution about 0, which grows
from 0 to x₀ without cancelling. SciPy sums at 0.

**Accuracy.** mpmath has no spheroidal functions, so the reference values are the same
eigenvalues and coefficients found at 60 digits, with the precision raised wherever the sum at 0
cancels, scaled by that sum (the script is in the fixtures' README). The characteristic values
are measured against the larger of |λ| and |c ∂λ/∂c|, and the functions against the largest of
|S|, |c ∂S/∂c|, |x ∂S/∂x| and the sum of the sizes of their Legendre terms. The tests reach
orders to 30, n − m to 40, c to 250 and x near ±1. The worst error is 8 ULP for the functions
and 22 for the characteristic values: λ is an eigenvalue of a matrix with entries as large as
c²/2, and for prolate c above about 100, where λ is near c(2(n − m) + 1), about c²/(2λ)
ULP are lost, in SciPy as here. Of the 930 cases, SciPy misses 384 by more than 64 ULP: 156
because it takes a c below 0 as 0, 154 angular functions, some by 10⁴⁰ times, and 74 at c
above 170, where it crashes the program. A call costs about 5 µs, against SciPy's 10 µs.

### At the ends

- An m or n that is not a whole number, an m below 0, an n below m, or an n more than 198
  above m gives NaN, as SciPy gives. So does a c that is not finite, and an x outside
  −1 < x < 1.
- λ depends on c², so a c below 0 gives what |c| gives. SciPy takes such a c as 0 and gives
  the Legendre function.
- `pro_cv_seq` and `obl_cv_seq` give an empty array for n below m and raise `ValueError` when
  m or n is not a whole number, m is below 0, n − m is above 199 or c is not finite. SciPy gives
  values for an m below 0, which no function has, and nothing at all for a c that is NaN.
- The `_cv` forms take the characteristic value as given. For any value other than the one
  `pro_cv` or `obl_cv` returns, the result belongs to no solution and differs from SciPy's.
- The tests reach c = 250. For larger c the oblate scale is found nearer and nearer x = ±1,
  and the error it is expected to leave grows; where that passes 10⁴ ULP, from c of about 300
  for n − m near 20 and about 720 for small n − m, the oblate angular functions give NaN.
  There the functions near ±1 approach the largest `f64` in any case: 3e425 at
  `obl_ang1(1, 3, 1000, 0.999)`. The prolate functions are scaled at 0 for every c.

### Radial functions

The radial part of the same separation is the radial spheroidal equation, with ξ the radial
coordinate: ξ > 1 for prolate spheroids, where ((ξ² − 1) R′)′ − (λ − c²ξ² + m²/(ξ² − 1)) R = 0,
and ξ ≥ 0 for oblate ones, where ((ξ² + 1) R′)′ − (λ − c²ξ² − m²/(ξ² + 1)) R = 0. λ is the same
characteristic value. The first kind R⁽¹⁾ is the solution that behaves like the spherical Bessel
function j_n(cξ) far out, and the second kind R⁽²⁾ the one that behaves like y_n(cξ), so that
R⁽¹⁾ + i R⁽²⁾ is an outgoing wave, the one a scattering problem matches at the surface ξ = ξ₀.

```python
from special import obl_rad2, pro_rad1


def main():
    value, slope = pro_rad1(1.0, 2.0, 2.5, 1.5)
    print(value)    # 0.31149518137922233
    print(slope)    # 0.07075345545840275
    value, slope = obl_rad2(1.0, 2.0, 2.5, 0.5)
    print(value)    # -0.5863137687362597
    print(slope)    # 0.859163447108363
```

**How they work.** Each radial function is a series over the angular coefficients d_r, of
spherical Bessel functions j (first kind) or y (second kind) of cξ (DLMF 30.11.3 and 30.11.7),
divided by the normalizing sum Σ d_r (r + 2m)!/r!. The series of j converges everywhere. The
series of y converges only far enough out, so nearer in Baryon starts from the series at the
nearest point where it is exact and carries R and R′ inward along the equation by Taylor steps,
each at most 3/c long and at most half the distance to the nearest singular point (ξ = 1, or ±i
for oblate spheroids). The module documentation of `special_spheroidal_radial` gives the
details and the measurements behind the step length.

**Accuracy.** The reference values are the same series and steps at 60 digits (the script is in
the fixtures' README). Each case is measured against the largest of |R|, |c ∂R/∂c|, |ξ ∂R/∂ξ|,
the modulus √(R⁽¹⁾² + R⁽²⁾²) the two kinds oscillate under, the sum of the sizes of the Bessel
terms, and |R| times the cancellation of the normalizing sum. The tests reach orders to 22, c
from 0.01 to 50, prolate ξ from 1 + 10⁻⁶ and oblate ξ from 0, both to 30. The worst error is 54
ULP, for the derivative of `obl_rad2(7, 8, 30.6, 0)`: every inward step rounds by about one
unit of the modulus. Of the 604 cases with values, SciPy misses 150 by more than 128 ULP: 122
for `obl_rad2`, 24 for `pro_rad2` and 4 for `obl_rad1`. Its second kinds near the spheroid are
often wrong in the leading digit: `obl_rad2(1, 2, 2.5, 0.5)` is −0.249 against −0.586, and
`pro_rad2(1, 2, 2.5, 1.05)` is −1.545 against −1.597.

### At the ends of the radial functions

- The m, n and characteristic value are checked as for the angular functions. A c that is not
  above 0, or not finite, gives NaN: the series are in cξ. So does a prolate ξ at most 1 or an
  oblate ξ below 0.
- At ξ = ∞ both kinds and their derivatives are 0. At ξ = 0 the oblate first kind is the limit
  of its series.
- For prolate spheroids the normalizing sum cancels by about e^c: 3.7 × 10³ at c = 10 and
  6.7 × 10⁷ at c = 20 for low n − m. Where it cancels by more than 10⁸, so that fewer than 8
  digits would be left, the prolate functions give NaN; in the tests that is
  `pro_rad1(7, 8, 30.6, 23.4)` and its second kind. SciPy gives a value there. The oblate sum
  cancels by about 2 at most.

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
- **ζ(x)** is an Euler–Maclaurin sum from 0 up, which stops summing powers directly once the
  rest is below 1e-17 of the sum, and the reflection formula below 0, with
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
- **The complete integrals** K(m) and E(m) are P(p) − log(p) Q(p) in p = 1 − m, the form both
  take near m = 1, with polynomials P and Q of degree 9 for K and 10 for E fitted with mpmath.
  Below m = 0 they use K(m) = K(m/(m − 1))/√(1 − m) and E(m) = √(1 − m) E(m/(m − 1)), where
  1 − m/(m − 1) is 1/(1 − m). `ellipkm1` takes `p` directly, so K stays exact as `p` goes to
  1e-300.
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
to μ₀ exactly. These are left as computed, each to its own precision, so their sum can differ
from μ₀ in the last few digits (by 2.5e-13 of it for `roots_gegenbauer(150, -0.4)`), but each
weight is far closer to the true one: at 150 nodes SciPy's are off by up to 1.2 million ULP,
and these by under 3,000. A weight's error comes mostly from its node's
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
| `special_gamma` | `gamma`, `rgamma`, `gammasgn`, `gammaln`, `loggamma`, `psi`, `digamma`, `polygamma` and `factorial`, which `polygamma` uses |
| `special_hurwitz` | The Hurwitz zeta function, which `polygamma` and `zeta` sum |
| `special_beta` | `beta`, `betaln` and `poch` |
| `special_error` | `erf`, `erfc`, `erfcx`, `erfinv`, `erfcinv`, `ndtr`, `log_ndtr` and `dawsn` |
| `special_ndtri` | `ndtri` and the three rational functions it is made of |
| `special_bessel_jy` | `j0`, `j1`, `jn`, `y0`, `y1` and `yn` |
| `special_bessel_ik` | `i0`, `i1`, `i0e`, `i1e`, `k0`, `k1`, `k0e`, `k1e` and `kn` |
| `special_bessel_steed` | `bessel_jy` and `bessel_ik`, the methods behind the functions of real order: Temme and Steed's, the power series and Hankel's expansions |
| `special_bessel_real` | `jv`, `yv`, `iv`, `kv`, `jve`, `yve`, `ive`, `kve`, `jvp`, `yvp`, `ivp` and `kvp` |
| `special_bessel_spherical` | `spherical_jn`, `spherical_yn`, `spherical_in` and `spherical_kn` |
| `special_airy` | `airy` and `airye` |
| `special_kelvin` | `kelvin`, `ber`, `bei`, `ker`, `kei`, `berp`, `beip`, `kerp` and `keip` |
| `special_struve` | `struve` and `modstruve` |
| `special_hyp0f1` | `hyp0f1` |
| `special_hyp1f1` | `hyp1f1` |
| `special_hyperu` | `hyperu` |
| `special_hyp2f1` | `hyp2f1` |
| `special_parabolic` | `pbdv`, `pbvv` and `pbwa` |
| `special_incomplete_gamma` | `gammainc`, `gammaincc`, `gammaincinv` and `gammainccinv` |
| `special_incomplete_beta` | `betainc`, `betaincc`, `betaincinv` and `betainccinv` |
| `special_distributions` | `bdtr`, `bdtrc`, `bdtri`, `nbdtr`, `nbdtrc`, `nbdtri`, `pdtr`, `pdtrc`, `pdtri`, `chdtr`, `chdtrc`, `chdtri`, `fdtr`, `fdtrc`, `fdtri`, `gdtr`, `gdtrc`, `stdtr` and `stdtrit` |
| `special_kolmogorov` | `kolmogorov`, `kolmogi`, `smirnov`, `smirnovi` and `owens_t` |
| `special_helpers` | `expit`, `log_expit`, `logit`, `xlogy`, `xlog1py`, `entr`, `rel_entr`, `kl_div`, `binom`, `comb`, `perm`, `logsumexp` and `softmax` |
| `special_integrals` | `exp1`, `expi`, `expn`, `sici`, `shichi` and `fresnel` |
| `special_zeta` | `zeta`, `zetac` and `spence` |
| `special_lambertw` | `lambertw` |
| `special_mathieu` | `mathieu_a`, `mathieu_b`, `mathieu_cem` and `mathieu_sem` |
| `special_mathieu_radial` | `mathieu_modcem1`, `mathieu_modcem2`, `mathieu_modsem1`, `mathieu_modsem2`, `mathieu_even_coef` and `mathieu_odd_coef` |
| `special_spheroidal` | `pro_cv`, `obl_cv`, `pro_cv_seq`, `obl_cv_seq`, `pro_ang1`, `obl_ang1`, `pro_ang1_cv` and `obl_ang1_cv` |
| `special_spheroidal_radial` | `pro_rad1`, `pro_rad2`, `obl_rad1`, `obl_rad2` and their `_cv` forms |
| `special_orthogonal` | `eval_legendre`, `eval_chebyt`, `eval_chebyu`, `eval_hermite`, `eval_hermitenorm`, `eval_laguerre`, `eval_genlaguerre`, `eval_jacobi`, `eval_gegenbauer`, `lpmv` and `sph_harm_y` |
| `special_carlson` | `elliprc`, `elliprd`, `elliprf`, `elliprg` and `elliprj`, and `carlson_rf`, `carlson_rd` and `carlson_rg`, the same integrals without the checks of their arguments |
| `special_elliptic` | `ellipk`, `ellipkm1`, `ellipe`, `ellipkinc`, `ellipeinc` and `ellipj` |
| `special_quadrature` | `roots_legendre`, `roots_chebyt`, `roots_chebyu`, `roots_hermite`, `roots_hermitenorm`, `roots_laguerre`, `roots_genlaguerre`, `roots_jacobi` and `roots_gegenbauer` |

A new function goes in the file of its family, and a name of `special` gets its line in
`special.byn`. A family file kept under about 500 lines is split again by responsibility.

## Not built yet

The rest of `scipy.special` comes in the sub-rows of roadmap 3.18, Bessel functions of a complex
argument (3.18.6) among them. The hypergeometric functions and Carlson's integrals of complex
arguments are not built. `loggamma` of a complex argument comes with the complex Bessel functions.
See [What builds today](status.md).
