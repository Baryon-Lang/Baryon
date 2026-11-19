# Special-function reference values

`gamma_family.txt`, `error_functions.txt`, `bessel_functions.txt`, `incomplete_functions.txt`,
`distribution_functions.txt`, `helper_functions.txt`, `integral_functions.txt` and `lambertw.txt`
hold the cases the build tests check the `special` module against: those in
[`../../build/special.rs`](../../build/special.rs) its gamma family (roadmap 3.18.1), error
functions (3.18.3), Bessel functions of whole order (3.18.4), incomplete gamma and beta
functions (3.18.2) and distribution functions (3.18.12), and those in
[`../../build/special_everyday.rs`](../../build/special_everyday.rs) its everyday helpers,
exponential integrals, zeta and Spence's function, and Lambert W function (3.18.11), and those in
[`../../build/special_elliptic.rs`](../../build/special_elliptic.rs) its elliptic integrals and
functions (3.18.9), from `elliptic_functions.txt`, and those in
[`../../build/special_polynomials.rs`](../../build/special_polynomials.rs) its orthogonal
polynomials and spherical harmonics (3.18.10), from `orthogonal_polynomials.txt`, and its Gauss
quadrature rules (3.18.10), from `gauss_quadrature.txt`, and those in
[`../../build/special_bessel.rs`](../../build/special_bessel.rs) its Bessel functions of real
order and spherical Bessel functions (3.18.5), from `bessel_real.txt`, and those in
[`../../build/special_waves.rs`](../../build/special_waves.rs) its Airy, Kelvin and Struve
functions (3.18.7), from `airy.txt`, `kelvin.txt` and `struve.txt`, and those in
[`../../build/special_hypergeometric.rs`](../../build/special_hypergeometric.rs) its confluent
hypergeometric functions (3.18.8), from `hypergeometric.txt`, and the Gauss function `hyp2f1`,
from `gauss.txt`, and those in
[`../../build/special_parabolic.rs`](../../build/special_parabolic.rs) its parabolic cylinder
functions (3.18.14), from `parabolic.txt`. Each line is a
function's name, its arguments and the value the function should give, separated by spaces and
written as Python's `repr` writes a float:

```text
gamma -16.481439370601617 -3.8732193888774417e-14
polygamma 3 -8.996117952868211 26418492688.871017
```

Most values were computed with mpmath 1.4.1 at 50 digits and rounded to `f64`, at arguments drawn at
random, with a fixed seed, over each function's range. The last lines are the poles, zeros and
infinities, where the value is SciPy 1.18.1's, which the module gives exactly, except the Bessel
functions' limits at infinity, `pdtri(k, 1.0)` and `stdtrit(df, 0.0)`, where SciPy gives NaN or the
wrong infinity and the file holds the limit. The incomplete beta function's values were computed at
80 digits, so a value far in a tail keeps its own, and each inverse's value is the root mpmath finds
of the forward function. Neither mpmath nor SciPy is needed to run the tests.

## The coefficients in `special.byn`

The Taylor coefficients, Bernoulli numbers and constants in
[`special.byn`](../../../library/special.byn) were computed with mpmath too. They are values of
mathematical functions, rounded to `f64`, and carry no third-party code. To compute them again:

```text
import mpmath as mp
mp.mp.dps = 50
def f(x): return repr(float(x))
def horner(name, cs, var, indent="    "):
    # value = cs[0] + var*(cs[1] + ...)
    lines=[f"{indent}{name} = {f(cs[-1])}"]
    for c in reversed(cs[:-1]):
        lines.append(f"{indent}{name} = {name} * {var} + {f(c)}")
    return "\n".join(lines)
def taylor(fn, c, radius, tol=2.0**-62):
    cs = mp.taylor(fn, c, 60); out=[]
    for k,a in enumerate(cs):
        out.append(a)
        if k>3 and abs(a)*radius**k < tol: break
    return out
c1 = taylor(mp.rgamma,1,0.5); c2=taylor(mp.rgamma,2,0.5)
print("# rgamma(1+p)-1 / p");print(horner("q", c1[1:], "p"))
print("# rgamma(2+p)-1 / p");print(horner("q", c2[1:], "p"))
# Stirling: sum B2k/(2k(2k-1) x^(2k-1)), in w=1/x^2
st=[mp.bernoulli(2*k)/(2*k*(2*k-1)) for k in range(1,11)]
print("# stirling");print(horner("s", st, "w"))
ps=[mp.bernoulli(2*k)/(2*k) for k in range(1,11)]
print("# psi asym");print(horner("s", ps, "w"))
x0 = mp.findroot(mp.digamma, 1.46)
hi = float(x0); lo = float(x0-hi)
print("# root", repr(hi), repr(lo))
cs=[mp.polygamma(k,x0)/mp.factorial(k) for k in range(1,40)]
em=[mp.bernoulli(2*j)/mp.factorial(2*j) for j in range(1,13)]
print("# EM", [f(x) for x in em])
print("# half log 2pi", f(mp.log(2*mp.pi)/2), "sqrt2pi", f(mp.sqrt(2*mp.pi)))
r=0.55; out=[]
for k,a in enumerate(cs):
    out.append(a)
    if abs(a)*r**(k+1) < 2.0**-60*0.1: break
print("# psi root series r=.55", len(out));print(horner("s", out, "d"))
```

Each block is a Horner loop, highest power first, pasted into the function the comment names:
`rgamma(1+p)-1 / p` and `rgamma(2+p)-1 / p` into `_rgamma_minus_one`, `stirling` into
`_stirling`, `psi asym` into `psi`, `psi root series r=.55` into `_psi_near_root`, and `EM` into
`_hurwitz_zeta`. The error functions' coefficients come from this script, its blocks pasted
into `_erf_small` and the branches of `_erfcx_positive`:

```text
import mpmath as mp
mp.mp.dps = 50
def f(x): return repr(float(x))
def horner(name, cs, var, indent):
    lines=[f"{indent}{name} = {f(cs[-1])}"]
    for c in reversed(cs[:-1]):
        lines.append(f"{indent}{name} = {name} * {var} + {f(c)}".replace("+ -","- "))
    return "\n".join(lines)
# erf(x) = x * sum a_n (x^2)^n for |x| <= 0.5
a=[2/mp.sqrt(mp.pi)*(-1)**n/(mp.factorial(n)*(2*n+1)) for n in range(30)]
n=next(k for k in range(30) if abs(a[k])*0.25**k < 2.0**-60)
print("# erf"); print(horner("s", a[:n+1], "w", "    "))
g=lambda x: mp.exp(x*x)*mp.erfc(x)
for i,c in enumerate([0.75,1.25,1.75,2.25,2.75]):
    cs=mp.taylor(g,c,60)
    k=next(k for k in range(4,60) if abs(cs[k])*0.25**k < 2.0**-60*abs(cs[0]))
    print(f"# erfcx {c}"); print(horner("s", cs[:k+1], "d", "        "))
print("# 2/sqrtpi", f(2/mp.sqrt(mp.pi)), "1/sqrtpi", f(1/mp.sqrt(mp.pi)), "sqrt2", f(mp.sqrt(2)), "1/sqrt2 hi lo", f(1/mp.sqrt(2)), f(1/mp.sqrt(2)-mp.mpf(float(1/mp.sqrt(2)))), "logsqrtpi", f(mp.log(mp.sqrt(mp.pi))))
```

## Recording the cases again

Run this in this directory, with mpmath, NumPy and SciPy installed. It writes every file;
each family's cases are drawn after the ones before, so adding a family leaves the earlier
files as they were:

```text
import math
import mpmath
import numpy
import scipy.special

mpmath.mp.dps = 50
generator = numpy.random.default_rng(3018)


def uniform(low, high, count):
    return [float(x) for x in generator.uniform(low, high, count)]


def spread(low, high, count):
    """Arguments spread evenly in their logarithm, from `low` to `high`."""
    return [float(math.exp(x)) for x in generator.uniform(math.log(low), math.log(high), count)]


def pairs(first, second):
    return list(zip(first, second))


def log_abs_gamma(x):
    return mpmath.log(abs(mpmath.gamma(x)))


def log_abs_beta(a, b):
    return mpmath.log(abs(mpmath.beta(a, b)))


one = {
    "gamma": (mpmath.gamma, uniform(-30, -0.5, 60) + uniform(-0.5, 0.5, 40) + uniform(0.5, 3, 60)
              + uniform(3, 10, 40) + uniform(10, 171, 60) + spread(1e-300, 1e-3, 20)),
    "rgamma": (mpmath.rgamma, uniform(-30, -0.5, 60) + uniform(-0.5, 3, 60) + uniform(3, 178, 80)),
    "gammaln": (log_abs_gamma, uniform(-30, -0.5, 60) + uniform(-0.5, 0.5, 40) + uniform(0.5, 3, 60)
                + uniform(0.9, 1.1, 30) + uniform(1.9, 2.1, 30) + uniform(3, 10, 40)
                + spread(10, 1e300, 60) + spread(1e-300, 1e-3, 20)),
    "psi": (mpmath.digamma, uniform(-30, -0.01, 60) + uniform(0.01, 3, 60) + uniform(1.3, 1.6, 30)
            + uniform(3, 10, 40) + spread(10, 1e300, 60) + spread(1e-300, 1e-3, 20)),
}
two = {
    "beta": (mpmath.beta, pairs(spread(1e-3, 500, 200), spread(1e-3, 500, 200))
             + pairs(uniform(-10, 10, 60), uniform(-10, 10, 60))),
    "betaln": (log_abs_beta, pairs(spread(1e-3, 1e10, 200), spread(1e-3, 1e10, 200))),
    "poch": (mpmath.rf, pairs(spread(1e-3, 1e6, 150), uniform(-5, 5, 150))
             + pairs(uniform(-10, 10, 80), uniform(-10, 10, 80))),
}
lines = []
for name, (function, arguments) in one.items():
    for x in arguments:
        lines.append(f"{name} {x!r} {float(function(x))!r}")
for name, (function, arguments) in two.items():
    for a, b in arguments:
        lines.append(f"{name} {a!r} {b!r} {float(function(a, b))!r}")
for n in range(1, 8):
    for x in uniform(0.01, 40, 25):
        lines.append(f"polygamma {n} {x!r} {float(mpmath.polygamma(n, x))!r}")
for n in range(1, 5):
    for x in uniform(-10, -0.01, 15):
        lines.append(f"polygamma {n} {x!r} {float(mpmath.polygamma(n, x))!r}")

# Poles, zeros and infinities: SciPy's values, which each function here gives exactly.
inf = math.inf
edges = [
    ("gamma", (0.0,)), ("gamma", (-0.0,)), ("gamma", (-1.0,)), ("gamma", (inf,)),
    ("gamma", (-inf,)), ("gamma", (5.0,)), ("gamma", (23.0,)), ("gamma", (172.0,)),
    ("rgamma", (0.0,)), ("rgamma", (-0.0,)), ("rgamma", (-2.0,)), ("rgamma", (inf,)),
    ("rgamma", (-inf,)), ("rgamma", (-180.5,)),
    ("gammaln", (0.0,)), ("gammaln", (-1.0,)), ("gammaln", (inf,)), ("gammaln", (-inf,)),
    ("gammaln", (1.0,)), ("gammaln", (2.0,)),
    ("loggamma", (0.0,)), ("loggamma", (-0.5,)), ("loggamma", (3.0,)),
    ("gammasgn", (0.0,)), ("gammasgn", (-0.0,)), ("gammasgn", (-1.0,)), ("gammasgn", (-0.5,)),
    ("gammasgn", (-1.5,)), ("gammasgn", (2.5,)), ("gammasgn", (-inf,)), ("gammasgn", (inf,)),
    ("psi", (0.0,)), ("psi", (-0.0,)), ("psi", (-1.0,)), ("psi", (inf,)), ("psi", (-inf,)),
    ("digamma", (1.0,)),
    ("beta", (0.0, 1.0)), ("beta", (1.0, 0.0)), ("beta", (-1.0, 2.0)), ("beta", (-1.0, -1.0)),
    ("beta", (2.0, -1.0)), ("beta", (inf, 1.0)), ("beta", (2.0, 3.0)),
    ("betaln", (0.0, 1.0)), ("betaln", (-1.0, 2.0)),
    ("poch", (0.0, 0.0)), ("poch", (-1.0, 1.0)), ("poch", (-1.0, 2.0)), ("poch", (0.0, 1.0)),
    ("poch", (1.0, -1.0)), ("poch", (3.0, inf)), ("poch", (3.0, 2.0)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
for n, x in [(1, 0.0), (1, -1.0), (2, -1.0), (1, inf), (-1, 1.0), (0, 1.0)]:
    value = float(scipy.special.polygamma(n, x))
    lines.append(f"polygamma {n} {x!r} {value!r}")

with open("gamma_family.txt", "w") as out:
    out.write("\n".join(lines) + "\n")

# The error functions (roadmap 3.18.3), drawn after the gamma family so its cases stay as they
# were recorded.
sqrt2 = mpmath.sqrt(2)
error_functions = {
    "erf": (mpmath.erf, uniform(-0.5, 0.5, 60) + uniform(-6, 6, 100) + spread(1e-300, 1e-3, 20)),
    "erfc": (mpmath.erfc, uniform(-6, 0.5, 60) + uniform(0.5, 3, 60) + uniform(3, 27, 80)),
    "erfcx": (lambda x: mpmath.exp(mpmath.mpf(x) ** 2) * mpmath.erfc(x) if x < 1e6 else
              (1 - 1 / (2 * mpmath.mpf(x) ** 2)) / (x * mpmath.sqrt(mpmath.pi)),
              uniform(-26, 0.5, 60) + uniform(0.5, 3, 60) + uniform(3, 30, 40)
              + spread(30, 1e300, 30)),
    "erfinv": (mpmath.erfinv, uniform(-1, 1, 120) + [1 - x for x in spread(1e-16, 0.5, 40)]),
    "erfcinv": (lambda q: mpmath.erfinv(1 - mpmath.mpf(q)) if q > 1e-15 else
                mpmath.findroot(lambda x: mpmath.log(mpmath.erfc(x)) - mpmath.log(q),
                                mpmath.sqrt(-mpmath.log(q))),
                spread(1e-300, 0.5, 80) + uniform(0.5, 2, 60)),
    "ndtr": (lambda x: mpmath.ncdf(x), uniform(-38, 8, 150)),
    "ndtri": (lambda p: -sqrt2 * mpmath.erfinv(1 - 2 * mpmath.mpf(p)) if p > 1e-15 else
              mpmath.findroot(lambda x: mpmath.log(mpmath.ncdf(x)) - mpmath.log(p),
                              -mpmath.sqrt(-2 * mpmath.log(p))),
              spread(1e-300, 0.5, 80) + uniform(0.5, 1, 60)),
    "log_ndtr": (lambda x: mpmath.log(mpmath.ncdf(x)) if x > -1e6 else
                 -mpmath.mpf(x) ** 2 / 2 - mpmath.log(-x * mpmath.sqrt(2 * mpmath.pi))
                 - 1 / mpmath.mpf(x) ** 2, uniform(-40, 10, 100)
                 + [-x for x in spread(40, 1e100, 30)]),
    "dawsn": (lambda x: mpmath.sqrt(mpmath.pi) / 2 * mpmath.exp(-mpmath.mpf(x) ** 2) * mpmath.erfi(x),
              uniform(-10, 10, 150) + spread(10, 1e6, 20)),
}
lines = []
for name, (function, arguments) in error_functions.items():
    for x in arguments:
        lines.append(f"{name} {x!r} {float(function(x))!r}")
edges = [
    ("erf", (inf,)), ("erf", (-inf,)), ("erf", (0.0,)), ("erfc", (inf,)), ("erfc", (-inf,)),
    ("erfcx", (inf,)), ("erfcx", (-inf,)), ("erfinv", (1.0,)), ("erfinv", (-1.0,)),
    ("erfinv", (1.5,)), ("erfinv", (0.0,)), ("erfcinv", (0.0,)), ("erfcinv", (2.0,)),
    ("erfcinv", (-1.0,)), ("erfcinv", (1.0,)), ("ndtr", (inf,)), ("ndtr", (-inf,)),
    ("ndtri", (0.0,)), ("ndtri", (1.0,)), ("ndtri", (0.5,)), ("ndtri", (2.0,)),
    ("log_ndtr", (inf,)), ("log_ndtr", (-inf,)), ("dawsn", (inf,)), ("dawsn", (-inf,)),
    ("dawsn", (0.0,)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
with open("error_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
# Bessel functions of whole order (roadmap 3.18.4), drawn after the error functions.
lines = []
bessel_one = {
    "j0": (lambda x: mpmath.besselj(0, x), uniform(-2, 2, 40) + uniform(2, 25, 80)
           + uniform(25, 200, 40) + spread(200, 1e8, 20)),
    "j1": (lambda x: mpmath.besselj(1, x), uniform(-2, 2, 40) + uniform(2, 25, 80)
           + uniform(-200, -25, 40) + spread(200, 1e8, 20)),
    "y0": (lambda x: mpmath.bessely(0, x), spread(1e-300, 2, 40) + uniform(2, 25, 80)
           + uniform(25, 200, 40) + spread(200, 1e8, 20)),
    "y1": (lambda x: mpmath.bessely(1, x), spread(1e-300, 2, 40) + uniform(2, 25, 80)
           + uniform(25, 200, 40) + spread(200, 1e8, 20)),
    "i0e": (lambda x: mpmath.besseli(0, x) * mpmath.exp(-abs(mpmath.mpf(x))),
            uniform(-20, 20, 80) + spread(20, 1e8, 30)),
    "i1e": (lambda x: mpmath.besseli(1, x) * mpmath.exp(-abs(mpmath.mpf(x))),
            uniform(-20, 20, 80) + spread(20, 1e8, 30)),
    "i0": (lambda x: mpmath.besseli(0, x), uniform(-700, 700, 60)),
    "i1": (lambda x: mpmath.besseli(1, x), uniform(-700, 700, 60)),
    "k0e": (lambda x: mpmath.besselk(0, x) * mpmath.exp(x), spread(1e-300, 0.5, 30)
            + uniform(0.5, 20, 80) + spread(20, 1e8, 30)),
    "k1e": (lambda x: mpmath.besselk(1, x) * mpmath.exp(x), spread(1e-300, 0.5, 30)
            + uniform(0.5, 20, 80) + spread(20, 1e8, 30)),
    "k0": (lambda x: mpmath.besselk(0, x), spread(1e-300, 0.5, 30) + uniform(0.5, 700, 60)),
    "k1": (lambda x: mpmath.besselk(1, x), spread(1e-300, 0.5, 30) + uniform(0.5, 700, 60)),
}
for name, (function, arguments) in bessel_one.items():
    for x in arguments:
        lines.append(f"{name} {x!r} {float(function(x))!r}")
for n in [2, 3, 5, 10, 30, -4, -7]:
    for x in uniform(-60, 60, 20) + spread(1e-3, 1, 5):
        lines.append(f"jn {n} {x!r} {float(mpmath.besselj(n, x))!r}")
    for x in spread(0.05, 60, 20):
        lines.append(f"yn {n} {x!r} {float(mpmath.bessely(n, x))!r}")
        lines.append(f"kn {n} {x!r} {float(mpmath.besselk(n, x))!r}")
edges = [
    ("j0", (0.0,)), ("j1", (0.0,)), ("y0", (0.0,)), ("y0", (-1.0,)), ("y1", (0.0,)),
    ("y1", (-1.0,)), ("i0", (0.0,)), ("i0e", (inf,)), ("i1e", (inf,)), ("k0", (0.0,)),
    ("k0", (-1.0,)), ("k0", (inf,)), ("k0e", (0.0,)), ("k1", (0.0,)), ("k1e", (inf,)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
for name, n, x in [("jn", 3, 0.0), ("yn", 2, 0.0), ("yn", 2, -1.0), ("kn", 2, 0.0),
                   ("kn", 2, -1.0)]:
    value = float(getattr(scipy.special, name)(n, x))
    lines.append(f"{name} {n} {x!r} {value!r}")
# The limits at infinity, where SciPy 1.18 gives NaN: Jₙ and Yₙ fall to 0 and I₀ and I₁ grow
# without bound.
lines += ["j0 inf 0.0", "j1 -inf 0.0", "y0 inf 0.0", "i0 inf inf", "i1 -inf -inf",
          "jn 4 inf 0.0"]
with open("bessel_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")

# Incomplete gamma and beta functions and their inverses (roadmap 3.18.2), drawn after the
# Bessel functions.
lines = []


def lower_gamma(a, x):
    return mpmath.gammainc(a, 0, x, regularized=True)


def upper_gamma(a, x):
    return mpmath.gammainc(a, x, mpmath.inf, regularized=True)


def lower_beta(a, b, x):
    """I_x(a, b), with 80 digits of working precision so a tail value keeps its own."""
    with mpmath.workdps(80):
        return +mpmath.betainc(a, b, 0, x, regularized=True)


def upper_beta(a, b, x):
    """1 - I_x(a, b), as I_{1-x}(b, a) with 1 - x formed exactly."""
    with mpmath.workdps(80):
        return +mpmath.betainc(b, a, 0, 1 - mpmath.mpf(x), regularized=True)


def bisect(function, x):
    """The root of the increasing `function` near `x` in (0, 1), by bisection to 1e-40."""
    low = mpmath.mpf(x) * (1 - mpmath.mpf(1e-6))
    high = 1 - (1 - mpmath.mpf(x)) * (1 - mpmath.mpf(1e-6))
    while function(low) > 0:
        low /= 2
    while function(high) < 0:
        high = (1 + high) / 2
    for _ in range(400):
        middle = (low + high) / 2
        if function(middle) < 0:
            low = middle
        else:
            high = middle
        if high - low < mpmath.mpf(10) ** -40 * high:
            break
    return (low + high) / 2


shapes = spread(1e-3, 1e3, 150)
gamma_cases = [(a, float(a * math.exp(generator.uniform(-3, 2)))) for a in shapes]
gamma_cases += [(a, float(generator.uniform(0, 2))) for a in spread(1e-3, 1, 40)]
for a, x in gamma_cases:
    lines.append(f"gammainc {a!r} {x!r} {float(lower_gamma(a, x))!r}")
    lines.append(f"gammaincc {a!r} {x!r} {float(upper_gamma(a, x))!r}")
for a in spread(1e-2, 1e3, 80):
    x = float(a * math.exp(generator.uniform(-2, 1.5)))
    for name, function in [("gammaincinv", lower_gamma), ("gammainccinv", upper_gamma)]:
        target = float(function(a, x))
        if 0.0 < target < 1.0:
            root = mpmath.findroot(lambda t: function(a, t) - target, x)
            lines.append(f"{name} {a!r} {target!r} {float(root)!r}")
beta_cases = []
for a, b in zip(spread(1e-2, 500, 200), spread(1e-2, 500, 200)):
    mean = a / (a + b)
    x = float(min(max(mean * math.exp(generator.uniform(-2, 0.5)), 1e-12), 1 - 1e-12))
    beta_cases.append((a, b, x))
for a, b, x in beta_cases:
    lines.append(f"betainc {a!r} {b!r} {x!r} {float(lower_beta(a, b, x))!r}")
    lines.append(f"betaincc {a!r} {b!r} {x!r} {float(upper_beta(a, b, x))!r}")
for a, b, x in beta_cases[:120]:
    target = float(lower_beta(a, b, x))
    if 1e-300 < target < 1.0:
        root = bisect(lambda t: lower_beta(a, b, t) - target, x)
        lines.append(f"betaincinv {a!r} {b!r} {target!r} {float(root)!r}")
for a, b, x in beta_cases[:120]:
    target = float(upper_beta(a, b, x))
    if 1e-300 < target < 1.0:
        root = bisect(lambda t: target - upper_beta(a, b, t), x)
        lines.append(f"betainccinv {a!r} {b!r} {target!r} {float(root)!r}")
edges = [
    ("gammainc", (1.0, 0.0)), ("gammainc", (0.0, 1.0)), ("gammainc", (1.0, inf)),
    ("gammainc", (-1.0, 1.0)), ("gammaincc", (1.0, 0.0)), ("gammaincc", (1.0, inf)),
    ("gammaincinv", (1.0, 0.0)), ("gammaincinv", (1.0, 1.0)), ("gammaincinv", (1.0, 2.0)),
    ("gammainccinv", (1.0, 0.0)), ("gammainccinv", (1.0, 1.0)),
    ("betainc", (1.0, 2.0, 0.0)), ("betainc", (1.0, 2.0, 1.0)), ("betainc", (1.0, 2.0, 1.5)),
    ("betaincc", (1.0, 2.0, 0.0)), ("betaincc", (1.0, 2.0, 1.0)),
    ("betaincinv", (1.0, 2.0, 0.0)), ("betaincinv", (1.0, 2.0, 1.0)),
    ("betainccinv", (1.0, 2.0, 0.0)), ("betainccinv", (1.0, 2.0, 1.0)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
with open("incomplete_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")

# Distribution functions (roadmap 3.18.12), drawn after the incomplete functions.
lines = []


def regularized_beta(a, b, x, y):
    """I_x(a, b), with y = 1 - x given exactly, at 80 digits: the integral from the nearer end."""
    with mpmath.workdps(80):
        x, y = mpmath.mpf(x), mpmath.mpf(y)
        if x <= (a + 1) / (a + b + 2):
            return +mpmath.betainc(a, b, 0, x, regularized=True)
        return 1 - mpmath.betainc(b, a, 0, y, regularized=True)


def regularized_beta_upper(a, b, x, y):
    """1 - I_x(a, b) as I_y(b, a)."""
    with mpmath.workdps(80):
        x, y = mpmath.mpf(x), mpmath.mpf(y)
        if y <= (b + 1) / (a + b + 2):
            return +mpmath.betainc(b, a, 0, y, regularized=True)
        return 1 - mpmath.betainc(a, b, 0, x, regularized=True)


def lower_gamma(a, x):
    with mpmath.workdps(60):
        return +mpmath.gammainc(a, 0, x, regularized=True)


def upper_gamma(a, x):
    with mpmath.workdps(60):
        return +mpmath.gammainc(a, x, mpmath.inf, regularized=True)


def bdtr(k, n, p):
    k = math.floor(k)
    return regularized_beta(n - k, k + 1, 1 - mpmath.mpf(p), p)


def bdtrc(k, n, p):
    k = math.floor(k)
    return regularized_beta(k + 1, n - k, p, 1 - mpmath.mpf(p))


def nbdtr(k, n, p):
    return regularized_beta(n, k + 1, p, 1 - mpmath.mpf(p))


def nbdtrc(k, n, p):
    return regularized_beta_upper(n, k + 1, p, 1 - mpmath.mpf(p))


def fdtr(a, b, x):
    w = mpmath.mpf(a) * x / (b + mpmath.mpf(a) * x)
    return regularized_beta(a / 2, b / 2, w, 1 - w)


def fdtrc(a, b, x):
    w = mpmath.mpf(a) * x / (b + mpmath.mpf(a) * x)
    return regularized_beta_upper(a / 2, b / 2, w, 1 - w)


def stdtr(df, t):
    with mpmath.workdps(80):
        t = mpmath.mpf(t)
        tail = regularized_beta(df / 2, 0.5, df / (df + t * t), t * t / (df + t * t)) / 2
        return tail if t < 0 else 1 - tail


def root(function, target, low, high):
    """The x in (low, high) where the increasing `function` reaches `target`, bisected in the
    logarithm of x to 1e-40."""
    low, high = max(mpmath.mpf(low), mpmath.mpf("1e-320")), mpmath.mpf(high)
    for _ in range(600):
        middle = mpmath.sqrt(low * high)
        if function(middle) < target:
            low = middle
        else:
            high = middle
        if high - low < mpmath.mpf(10) ** -40 * abs(high):
            break
    return (low + high) / 2


# Whole-number counts, probabilities and positive shapes, with values far into both tails.
for _ in range(80):
    n = int(generator.integers(1, 2000))
    k = float(generator.integers(0, n))
    p = float(generator.uniform(0, 1)) ** 2
    lines.append(f"bdtr {k!r} {float(n)!r} {p!r} {float(bdtr(k, n, p))!r}")
    lines.append(f"bdtrc {k!r} {float(n)!r} {p!r} {float(bdtrc(k, n, p))!r}")
    y = float(bdtr(k, n, p))
    if 1e-300 < y < 1:
        x = root(lambda q: -bdtr(k, n, q), -y, 0, 1)
        lines.append(f"bdtri {k!r} {float(n)!r} {y!r} {float(x)!r}")
for _ in range(60):
    n = float(generator.integers(1, 500))
    k = float(generator.integers(0, 1000))
    p = float(generator.uniform(0, 1))
    lines.append(f"nbdtr {k!r} {n!r} {p!r} {float(nbdtr(k, n, p))!r}")
    lines.append(f"nbdtrc {k!r} {n!r} {p!r} {float(nbdtrc(k, n, p))!r}")
    y = float(nbdtr(k, n, p))
    if 1e-300 < y < 1:
        x = root(lambda q: nbdtr(k, n, q), y, 0, 1)
        lines.append(f"nbdtri {k!r} {n!r} {y!r} {float(x)!r}")
for _ in range(80):
    k = float(generator.integers(0, 1000))
    m = float((k + 1) * math.exp(generator.uniform(-1.5, 1.5)))
    lines.append(f"pdtr {k!r} {m!r} {float(upper_gamma(k + 1, m))!r}")
    lines.append(f"pdtrc {k!r} {m!r} {float(lower_gamma(k + 1, m))!r}")
    y = float(upper_gamma(k + 1, m))
    if 1e-300 < y < 1:
        x = root(lambda t: -upper_gamma(k + 1, t), -y, 0, 10 * (k + 50))
        lines.append(f"pdtri {k!r} {y!r} {float(x)!r}")
for v in spread(0.05, 2000, 80):
    x = float(v * math.exp(generator.uniform(-2, 1.5)))
    lines.append(f"chdtr {v!r} {x!r} {float(lower_gamma(v / 2, x / 2))!r}")
    lines.append(f"chdtrc {v!r} {x!r} {float(upper_gamma(v / 2, x / 2))!r}")
    q = float(upper_gamma(v / 2, x / 2))
    if 1e-300 < q < 1:
        x = root(lambda t: -upper_gamma(v / 2, t / 2), -q, 0, 100 * (v + 50))
        lines.append(f"chdtri {v!r} {q!r} {float(x)!r}")
for a, b in zip(spread(0.1, 1000, 80), spread(0.1, 1000, 80)):
    x = float(math.exp(generator.uniform(-4, 4)))
    lines.append(f"fdtr {a!r} {b!r} {x!r} {float(fdtr(a, b, x))!r}")
    lines.append(f"fdtrc {a!r} {b!r} {x!r} {float(fdtrc(a, b, x))!r}")
    y = float(fdtr(a, b, x))
    if 1e-300 < y < 1:
        r = root(lambda t: fdtr(a, b, t), y, 0, 1e300)
        lines.append(f"fdtri {a!r} {b!r} {y!r} {float(r)!r}")
for a, b in zip(spread(0.01, 100, 60), spread(0.1, 1000, 60)):
    x = float(b / a * math.exp(generator.uniform(-2, 1.5)))
    lines.append(f"gdtr {a!r} {b!r} {x!r} {float(lower_gamma(b, a * mpmath.mpf(x)))!r}")
    lines.append(f"gdtrc {a!r} {b!r} {x!r} {float(upper_gamma(b, a * mpmath.mpf(x)))!r}")
for df in spread(0.1, 1e4, 100):
    t = float(generator.uniform(-1, 1) * math.exp(generator.uniform(-3, 4)))
    lines.append(f"stdtr {df!r} {t!r} {float(stdtr(df, t))!r}")
    p = float(stdtr(df, t))
    if 1e-300 < p < 1:
        with mpmath.workdps(60):
            r = mpmath.findroot(lambda u: stdtr(df, u) - p, t, tol=mpmath.mpf(10) ** -45)
        lines.append(f"stdtrit {df!r} {p!r} {float(r)!r}")
edges = [
    ("bdtr", (-1.0, 10.0, 0.5)), ("bdtr", (10.0, 10.0, 0.5)), ("bdtr", (11.0, 10.0, 0.5)),
    ("bdtr", (0.0, 10.0, 0.5)), ("bdtr", (3.0, 10.0, 0.0)), ("bdtr", (3.0, 10.0, 1.0)),
    ("bdtr", (3.0, 10.0, 1.5)), ("bdtr", (3.7, 10.0, 0.3)), ("bdtrc", (-1.0, 10.0, 0.5)),
    ("bdtrc", (10.0, 10.0, 0.5)), ("bdtrc", (3.0, 10.0, 0.0)), ("bdtrc", (3.0, 10.0, 1.0)),
    ("bdtri", (3.0, 10.0, 0.0)), ("bdtri", (3.0, 10.0, 1.0)), ("bdtri", (10.0, 10.0, 0.5)),
    ("bdtri", (-1.0, 10.0, 0.5)), ("nbdtr", (-1.0, 10.0, 0.5)), ("nbdtr", (3.0, 10.0, 0.0)),
    ("nbdtr", (3.0, 10.0, 1.0)), ("nbdtr", (3.0, 0.0, 0.5)), ("nbdtrc", (3.0, 10.0, 0.0)),
    ("nbdtrc", (3.0, 10.0, 1.0)), ("nbdtri", (3.0, 10.0, 0.0)), ("nbdtri", (3.0, 10.0, 1.0)),
    ("pdtr", (-1.0, 3.0)), ("pdtr", (3.0, 0.0)), ("pdtr", (3.0, inf)), ("pdtr", (2.7, 3.0)),
    ("pdtr", (3.0, -1.0)), ("pdtrc", (3.0, 0.0)), ("pdtrc", (3.0, inf)), ("pdtri", (3.0, 0.0)),
    ("pdtri", (3.0, 1.5)), ("chdtr", (2.0, 0.0)), ("chdtr", (2.0, -1.0)), ("chdtr", (2.0, inf)),
    ("chdtr", (0.0, 1.0)), ("chdtrc", (2.0, 0.0)), ("chdtrc", (2.0, inf)), ("chdtri", (2.0, 0.0)),
    ("chdtri", (2.0, 1.0)), ("chdtri", (0.0, 0.5)), ("fdtr", (2.0, 3.0, 0.0)),
    ("fdtr", (2.0, 3.0, -1.0)), ("fdtr", (2.0, 3.0, inf)), ("fdtr", (0.0, 3.0, 1.0)),
    ("fdtr", (inf, 3.0, 1.0)), ("fdtrc", (2.0, 3.0, 0.0)), ("fdtrc", (2.0, 3.0, inf)),
    ("fdtri", (2.0, 3.0, 0.0)), ("fdtri", (2.0, 3.0, 1.0)), ("gdtr", (1.0, 2.0, 0.0)),
    ("gdtr", (1.0, 2.0, -1.0)), ("gdtr", (1.0, 2.0, inf)), ("gdtr", (0.0, 2.0, 1.0)),
    ("gdtr", (1.0, 0.0, 1.0)), ("gdtrc", (1.0, 2.0, 0.0)), ("gdtrc", (1.0, 2.0, inf)),
    ("stdtr", (3.0, 0.0)), ("stdtr", (3.0, inf)), ("stdtr", (3.0, -inf)), ("stdtr", (inf, 1.0)),
    ("stdtr", (0.0, 1.0)), ("stdtrit", (3.0, 1.0)), ("stdtrit", (3.0, 0.5)),
    ("stdtrit", (inf, 0.3)), ("stdtrit", (0.0, 0.5)), ("bdtr", (1e300, 10.0, 0.5)),
    ("pdtr", (1e300, 3.0)), ("pdtrc", (1e300, 3.0)), ("smirnov", (1e11, 1e-6)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
# Where SciPy 1.18 gives NaN or the wrong infinity, the file holds the limit: pdtr(k, m) is 1
# only at m = 0, and the t with probability 0 is minus infinity.
lines.append("pdtri 3.0 1.0 0.0")
lines.append("stdtrit 3.0 0.0 -inf")
# The Kolmogorov distribution and Owen's T function, drawn after the other distribution
# functions.


def kolmogorov_parts(y):
    """K(y) and 1 - K(y) at 50 digits, each from the series that converges fast for `y`."""
    y = mpmath.mpf(y)
    if y < 1:
        lower = mpmath.sqrt(2 * mpmath.pi) / y * mpmath.nsum(
            lambda k: mpmath.exp(-(2 * k - 1) ** 2 * mpmath.pi ** 2 / (8 * y * y)), [1, mpmath.inf])
        return lower, 1 - lower
    upper = 2 * mpmath.nsum(lambda k: (-1) ** (k - 1) * mpmath.exp(-2 * k * k * y * y),
                            [1, mpmath.inf])
    return 1 - upper, upper


def owens_t(h, a):
    """Owen's T by quadrature at 40 digits, over the range where the integrand is not below
    exp(-800) of its peak."""
    with mpmath.workdps(40):
        h, a = mpmath.mpf(h), mpmath.mpf(a)
        length = min(a, 40 / abs(h)) if h != 0 else a
        integrand = lambda x: mpmath.exp(-h * h * (1 + x * x) / 2) / (1 + x * x)
        points = [0] + [length / 2 ** j for j in range(6, -1, -1)]
        return mpmath.quad(integrand, points) / (2 * mpmath.pi)


for y in uniform(0.05, 1, 60) + uniform(1, 3, 40) + uniform(3, 27, 40):
    lines.append(f"kolmogorov {y!r} {float(kolmogorov_parts(y)[1])!r}")
for p in spread(1e-300, 0.5, 50) + uniform(0.5, 1, 40):
    q = 1 - mpmath.mpf(p)
    low, high = mpmath.mpf("0.01"), mpmath.mpf(30)
    for _ in range(200):
        middle = (low + high) / 2
        lower, upper = kolmogorov_parts(middle)
        if upper > p:
            low = middle
        else:
            high = middle
    lines.append(f"kolmogi {p!r} {float((low + high) / 2)!r}")
for h, a in zip(uniform(-10, 10, 150), spread(1e-3, 100, 150)):
    lines.append(f"owens_t {h!r} {a!r} {float(owens_t(h, a))!r}")
edges = [
    ("kolmogorov", (-1.0,)), ("kolmogorov", (0.0,)), ("kolmogorov", (inf,)),
    ("kolmogi", (0.0,)), ("kolmogi", (1.0,)), ("kolmogi", (1.5,)),
    ("owens_t", (0.0, 1.0)), ("owens_t", (1.0, 0.0)), ("owens_t", (1.0, inf)),
    ("owens_t", (inf, 1.0)), ("owens_t", (0.0, inf)), ("owens_t", (1.0, -1.0)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))


def smirnov(n, d):
    """Birnbaum and Tingey's exact sum at 40 digits; every term is positive."""
    with mpmath.workdps(40):
        d = mpmath.mpf(d)
        total = mpmath.mpf(0)
        for j in range(int(mpmath.floor(n * (1 - d))) + 1):
            total += mpmath.binomial(n, j) * (1 - d - mpmath.mpf(j) / n) ** (n - j) * (
                d + mpmath.mpf(j) / n) ** (j - 1)
        return d * total


for n in [int(x) for x in spread(1, 5000, 80)]:
    d = float(generator.uniform(0, 1) ** 2 * min(1, 3 / math.sqrt(n)) + 1e-3)
    lines.append(f"smirnov {float(n)!r} {d!r} {float(smirnov(n, d))!r}")
for n in [int(x) for x in spread(1, 300, 40)]:
    d = float(generator.uniform(0, 1) ** 2 * min(1, 3 / math.sqrt(n)) + 1e-3)
    p = float(smirnov(n, d))
    if 1e-300 < p < 1:
        low, high = mpmath.mpf(0), mpmath.mpf(1)
        for _ in range(140):
            middle = (low + high) / 2
            if smirnov(n, middle) > p:
                low = middle
            else:
                high = middle
        lines.append(f"smirnovi {float(n)!r} {p!r} {float((low + high) / 2)!r}")
edges = [
    ("smirnov", (1.0, 0.5)), ("smirnov", (10.0, 0.0)), ("smirnov", (10.0, 1.0)),
    ("smirnov", (10.0, -0.1)), ("smirnov", (0.0, 0.5)), ("smirnov", (5.0, 0.9)),
    ("smirnovi", (10.0, 0.0)), ("smirnovi", (10.0, 1.0)), ("smirnovi", (10.0, 1.5)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
with open("distribution_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
# The everyday helpers (roadmap 3.18.11), drawn after the distribution functions.
lines = []


def ref_binom(n, k):
    with mpmath.workdps(60):
        return mpmath.binomial(n, k)


def ref_kl(x, y):
    with mpmath.workdps(60):
        x, y = mpmath.mpf(x), mpmath.mpf(y)
        return x * mpmath.log(x / y) - x + y


helpers = {
    "expit": (lambda x: 1 / (1 + mpmath.exp(-mpmath.mpf(x))),
              uniform(-40, 40, 80) + [-x for x in spread(40, 700, 20)]),
    "log_expit": (lambda x: -mpmath.log1p(mpmath.exp(-mpmath.mpf(x))),
                  uniform(-40, 40, 80) + spread(40, 700, 10) + [-x for x in spread(40, 1e300, 10)]),
    "logit": (lambda p: mpmath.log(mpmath.mpf(p) / (1 - mpmath.mpf(p))),
              uniform(0, 1, 80) + uniform(0.45, 0.55, 30) + spread(1e-300, 1e-3, 20)),
    "entr": (lambda x: -mpmath.mpf(x) * mpmath.log(x),
             uniform(0, 3, 40) + spread(1e-300, 1e300, 40)),
}
for name, (function, arguments) in helpers.items():
    for x in arguments:
        lines.append(f"{name} {x!r} {float(function(x))!r}")
for x, y in zip(spread(1e-10, 1e10, 60), spread(1e-10, 1e10, 60)):
    with mpmath.workdps(60):
        lines.append(f"xlogy {x!r} {y!r} {float(x * mpmath.log(y))!r}")
        lines.append(f"rel_entr {x!r} {y!r} {float(x * mpmath.log(mpmath.mpf(x) / y))!r}")
        lines.append(f"kl_div {x!r} {y!r} {float(ref_kl(x, y))!r}")
for x, y in zip(spread(1e-10, 1e10, 30), uniform(-0.99, 3, 30) + []):
    with mpmath.workdps(60):
        lines.append(f"xlog1py {x!r} {y!r} {float(x * mpmath.log1p(y))!r}")
for y in uniform(0.5, 2, 40):
    x = float(y * (1 + generator.uniform(-1e-3, 1e-3)))
    lines.append(f"kl_div {x!r} {y!r} {float(ref_kl(x, y))!r}")
for _ in range(60):
    n = float(generator.integers(0, 1000))
    k = float(generator.integers(0, n + 1))
    lines.append(f"binom {n!r} {k!r} {float(ref_binom(n, k))!r}")
    lines.append(f"comb {n!r} {k!r} {float(ref_binom(n, k))!r}")
    with mpmath.workdps(60):
        lines.append(f"perm {n!r} {k!r} {float(mpmath.rf(n - k + 1, k))!r}")
for n, k in zip(uniform(-30, 30, 60), uniform(-10, 30, 60)):
    if not (n < 0 and n == math.floor(n)):
        lines.append(f"binom {n!r} {k!r} {float(ref_binom(n, k))!r}")
for n in uniform(0, 170, 40):
    lines.append(f"factorial {n!r} {float(mpmath.gamma(n + 1))!r}")
edges = [
    ("expit", (0.0,)), ("expit", (inf,)), ("expit", (-inf,)), ("log_expit", (inf,)),
    ("log_expit", (-inf,)), ("logit", (0.0,)), ("logit", (1.0,)), ("logit", (0.5,)),
    ("logit", (-0.1,)), ("xlogy", (0.0, 0.0)), ("xlogy", (0.0, -1.0)), ("xlogy", (1.0, 0.0)),
    ("xlog1py", (0.0, -1.0)), ("xlog1py", (1.0, -1.0)), ("entr", (0.0,)), ("entr", (-1.0,)),
    ("entr", (inf,)), ("rel_entr", (0.0, 0.0)), ("rel_entr", (1.0, 0.0)), ("rel_entr", (-1.0, 1.0)),
    ("rel_entr", (1.0, inf)), ("kl_div", (0.0, 1.0)), ("kl_div", (1.0, 0.0)),
    ("kl_div", (-1.0, 1.0)), ("kl_div", (inf, 1.0)), ("binom", (5.0, -1.0)), ("binom", (5.0, 6.0)),
    ("binom", (-3.0, 2.0)), ("binom", (0.0, 0.0)), ("binom", (1e300, 2.0)), ("comb", (5.0, -1.0)),
    ("comb", (5.0, 6.0)), ("comb", (-3.0, 2.0)), ("perm", (5.0, 6.0)), ("perm", (0.0, 0.0)),
    ("factorial", (-1.0,)), ("factorial", (0.0,)), ("factorial", (171.0,)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
with open("helper_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")

# Exponential, sine, cosine and Fresnel integrals, zeta and Spence's function (roadmap
# 3.18.11), drawn after the everyday helpers. A function that returns a pair is recorded as
# two names: `si` and `ci` for `sici`, `shi` and `chi` for `shichi`, and `fresnel_s` and
# `fresnel_c` for `fresnel`.
lines = []


def hurwitz(s, q, count=60, terms=25):
    """The Hurwitz zeta function by the Euler-Maclaurin formula at 50 digits: mpmath's own
    `zeta(s, q)` loses digits when `s` and `q` are both large."""
    with mpmath.workdps(50):
        s, q = mpmath.mpf(s), mpmath.mpf(q)
        total = sum((q + k) ** -s for k in range(count))
        a = q + count
        total += a ** (1 - s) / (s - 1) + a ** -s / 2
        for j in range(1, terms):
            total += (mpmath.bernoulli(2 * j) / mpmath.factorial(2 * j) * mpmath.rf(s, 2 * j - 1)
                      * a ** (-s - 2 * j + 1))
        return total


integrals = {
    "exp1": (mpmath.e1, spread(1e-300, 1, 40) + uniform(0, 3, 40) + spread(3, 700, 40)),
    "expi": (mpmath.ei, uniform(-40, 40, 100) + spread(1e-300, 1e-3, 20) + spread(40, 700, 20)
             + [-x for x in spread(40, 700, 20)]),
    "si": (mpmath.si, uniform(-10, 10, 60) + spread(10, 1e10, 30)),
    "ci": (mpmath.ci, uniform(0, 10, 60) + spread(10, 1e10, 30) + spread(1e-300, 1e-3, 10)),
    "shi": (mpmath.shi, uniform(-10, 10, 60) + spread(10, 700, 20)),
    "chi": (mpmath.chi, uniform(0, 10, 60) + spread(10, 700, 20) + spread(1e-300, 1e-3, 10)),
    "fresnel_s": (mpmath.fresnels, uniform(-5, 5, 60) + spread(5, 1e8, 30)),
    "fresnel_c": (mpmath.fresnelc, uniform(-5, 5, 60) + spread(5, 1e8, 30)),
    "zeta": (mpmath.zeta, uniform(-30, 30, 100) + uniform(0.9, 1.1, 20) + spread(30, 1000, 10)
             + uniform(-250, -30, 20)),
    "zetac": (lambda x: mpmath.zeta(x) - 1 if x < 2 else hurwitz(x, 2),
              uniform(-10, 10, 40) + spread(10, 1000, 30)),
    "spence": (lambda x: mpmath.polylog(2, 1 - mpmath.mpf(x)), uniform(0, 3, 80)
               + uniform(0.9, 1.1, 20) + spread(3, 1e10, 20) + spread(1e-300, 1e-3, 10)),
}
for name, (function, arguments) in integrals.items():
    for x in arguments:
        with mpmath.workdps(60):
            lines.append(f"{name} {x!r} {float(function(x))!r}")
for _ in range(100):
    n = int(generator.integers(0, 60))
    x = float(math.exp(generator.uniform(-6, 5)))
    with mpmath.workdps(60):
        lines.append(f"expn {n} {x!r} {float(mpmath.expint(n, x))!r}")
for x, q in zip(spread(1.01, 60, 60), spread(1e-3, 1e3, 60)):
    with mpmath.workdps(60):
        lines.append(f"hurwitz {x!r} {q!r} {float(hurwitz(x, q))!r}")
edges = [
    ("exp1", (0.0,)), ("exp1", (-1.0,)), ("exp1", (inf,)), ("expi", (0.0,)), ("expi", (inf,)),
    ("expi", (-inf,)), ("zeta", (1.0,)), ("zeta", (0.0,)), ("zeta", (-2.0,)), ("zeta", (inf,)),
    ("zetac", (1.0,)), ("zetac", (inf,)), ("zetac", (0.0,)), ("spence", (0.0,)),
    ("spence", (1.0,)), ("spence", (-1.0,)), ("spence", (inf,)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
for n, x in [(0, 0.0), (1, 0.0), (2, 0.0), (-1, 1.0), (2, inf)]:
    lines.append(f"expn {n} {x!r} {float(scipy.special.expn(n, x))!r}")
for x, q in [(2.0, 0.0), (2.0, -1.0), (0.5, 1.0), (inf, 2.0), (inf, 1.0)]:
    lines.append(f"hurwitz {x!r} {q!r} {float(scipy.special.zeta(x, q))!r}")
for x in [0.0, inf, -inf]:
    si, ci = scipy.special.sici(x)
    shi, chi = scipy.special.shichi(x)
    s, c = scipy.special.fresnel(x)
    lines += [f"si {x!r} {float(si)!r}", f"ci {x!r} {float(ci)!r}", f"shi {x!r} {float(shi)!r}",
              f"chi {x!r} {float(chi)!r}", f"fresnel_s {x!r} {float(s)!r}",
              f"fresnel_c {x!r} {float(c)!r}"]
with open("integral_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## The Lambert W function

`lambertw.txt` has its own format, since W is complex: each value is two lines, `lambertw_re`
and `lambertw_im`, each holding the branch `k`, the real and imaginary parts of `z`, the size
|W_k(z)| the part's error is measured against, and the part's value. The script below recorded
it with its own seed, with mpmath 1.4.1 at 50 digits and SciPy 1.18.1 at the ends:

```python
import math

import mpmath
import numpy
import scipy.special

generator = numpy.random.default_rng(3181101)
inf, nan = math.inf, math.nan
lines = []


def case(k, z, w):
    """Two lines for W_k(z) = w, its real and its imaginary part, each with |w| as the size
    its error is measured against, or the part's own size where |w| is infinite."""
    for part, value in (("re", w.real), ("im", w.imag)):
        size = abs(w) if math.isfinite(abs(w)) else abs(value)
        lines.append(f"lambertw_{part} {k} {z.real!r} {z.imag!r} {size!r} {value!r}")


def record(k, z):
    with mpmath.workdps(50):
        w = complex(mpmath.lambertw(mpmath.mpc(z), k))
    case(k, z, w)


# Real arguments on the two real branches: branch 0 from just above -1/e to 1e300, branch -1
# from just above -1/e to just below 0.
edge = -math.exp(-1)
for x in [edge + float(10 ** generator.uniform(-12, -0.5)) for _ in range(30)]:
    record(0, complex(x, 0.0))
    if x < 0:
        record(-1, complex(x, 0.0))
for x in generator.uniform(-0.36, 3, 40):
    record(0, complex(float(x), 0.0))
for x in 10 ** generator.uniform(-300, 300, 40):
    record(0, complex(float(x), 0.0))
for x in -(10 ** generator.uniform(-300, math.log10(-edge) - 1e-3, 30)):
    record(-1, complex(float(x), 0.0))
# Complex arguments, and real ones off the real branches, on branches -3 to 3 and 40.
for k in (0, -1, 1, 2, -2, 3, -3, 40):
    for _ in range(30):
        size = float(10 ** generator.uniform(-10, 10))
        angle = float(generator.uniform(-math.pi, math.pi))
        record(k, complex(size * math.cos(angle), size * math.sin(angle)))
    for x in generator.uniform(-5, 5, 10):
        record(k, complex(float(x), 0.0))
# Near the branch point, from above and below the cut.
for _ in range(20):
    record(0, complex(edge + float(generator.uniform(-0.2, 0.2)),
                      float(generator.uniform(-0.2, 0.2))))
# The branch point itself, rounded to f64, and its neighbours.
for x in [edge, math.nextafter(edge, 0), math.nextafter(edge, -1)]:
    record(0, complex(x, 0.0))
    record(-1, complex(x, 0.0))
# SciPy's values at 0, 1, the branch point and infinity, and NaN.
for k, z in [(0, 0j), (1, 0j), (-1, 0j), (0, 1 + 0j), (0, complex(inf, 0.0)),
             (2, complex(inf, 1.0)), (0, complex(-inf, 0.0)), (-1, complex(-inf, 2.0)),
             (0, complex(nan, 0.0))]:
    case(k, z, complex(scipy.special.lambertw(z, k)))
with open("lambertw.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Elliptic integrals and functions

`elliptic_functions.txt` has the usual format; a case of `ellipj` names one of its four values,
`ellipj_sn`, `ellipj_cn`, `ellipj_dn` or `ellipj_ph`. The script below recorded it with its own
seed, with mpmath 1.4.1 at 50 digits and SciPy 1.18.1 at the ends. The amplitude is mpmath's
sn and cn at u − 2nK, brought back by nπ, since mpmath has no amplitude function, and
`ellipkm1` is π/(2 AGM(1, √p)), which keeps every digit of `p`. For `y` or `p` below 0,
`elliprc` and `elliprj` are the real parts of mpmath's values, which are Cauchy's principal
values:

```python
import math

import mpmath
import numpy
import scipy.special

generator = numpy.random.default_rng(31809)
inf, nan = math.inf, math.nan
lines = []


def uniform(low, high, count):
    return [float(x) for x in generator.uniform(low, high, count)]


def spread(low, high, count):
    return [float(math.exp(x)) for x in generator.uniform(math.log(low), math.log(high), count)]


def add(name, arguments, value):
    lines.append(" ".join([name] + [repr(float(x)) for x in arguments] + [repr(float(value))]))


def amplitude(u, m):
    """am(u | m), continuous in u: am(u + 2K) = am(u) + π."""
    k = mpmath.ellipk(m)
    n = mpmath.nint(u / (2 * k))
    rest = u - 2 * n * k
    sn, cn = mpmath.ellipfun("sn", rest, m=m), mpmath.ellipfun("cn", rest, m=m)
    return n * mpmath.pi + mpmath.atan2(sn, cn)


with mpmath.workdps(50):
    near_one = [1 - x for x in spread(1e-16, 0.1, 30)]
    for m in uniform(-10, 1, 60) + near_one + [-x for x in spread(10, 1e300, 20)]:
        add("ellipk", [m], mpmath.ellipk(m))
        add("ellipe", [m], mpmath.ellipe(m))
    for p in spread(1e-300, 1e3, 60):
        add("ellipkm1", [p], mpmath.pi / (2 * mpmath.agm(1, mpmath.sqrt(p))))
    for _ in range(150):
        phi = float(generator.uniform(-12, 12))
        m = float(generator.choice([generator.uniform(-5, 1),
                                    1 - math.exp(generator.uniform(-36, -1))]))
        add("ellipkinc", [phi, m], mpmath.ellipf(phi, m))
        add("ellipeinc", [phi, m], mpmath.ellipe(phi, m))
    for phi in [math.pi / 2 - x for x in spread(1e-12, 0.1, 20)]:
        m = 1 - float(math.exp(generator.uniform(-36, -1)))
        add("ellipkinc", [phi, m], mpmath.ellipf(phi, m))
        add("ellipeinc", [phi, m], mpmath.ellipe(phi, m))
    for phi in uniform(-1.5, 1.5, 10):
        add("ellipkinc", [phi, 1.0], mpmath.ellipf(phi, 1))
        add("ellipeinc", [phi, 1.0], mpmath.ellipe(phi, 1))
    for _ in range(150):
        u = float(generator.uniform(-20, 20))
        m = float(generator.choice([generator.uniform(0, 1),
                                    1 - math.exp(generator.uniform(-36, -1)),
                                    math.exp(generator.uniform(-36, -1))]))
        sn = mpmath.ellipfun("sn", u, m=m)
        cn = mpmath.ellipfun("cn", u, m=m)
        dn = mpmath.ellipfun("dn", u, m=m)
        add("ellipj_sn", [u, m], sn)
        add("ellipj_cn", [u, m], cn)
        add("ellipj_dn", [u, m], dn)
        add("ellipj_ph", [u, m], amplitude(u, m))
    def positive(count):
        return [float(x) for x in 10 ** generator.uniform(-20, 20, count)]
    for x, y in zip(positive(60), positive(60)):
        add("elliprc", [x, y], mpmath.elliprc(x, y))
        add("elliprc", [x, -y], mpmath.re(mpmath.elliprc(x, -y)))
    for x, y in zip(uniform(0.5, 2, 20), uniform(0.5, 2, 20)):
        add("elliprc", [x, x * (1 + 1e-6 * y)], mpmath.elliprc(x, x * (1 + 1e-6 * y)))
    for x, y, z in zip(positive(80), positive(80), positive(80)):
        add("elliprf", [x, y, z], mpmath.elliprf(x, y, z))
        add("elliprd", [x, y, z], mpmath.elliprd(x, y, z))
        add("elliprg", [x, y, z], mpmath.elliprg(x, y, z))
    for y, z in zip(positive(20), positive(20)):
        add("elliprf", [0.0, y, z], mpmath.elliprf(0, y, z))
        add("elliprd", [0.0, y, z], mpmath.elliprd(0, y, z))
        add("elliprg", [0.0, y, z], mpmath.elliprg(0, y, z))
    small = [float(x) for x in 10 ** generator.uniform(-5, 5, 240)]
    for x, y, z, p in zip(small[0::4], small[1::4], small[2::4], small[3::4]):
        add("elliprj", [x, y, z, p], mpmath.elliprj(x, y, z, p))
        add("elliprj", [x, y, z, -p], mpmath.re(mpmath.elliprj(x, y, z, -p)))
edges = [
    ("ellipk", (1.0,)), ("ellipk", (1.5,)), ("ellipk", (-inf,)), ("ellipk", (0.0,)),
    ("ellipkm1", (0.0,)), ("ellipkm1", (-1.0,)), ("ellipkm1", (inf,)),
    ("ellipe", (1.0,)), ("ellipe", (1.5,)), ("ellipe", (-inf,)), ("ellipe", (0.0,)),
    ("ellipkinc", (0.0, 0.5)), ("ellipkinc", (inf, 0.5)), ("ellipkinc", (-inf, 0.5)),
    ("ellipkinc", (0.5, 2.0)), ("ellipkinc", (0.5, -inf)), ("ellipkinc", (math.pi / 2, 1.0)),
    ("ellipkinc", (10.0, 1.0)), ("ellipkinc", (0.7, 0.0)),
    ("ellipeinc", (0.0, 0.5)), ("ellipeinc", (inf, 0.5)), ("ellipeinc", (0.5, 2.0)),
    ("ellipeinc", (0.5, -inf)), ("ellipeinc", (0.7, 0.0)),
    ("elliprc", (1.0, 0.0)), ("elliprc", (-1.0, 1.0)), ("elliprc", (inf, 1.0)),
    ("elliprc", (0.0, -1.0)), ("elliprf", (0.0, 0.0, 1.0)), ("elliprf", (1.0, 2.0, inf)),
    ("elliprf", (-1.0, 2.0, 3.0)), ("elliprd", (0.0, 0.0, 1.0)), ("elliprd", (1.0, 2.0, 0.0)),
    ("elliprd", (1.0, 2.0, inf)), ("elliprg", (0.0, 0.0, 0.0)), ("elliprg", (0.0, 0.0, 4.0)),
    ("elliprg", (1.0, inf, 2.0)), ("elliprj", (0.0, 0.0, 1.0, 1.0)),
    ("elliprj", (1.0, 2.0, 3.0, 0.0)),
    ("elliprj", (1.0, 2.0, inf, 1.0)),
]
for name, arguments in edges:
    add(name, arguments, getattr(scipy.special, name)(*arguments))
for u, m in [(1.0, 0.0), (1.0, 1.0), (1.0, 1.5), (1.0, -0.5), (inf, 0.5)]:
    sn, cn, dn, ph = scipy.special.ellipj(u, m)
    for part, value in zip(("sn", "cn", "dn", "ph"), (sn, cn, dn, ph)):
        add(f"ellipj_{part}", [u, m], value)
with open("elliptic_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Orthogonal polynomials and spherical harmonics

In `orthogonal_polynomials.txt` each case holds, before the reference value, the size its error
is measured against: the largest value the polynomial's recurrence passes through on its way to
the degree, computed at 60 digits. A case of `sph_harm_y` is one part, `sph_harm_y_re` or
`sph_harm_y_im`. The script below recorded it with its own seed, with mpmath 1.4.1 and SciPy
1.18.1 at the ends; mpmath's `legenp` of type 2 and `spherharm` carry the Condon–Shortley phase,
as SciPy's `lpmv` and `sph_harm_y` do:

```python
import math

import mpmath
import numpy
import scipy.special

generator = numpy.random.default_rng(318010)
inf, nan = math.inf, math.nan
lines = []


def add(name, arguments, size, value):
    lines.append(" ".join([name] + [repr(x) if isinstance(x, float) else str(x) for x in arguments]
                          + [repr(float(size)), repr(float(value))]))


def largest(step, n, first, second):
    """The largest size the recurrence passes through on its way to degree n: its error is a
    few ULP of that, not of a value near one of the polynomial's zeros."""
    values = [first, second]
    for k in range(1, n):
        values.append(step(k, values[-1], values[-2]))
    return max(abs(v) for v in values[: n + 1])


def degree(high):
    """A degree below `high` half the time, below 12 otherwise."""
    if generator.uniform() < 0.5:
        return int(generator.integers(0, high))
    return int(generator.integers(0, 12))


with mpmath.workdps(60):
    for _ in range(120):
        n = degree(200)
        inside = generator.uniform() < 0.8
        x = float(generator.uniform(-1, 1)) if inside else float(generator.uniform(-3, 3))
        X = mpmath.mpf(x)
        size = largest(lambda k, a, b: ((2 * k + 1) * X * a - k * b) / (k + 1), n, 1, X)
        add("eval_legendre", [n, x], size, mpmath.legendre(n, x))
        size = largest(lambda k, a, b: 2 * X * a - b, n, 1, X)
        add("eval_chebyt", [n, x], size, mpmath.chebyt(n, x))
        size = largest(lambda k, a, b: 2 * X * a - b, n, 1, 2 * X)
        add("eval_chebyu", [n, x], size, mpmath.chebyu(n, x))
    for _ in range(120):
        n = degree(120)
        x = float(generator.uniform(-1, 1) * math.sqrt(2 * n + 1) * 1.2)
        X = mpmath.mpf(x)
        add("eval_hermite", [n, x], largest(lambda k, a, b: 2 * X * a - 2 * k * b, n, 1, 2 * X),
            mpmath.hermite(n, x))
        add("eval_hermitenorm", [n, x], largest(lambda k, a, b: X * a - k * b, n, 1, X),
            mpmath.hermite(n, X / mpmath.sqrt(2)) / mpmath.mpf(2) ** (mpmath.mpf(n) / 2))
    for _ in range(120):
        n = degree(120)
        alpha = float(generator.choice([0.0, generator.uniform(-0.9, 5)]))
        x = float(generator.uniform(0, 4 * n + 10))
        A, X = mpmath.mpf(alpha), mpmath.mpf(x)
        size = largest(lambda k, a, b: ((2 * k + 1 + A - X) * a - (k + A) * b) / (k + 1),
                       n, 1, 1 + A - X)
        add("eval_genlaguerre", [n, alpha, x], size, mpmath.laguerre(n, alpha, x))
        if alpha == 0.0:
            add("eval_laguerre", [n, x], size, mpmath.laguerre(n, 0, x))
    for _ in range(120):
        n = degree(100)
        alpha, beta = (float(v) for v in generator.uniform(-0.9, 4, 2))
        x = float(generator.uniform(-1, 1))
        A, B, X = mpmath.mpf(alpha), mpmath.mpf(beta), mpmath.mpf(x)

        def jacobi_step(k, a, b):
            s = 2 * k + A + B
            return (((s + 1) * ((s + 2) * s * X + A * A - B * B)) * a
                    - 2 * (k + A) * (k + B) * (s + 2) * b) / (2 * (k + 1) * (k + A + B + 1) * s)
        size = largest(jacobi_step, n, 1, (A + 1) + (A + B + 2) * (X - 1) / 2)
        add("eval_jacobi", [n, alpha, beta, x], size, mpmath.jacobi(n, alpha, beta, x))
        gamma_alpha = float(generator.uniform(-0.45, 4))
        G = mpmath.mpf(gamma_alpha)
        size = largest(lambda k, a, b: (2 * (k + G) * X * a - (k + 2 * G - 1) * b) / (k + 1),
                       n, 1, 2 * G * X)
        add("eval_gegenbauer", [n, gamma_alpha, x], size, mpmath.gegenbauer(n, gamma_alpha, x))
    for _ in range(150):
        level_top = int(generator.integers(0, 60))
        m = int(generator.integers(-level_top, level_top + 1)) if level_top else 0
        x = float(generator.uniform(-1, 1))
        X = mpmath.mpf(x)
        order = abs(m)
        values = [mpmath.legenp(level, order, X, type=2) for level in range(order, level_top + 1)]
        scale = 1
        if m < 0:
            scale = ((-1) ** order * mpmath.factorial(level_top - order)
                     / mpmath.factorial(level_top + order))
        size = max(abs(v) for v in values) * abs(scale)
        add("lpmv", [m, float(level_top), x], size, mpmath.legenp(level_top, m, X, type=2))
    for _ in range(150):
        n = int(generator.integers(0, 80))
        m = int(generator.integers(-n, n + 1)) if n else 0
        theta = float(generator.uniform(0, math.pi))
        phi = float(generator.uniform(-math.pi, math.pi))
        values = [mpmath.spherharm(level, abs(m), theta, 0) for level in range(abs(m), n + 1)]
        size = max(abs(v) for v in values)
        y = mpmath.spherharm(n, m, theta, phi)
        add("sph_harm_y_re", [n, m, theta, phi], size, mpmath.re(y))
        add("sph_harm_y_im", [n, m, theta, phi], size, mpmath.im(y))
edges = [
    ("eval_legendre", (-3, 0.5)), ("eval_legendre", (3, inf)), ("eval_legendre", (0, nan)),
    ("eval_chebyt", (-2, 0.3)), ("eval_chebyu", (-2, 0.3)), ("eval_chebyu", (-1, 0.3)),
    ("eval_hermite", (-1, 0.5)), ("eval_hermitenorm", (-1, 0.5)), ("eval_laguerre", (-1, 0.5)),
    ("eval_genlaguerre", (2, -1.5, 0.5)), ("eval_genlaguerre", (2, -1.0, 0.5)),
    ("eval_genlaguerre", (-2, 0.5, 0.5)), ("eval_gegenbauer", (3, 0.0, 0.5)),
    ("eval_gegenbauer", (0, 0.0, 0.5)), ("eval_gegenbauer", (-1, 0.5, 0.5)),
    ("eval_jacobi", (-1, 0.5, 0.5, 0.3)), ("lpmv", (2, 3.0, 1.5)), ("lpmv", (4, 3.0, 0.5)),
    ("lpmv", (2, -4.0, 0.5)), ("lpmv", (-4, 3.0, 0.5)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) if isinstance(x, float) else str(x) for x in arguments]
                          + [repr(abs(value) if math.isfinite(value) else 1.0), repr(value)]))
with open("orthogonal_polynomials.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Gauss quadrature

In `gauss_quadrature.txt` each case is one node, `roots_<family>_x`, or one weight,
`roots_<family>_w`, of a rule: the number of nodes, the node's index, the parameters α and β (0.0
where the family has none), the size its error is measured against, and the reference. A weight's
size is its own; a node's is the larger of itself and its distance to the nearest other node, since
Newton's method fixes a node to a fraction of that gap, and a node near 0 has no more precision than
that. The rules of 10 nodes or fewer are recorded whole, and of larger rules the three nodes at each
end, the node a quarter of the way in and the middle one. The script below recorded them with mpmath
1.4.1 at 60 digits. It starts from NumPy's eigenvalues of each Jacobi matrix in `f64`, polishes
every node with Newton's method at 60 digits, and checks that no two nodes met. The weights are μ₀ /
Σ qₖ(xᵢ)², exact at the polished node. Chebyshev's rules are their closed forms at 60 digits. The
last lines are SciPy 1.18.1's NaN rules at the edge of a parameter's range. Run it from this
directory with `python3 -I`; it takes about a minute.

```python
import math

import mpmath
import numpy
import scipy.special

lines = []
nan = math.nan


def add(name, n, i, alpha, beta, size, value):
    lines.append(" ".join([name, str(n), str(i), repr(alpha), repr(beta),
                           repr(float(size)), repr(float(value))]))


def jacobi_matrix(family, n, alpha, beta):
    """The diagonal, the off-diagonal and μ₀ of the family's Jacobi matrix, at mpmath's
    precision."""
    a, b = mpmath.mpf(alpha), mpmath.mpf(beta)
    if family == "legendre":
        return [0] * n, [(k + 1) / mpmath.sqrt(4 * (k + 1) ** 2 - 1) for k in range(n)], 2
    if family == "hermite":
        off = [mpmath.sqrt(mpmath.mpf(k + 1) / 2) for k in range(n)]
        return [0] * n, off, mpmath.sqrt(mpmath.pi)
    if family == "hermitenorm":
        return [0] * n, [mpmath.sqrt(k + 1) for k in range(n)], mpmath.sqrt(2 * mpmath.pi)
    if family in ("laguerre", "genlaguerre"):
        diagonal = [2 * k + a + 1 for k in range(n)]
        return diagonal, [mpmath.sqrt((k + 1) * (k + 1 + a)) for k in range(n)], mpmath.gamma(a + 1)
    if family == "gegenbauer":
        total = mpmath.sqrt(mpmath.pi) * mpmath.gamma(a + 0.5) / mpmath.gamma(a + 1)
        a = b = a - mpmath.mpf(0.5)
    else:
        total = 2 ** (a + b + 1) * mpmath.beta(a + 1, b + 1)
    diagonal, off = [], []
    for k in range(n):
        s = 2 * k + a + b
        diagonal.append((b - a) / (a + b + 2) if k == 0 else (b * b - a * a) / (s * (s + 2)))
        m = k + 1
        t = 2 * m + a + b
        if k == 0:
            squared = 4 * (1 + a) * (1 + b) / (t * t * (t + 1))
        else:
            squared = 4 * m * (m + a) * (m + b) * (m + a + b) / (t * t * (t + 1) * (t - 1))
        off.append(mpmath.sqrt(squared))
    return diagonal, off, total


def evaluate(diagonal, off, x):
    """qₙ(x), its derivative and Σ qₖ(x)² below n, with q₀ = 1."""
    previous, current, previous_slope, slope, squares, back = 0, 1, 0, 0, 0, 0
    for k in range(len(diagonal)):
        squares += current * current
        shift = x - diagonal[k]
        following = (shift * current - back * previous) / off[k]
        following_slope = (current + shift * slope - back * previous_slope) / off[k]
        previous, current, previous_slope, slope = current, following, slope, following_slope
        back = off[k]
    return current, slope, squares


def rule(family, n, alpha, beta):
    """The n-point rule at 50 digits: NumPy's eigenvalues of the Jacobi matrix, each polished
    by Newton's method on qₙ until it stops moving, and the weights μ₀ / Σ qₖ(xᵢ)²."""
    if family == "chebyt":
        nodes = [mpmath.sin((2 * k - n + 1) * mpmath.pi / (2 * n)) for k in range(n)]
        return nodes, [mpmath.pi / n] * n
    if family == "chebyu":
        angles = [(n - k) * mpmath.pi / (n + 1) for k in range(n)]
        nodes = [mpmath.sin((2 * k - n + 1) * mpmath.pi / (2 * (n + 1))) for k in range(n)]
        return nodes, [mpmath.pi * mpmath.sin(t) ** 2 / (n + 1) for t in angles]
    diagonal, off, total = jacobi_matrix(family, n, alpha, beta)
    matrix = numpy.diag([float(d) for d in diagonal])
    for k in range(n - 1):
        matrix[k, k + 1] = matrix[k + 1, k] = float(off[k])
    start = numpy.linalg.eigvalsh(matrix)
    nodes, weights = [], []
    for guess in start:
        x = mpmath.mpf(float(guess))
        for _ in range(100):
            value, slope, _ = evaluate(diagonal, off, x)
            step = value / slope
            x -= step
            if abs(step) <= mpmath.mpf(10) ** -55 * (1 + abs(x)):
                break
        nodes.append(x)
        weights.append(total / evaluate(diagonal, off, x)[2])
    for left, right in zip(nodes, nodes[1:]):
        assert right - left > mpmath.mpf(10) ** -10, (family, n, alpha, beta)
    return nodes, weights


def scipy_rule(family, n, alpha, beta):
    if family in ("genlaguerre", "gegenbauer"):
        return getattr(scipy.special, "roots_" + family)(n, alpha)
    if family == "jacobi":
        return scipy.special.roots_jacobi(n, alpha, beta)
    return getattr(scipy.special, "roots_" + family)(n)


def node_size(nodes, i):
    """The size a node's error is measured against: the larger of the node and its distance to
    the nearest other node, the scale on which Newton's method fixes it."""
    gaps = [abs(nodes[i] - nodes[j]) for j in (i - 1, i + 1) if 0 <= j < len(nodes)]
    return max([abs(nodes[i])] + ([min(gaps)] if gaps else []))


def indices(n):
    if n <= 10:
        return range(n)
    return sorted({0, 1, 2, n // 4, n // 2, n - 3, n - 2, n - 1})


rules = [("legendre", 0.0, 0.0), ("chebyt", 0.0, 0.0), ("chebyu", 0.0, 0.0),
         ("hermite", 0.0, 0.0), ("hermitenorm", 0.0, 0.0), ("laguerre", 0.0, 0.0)]
rules += [("genlaguerre", alpha, 0.0) for alpha in (-0.75, 0.5, 3.0, 20.0)]
rules += [("jacobi", alpha, beta) for alpha, beta in
          ((-0.5, -0.5), (0.5, 1.5), (-0.75, 0.25), (3.0, -0.9), (10.0, 2.0))]
rules += [("gegenbauer", alpha, 0.0) for alpha in (-0.4, 0.25, 1.0, 2.5)]
with mpmath.workdps(60):
    for family, alpha, beta in rules:
        for n in (1, 2, 3, 4, 5, 7, 10, 16, 25, 40, 64, 100, 150):
            nodes, weights = rule(family, n, alpha, beta)
            for i in indices(n):
                add("roots_" + family + "_x", n, i, alpha, beta, node_size(nodes, i), nodes[i])
            for i in indices(n):
                add("roots_" + family + "_w", n, i, alpha, beta, weights[i], weights[i])
    # Rules large enough that the recurrences pass the range of f64 in their tails.
    for family in ("hermite", "hermitenorm", "laguerre"):
        nodes, weights = rule(family, 400, 0.0, 0.0)
        for i in indices(400):
            add("roots_" + family + "_x", 400, i, 0.0, 0.0, node_size(nodes, i), nodes[i])
        for i in indices(400):
            add("roots_" + family + "_w", 400, i, 0.0, 0.0, weights[i], weights[i])
# The ends, as SciPy 1.18.1 gives them: NaN rules at the edge of the parameter's range, below
# which SciPy raises ValueError.
for family, alpha in (("genlaguerre", -1.0), ("gegenbauer", -0.5)):
    x, w = scipy_rule(family, 3, alpha, 0.0)
    for i in range(3):
        add("roots_" + family + "_x", 3, i, alpha, 0.0, 1.0, x[i])
        add("roots_" + family + "_w", 3, i, alpha, 0.0, 1.0, w[i])
with open("gauss_quadrature.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Bessel functions of real order

In `bessel_real.txt` each case holds, before the reference value, the size its error is
measured against. Past the turning point x = |v|, J and Y and their derivatives are measured
against the modulus √(J² + Y²), the envelope they oscillate under; everywhere else, and for I
and K always, against the value itself. A case of `jvp`, `yvp`, `ivp` or `kvp` holds the order
of the derivative after the argument, and a case of a spherical function holds 1 there for its
derivative and 0 for its value. mpmath's `besselk` gives no derivative, so the script sums K at
neighbouring orders (DLMF 10.29.5) at 50 digits instead. The script below recorded the cases
with its own seed, with mpmath 1.4.1 and SciPy 1.18.1 at the ends; run it from this directory
with `python3 -I`:

```python
import math

import mpmath
import numpy
import scipy.special

generator = numpy.random.default_rng(318005)
inf, nan = math.inf, math.nan
lines = []


def add(name, arguments, size, value):
    lines.append(" ".join([name] + [repr(x) if isinstance(x, float) else str(x) for x in arguments]
                          + [repr(float(size)), repr(float(value))]))


def besselk_slope(v, x, n):
    """The n-th derivative of K_v(x), which mpmath's besselk does not give, by DLMF 10.29.5."""
    return sum(mpmath.binomial(n, k) * mpmath.besselk(v - n + 2 * k, x)
               for k in range(n + 1)) / (-2) ** n


def order():
    """An order: small, whole or half-whole, or anywhere up to 60 in size, of either sign."""
    kind = generator.uniform()
    if kind < 0.3:
        v = float(generator.uniform(0, 3))
    elif kind < 0.45:
        v = float(generator.integers(0, 40))
    elif kind < 0.6:
        v = float(generator.integers(0, 40)) + 0.5
    else:
        v = float(generator.uniform(0, 60))
    if generator.uniform() < 0.3:
        v = -v
    return v


def argument(v):
    """An argument: spread from 1e-3 to 1e4 on a log scale, near the turning point |v|, or
    far out, up to 1e8."""
    kind = generator.uniform()
    if kind < 0.6:
        return float(10 ** generator.uniform(-3, 4))
    if kind < 0.85:
        return float(abs(v) * generator.uniform(0.8, 1.2) + generator.uniform(0, 1))
    return float(10 ** generator.uniform(4, 8))


with mpmath.workdps(50):
    for _ in range(500):
        v = order()
        x = argument(v)
        V, X = mpmath.mpf(v), mpmath.mpf(x)
        j, y = mpmath.besselj(V, X), mpmath.bessely(V, X)
        jp, yp = mpmath.besselj(V, X, derivative=1), mpmath.bessely(V, X, derivative=1)
        modulus, slope = mpmath.sqrt(j * j + y * y), mpmath.sqrt(jp * jp + yp * yp)
        # Past the turning point |v| the functions oscillate, and are measured against the
        # modulus sqrt(J² + Y²), their envelope; before it, against themselves.
        inside = x <= abs(v)
        add("jv", [v, x], abs(j) if inside else modulus, j)
        add("yv", [v, x], abs(y) if inside else modulus, y)
        add("jvp", [v, x, 1], abs(jp) if inside else slope, jp)
        add("yvp", [v, x, 1], abs(yp) if inside else slope, yp)
        if x < 700:
            i, k = mpmath.besseli(V, X), mpmath.besselk(V, X)
            add("iv", [v, x], abs(i), i)
            add("kv", [v, x], abs(k), k)
            ip, kp = mpmath.besseli(V, X, derivative=1), besselk_slope(V, X, 1)
            add("ivp", [v, x, 1], abs(ip), ip)
            add("kvp", [v, x, 1], abs(kp), kp)
        ie = mpmath.besseli(V, X) * mpmath.exp(-X)
        ke = mpmath.besselk(V, X) * mpmath.exp(X)
        add("ive", [v, x], abs(ie), ie)
        add("kve", [v, x], abs(ke), ke)
    # Higher derivatives at moderate arguments.
    for _ in range(40):
        v = order()
        x = float(10 ** generator.uniform(-1, 2))
        n = int(generator.integers(2, 5))
        V, X = mpmath.mpf(v), mpmath.mpf(x)
        jp, yp = mpmath.besselj(V, X, derivative=n), mpmath.bessely(V, X, derivative=n)
        slope = mpmath.sqrt(jp * jp + yp * yp)
        add("jvp", [v, x, n], slope, jp)
        add("yvp", [v, x, n], slope, yp)
        ip, kp = mpmath.besseli(V, X, derivative=n), besselk_slope(V, X, n)
        add("ivp", [v, x, n], abs(ip), ip)
        add("kvp", [v, x, n], abs(kp), kp)
    # Negative arguments at whole orders.
    for _ in range(20):
        v = float(generator.integers(-20, 21))
        x = -float(10 ** generator.uniform(-2, 2.5))
        V, X = mpmath.mpf(v), mpmath.mpf(x)
        j = mpmath.besselj(V, X)
        add("jv", [v, x], mpmath.sqrt(j * j + mpmath.bessely(V, -X) ** 2), j)
        i = mpmath.besseli(V, X)
        add("iv", [v, x], abs(i), i)
        add("ive", [v, x], abs(i) * mpmath.exp(X), i * mpmath.exp(X))
    # Spherical Bessel functions and their derivatives, of either sign of argument.
    def spherical(kind, n, x):
        """jₙ, yₙ, iₙ or kₙ at 50 digits; below 0 by their parity, and kₙ by SciPy's closed
        form, which continues it there."""
        if kind == "kn" and x < 0:
            return (mpmath.pi / 2 * mpmath.exp(-x) / x
                    * sum(mpmath.factorial(n + k) / (mpmath.factorial(k) * mpmath.factorial(n - k))
                          * (2 * x) ** -k for k in range(n + 1)))
        if x < 0:
            sign = (-1) ** n if kind in ("jn", "in") else (-1) ** (n + 1)
            return sign * spherical(kind, n, -x)
        cylinder = {"jn": mpmath.besselj, "yn": mpmath.bessely, "in": mpmath.besseli,
                    "kn": mpmath.besselk}[kind]
        return mpmath.sqrt(mpmath.pi / (2 * x)) * cylinder(n + mpmath.mpf(0.5), x)

    def slope(kind, n, x):
        """The derivative, by DLMF 10.51.2 and 10.51.5 at 50 digits."""
        if n == 0:
            return {"jn": -1, "yn": -1, "in": 1, "kn": -1}[kind] * spherical(kind, 1, x)
        below = -1 if kind == "kn" else 1
        return below * spherical(kind, n - 1, x) - (n + 1) / x * spherical(kind, n, x)

    for _ in range(150):
        n = int(generator.integers(0, 12)) if generator.uniform() < 0.5 else int(
            generator.integers(0, 60))
        x = float(10 ** generator.uniform(-3, 3.5))
        if generator.uniform() < 0.15:
            x = -x
        X = mpmath.mpf(x)
        for kind in ("jn", "yn", "in", "kn"):
            if kind in ("in", "kn") and abs(x) > 700:
                continue
            for derivative in (0, 1):
                value = slope(kind, n, X) if derivative else spherical(kind, n, X)
                size = abs(value)
                if kind in ("jn", "yn") and abs(x) > n:
                    pair = [slope(k, n, X) if derivative else spherical(k, n, X)
                            for k in ("jn", "yn")]
                    size = mpmath.sqrt(pair[0] ** 2 + pair[1] ** 2)
                add("spherical_" + kind, [n, x, derivative], size, value)
# Large orders, from 60 to 250, where the recurrences of Steed's method run long. mpmath's
# besselk needs more digits here, as I₋ᵥ − Iᵥ cancels.
with mpmath.workdps(150):
    for _ in range(100):
        v = float(generator.uniform(60, 250))
        x = float(10 ** generator.uniform(0, 3.3))
        V, X = mpmath.mpf(v), mpmath.mpf(x)
        j, y = mpmath.besselj(V, X), mpmath.bessely(V, X)
        modulus = mpmath.sqrt(j * j + y * y)
        inside = x <= abs(v)
        for name, value in (("jv", j), ("yv", y)):
            if 1e-300 < abs(value) < 1e300:
                add(name, [v, x], abs(value) if inside else modulus, value)
        for name, value in (("iv", mpmath.besseli(V, X)), ("kv", mpmath.besselk(V, X))):
            if 1e-300 < abs(value) < 1e300:
                add(name, [v, x], abs(value), value)
# The ends, as SciPy 1.18.1 gives them, except where SciPy's value is not the limit, which the
# reference lists. At x = 0 and a negative order v that is not whole, J_v and I_v are infinite
# with the sign of 1/gamma(v + 1), where SciPy gives +inf for iv and NaN for jve and ive, and
# Y_v is 0 when -v is half-whole, since Y_v = ±J_(-v) there, where SciPy gives an infinity.
# The scaled forms are their functions at 0 and at an infinite order, where SciPy gives NaN,
# and ive and kve stay finite past 1e10. yv and kv at an infinite order, K of either sign, and
# jv, yv and iv at an infinite argument, are their limits.
large = mpmath.mpf(10) ** 10
ends = {
    ("yv", -0.5, 0.0): 0.0, ("yv", -1.5, 0.0): 0.0, ("iv", -1.5, 0.0): -inf,
    ("jve", -0.5, 0.0): inf, ("jve", -1.5, 0.0): -inf,
    ("ive", -0.5, 0.0): inf, ("ive", -1.5, 0.0): -inf,
    ("yve", 0.0, 0.0): -inf, ("yve", 1.0, 0.0): -inf, ("yve", 0.5, 0.0): -inf,
    ("yve", -0.5, 0.0): 0.0, ("yve", -1.0, 0.0): inf, ("yve", -1.5, 0.0): 0.0,
    ("yve", 2.5, 0.0): -inf, ("yve", -2.0, 0.0): -inf,
    ("ive", 0.0, 1e10): float(1 / mpmath.sqrt(2 * mpmath.pi * large)),
    ("ive", 0.7, 1e10): float(mpmath.besseli(0.7, large) * mpmath.exp(-large)),
    ("kve", 0.0, 1e10): float(mpmath.besselk(0, large) * mpmath.exp(large)),
    ("kve", 0.7, 1e10): float(mpmath.besselk(0.7, large) * mpmath.exp(large)),
    ("kv", -inf, 1.0): inf, ("kve", -inf, 1.0): inf,
    ("jv", 0.5, inf): 0.0, ("yv", 0.5, inf): 0.0, ("iv", 0.5, inf): inf, ("ive", 0.5, inf): 0.0,
    ("kve", 0.5, inf): 0.0, ("jve", 0.5, inf): 0.0, ("yve", 0.5, inf): 0.0,
    ("yv", inf, 1.0): -inf, ("kv", inf, 1.0): inf, ("kve", inf, 1.0): inf,
    ("jv", -inf, 1.0): nan, ("iv", -inf, 1.0): nan, ("jve", inf, 1.0): 0.0,
    ("ive", inf, 1.0): 0.0, ("yve", inf, 1.0): -inf,
}
points = [(0.0, 0.0), (1.0, 0.0), (0.5, 0.0), (-0.5, 0.0), (-1.0, 0.0), (-1.5, 0.0), (2.5, 0.0),
          (-2.0, 0.0), (2.0, -1.5), (2.5, -1.5), (-3.0, -1.5), (0.5, inf), (0.5, -inf), (inf, 1.0),
          (-inf, 1.0), (nan, 1.0), (1.0, nan), (0.0, 1e10), (0.7, 1e10)]
for name in ("jv", "yv", "iv", "kv", "jve", "yve", "ive", "kve"):
    for v, x in points:
        with numpy.errstate(all="ignore"):
            value = float(getattr(scipy.special, name)(v, x))
        value = ends.get((name, v, x), value)
        if math.isfinite(value) and value != 0.0 and (name, v, x) not in ends:
            continue
        add(name, [v, x], 1.0, value)
# The spherical functions' ends, as SciPy gives them, except yₙ′(0) for n from 1, which is ∞
# where SciPy gives NaN, and iₙ′(∞), which is ∞ where SciPy gives NaN from n = 3.
for kind in ("jn", "yn", "in", "kn"):
    for derivative in (False, True):
        for n, x in [(0, 0.0), (1, 0.0), (2, 0.0), (-1, 1.0), (0, inf), (0, -inf), (3, inf),
                     (0, nan)]:
            with numpy.errstate(all="ignore"):
                value = float(getattr(scipy.special, "spherical_" + kind)(n, x, derivative))
            if kind == "yn" and derivative and x == 0.0 and n >= 1:
                value = inf
            if kind == "in" and derivative and x == inf:
                value = inf
            add("spherical_" + kind, [n, x, int(derivative)], 1.0, value)
with open("bessel_real.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Airy functions

In `airy.txt` each case holds, before the reference value, the size its error is measured
against: below 0, where Ai and Bi oscillate, the envelope √(Ai² + Bi²) for Ai and Bi and
√(Ai′² + Bi′²) for the derivatives; above 0, the value itself. Each of `airy` and `airye` is
recorded as four names, one for each value it returns. The script below recorded the cases with
its own seed, with mpmath 1.4.1 at 60 digits; run it from this directory with `python3 -I`:

```python
import math
import random

import mpmath

random.seed(318007)
lines = []


def add(name, x, size, value):
    lines.append(f"{name} {x!r} {float(size)!r} {float(value)!r}")


def record(x):
    with mpmath.workdps(60):
        m = mpmath.mpf(x)
        ai, aip = mpmath.airyai(m), mpmath.airyai(m, derivative=1)
        bi, bip = mpmath.airybi(m), mpmath.airybi(m, derivative=1)
        if x < 0:
            # Below 0 each value is measured against its envelope, √(Ai² + Bi²) or
            # √(Ai′² + Bi′²), and the scale of `airye` is 1.
            values = mpmath.sqrt(ai ** 2 + bi ** 2)
            slopes = mpmath.sqrt(aip ** 2 + bip ** 2)
            sizes = [values, slopes, values, slopes]
            scaled = [ai, aip, bi, bip]
        else:
            zeta = 2 * m ** mpmath.mpf(1.5) / 3
            scaled = [ai * mpmath.exp(zeta), aip * mpmath.exp(zeta),
                      bi * mpmath.exp(-zeta), bip * mpmath.exp(-zeta)]
            sizes = None
        for name, value, size in zip(("ai", "aip", "bi", "bip"), (ai, aip, bi, bip),
                                     sizes or (ai, aip, bi, bip)):
            if abs(value) < 1e-300 or abs(value) > 1e300:
                continue
            add("airy_" + name, x, abs(size), value)
        for name, value, size in zip(("ai", "aip", "bi", "bip"), scaled, sizes or scaled):
            add("airye_" + name, x, abs(size), value)


points = [random.uniform(-2.0, 1.0) for _ in range(150)]
points += [-math.exp(random.uniform(math.log(2.0), math.log(1e4))) for _ in range(150)]
points += [math.exp(random.uniform(0.0, math.log(103.0))) for _ in range(120)]
points += [math.exp(random.uniform(math.log(103.0), math.log(1e8))) for _ in range(30)]
points += [0.0, 1.0, -2.0, 1e-300, -1e-300, -2.3381074104597670, -1.0187929716474710]
for x in points:
    record(x)
# The ends. Where SciPy gives NaN, at x = ±∞ and past |x| = 1e8 or so, these are the limits
# and the true values; past |x| = 1.5e205, where ζ overflows, `airye` above 0 is the first term
# of each expansion, exact there, and below 0 every value is NaN, since the phase is lost.
inf, nan = math.inf, math.nan
huge = 1e210
quarter = huge ** 0.25
root_pi = math.sqrt(math.pi)
ends = [
    ("airy", inf, (0.0, 0.0, inf, inf)),
    ("airye", inf, (0.0, -inf, 0.0, inf)),
    ("airy", -inf, (0.0, nan, 0.0, nan)),
    ("airye", -inf, (0.0, nan, 0.0, nan)),
    ("airy", nan, (nan, nan, nan, nan)),
    ("airye", nan, (nan, nan, nan, nan)),
    ("airy", huge, (0.0, 0.0, inf, inf)),
    ("airye", huge, (1 / (2 * root_pi * quarter), -quarter / (2 * root_pi),
                     1 / (root_pi * quarter), quarter / root_pi)),
    ("airy", -huge, (nan, nan, nan, nan)),
    ("airye", -huge, (nan, nan, nan, nan)),
    ("airy", 200.0, (0.0, 0.0, inf, inf)),
]
for family, x, values in ends:
    for name, value in zip(("ai", "aip", "bi", "bip"), values):
        add(family + "_" + name, x, abs(value) if math.isfinite(value) else 1.0, value)
with open("airy.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Kelvin functions

In `kelvin.txt` each case holds, before the reference value, the size its error is measured
against: the modulus of the complex value the function is a part of, |ber + i bei| for `ber` and
`bei` and so on, since each part passes through 0 as the phase turns. The script below recorded
the cases with its own seed, with mpmath 1.4.1 at 60 digits; run it from this directory with
`python3 -I`:

```python
import math
import random

import mpmath

random.seed(318070)
lines = []


def add(name, x, size, value):
    lines.append(f"{name} {x!r} {float(size)!r} {float(value)!r}")


def record(x):
    with mpmath.workdps(60):
        m = mpmath.mpf(abs(x))
        z = m * mpmath.expjpi(mpmath.mpf(1) / 4)
        rot = mpmath.expjpi(mpmath.mpf(1) / 4)
        i0, i1 = mpmath.besseli(0, z), mpmath.besseli(1, z)
        k0, k1 = mpmath.besselk(0, z), mpmath.besselk(1, z)
        be, bep = i0, rot * i1
        ke, kep = k0, -rot * k1
        if x < 0:
            bep = -bep
        # Each part is measured against the size of the complex value it belongs to.
        for name, value in (("ber", be.real), ("bei", be.imag), ("berp", bep.real),
                            ("beip", bep.imag)):
            size = abs(be) if name in ("ber", "bei") else abs(bep)
            if size < 1e300:
                add(name, x, size, value)
        if x > 0:
            for name, value in (("ker", ke.real), ("kei", ke.imag), ("kerp", kep.real),
                                ("keip", kep.imag)):
                size = abs(ke) if name in ("ker", "kei") else abs(kep)
                if size > 1e-300:
                    add(name, x, size, value)


points = [random.uniform(0.0, 1.0) for _ in range(60)]
points += [random.uniform(1.0, 30.0) for _ in range(200)]
points += [math.exp(random.uniform(math.log(30.0), math.log(1000.0))) for _ in range(80)]
points += [-random.uniform(0.0, 50.0) for _ in range(30)]
points += [1e-10, 1.0, 1.01, 1.08, 2.0, 2.5, 1003.0]
for x in points:
    record(x)
# The ends: at 0, the series' first terms, with ker = ∞ and ker′ = −∞; below 0, ker and kei
# and their derivatives NaN; at x = ∞, where SciPy gives NaN for all, ker and kei and their
# derivatives 0 and the rest NaN; and from x = 1004 the overflow of ber and bei, with the sign of
# the phase.
inf, nan = math.inf, math.nan
names = ("ber", "bei", "ker", "kei", "berp", "beip", "kerp", "keip")
ends = [
    (0.0, (1.0, 0.0, inf, -math.pi / 4, 0.0, 0.0, -inf, 0.0)),
    (inf, (nan, nan, 0.0, 0.0, nan, nan, 0.0, 0.0)),
    (nan, (nan,) * 8),
    (-2.0, (None, None, nan, nan, None, None, nan, nan)),
]
with mpmath.workdps(60):
    z = 1100 * mpmath.expjpi(mpmath.mpf(1) / 4)
    be, bep = mpmath.besseli(0, z), mpmath.expjpi(mpmath.mpf(1) / 4) * mpmath.besseli(1, z)
    signs = [math.copysign(inf, float(mpmath.sign(part)))
             for part in (be.real, be.imag, bep.real, bep.imag)]
ends.append((1100.0, (signs[0], signs[1], 0.0, 0.0, signs[2], signs[3], 0.0, 0.0)))
for x, values in ends:
    for name, value in zip(names, values):
        if value is not None:
            add(name, x, abs(value) if math.isfinite(value) and value != 0 else 1.0, value)
with open("kelvin.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Struve functions

In `struve.txt` each case holds, before the reference value, the size its error is measured
against: for H past the turning point x = |v|, where it oscillates about Yᵥ, the envelope
max(|H|, √(Jᵥ² + Yᵥ²)); everywhere else, and for L always, the value itself. The script below
recorded the cases with its own seed, with mpmath 1.4.1's `struveh` and `struvel` at 60 digits;
run it from this directory with `python3 -I`:

```python
import math
import random

import mpmath

random.seed(318071)
lines = []


def add(name, v, x, size, value):
    lines.append(f"{name} {v!r} {x!r} {float(size)!r} {float(value)!r}")


def record(v, x):
    with mpmath.workdps(60):
        mv, mx = mpmath.mpf(v), mpmath.mpf(x)
        h = mpmath.struveh(mv, mx)
        l = mpmath.struvel(mv, mx)
        modulus = mpmath.hypot(mpmath.besselj(mv, mx), mpmath.bessely(mv, mx))
        # Past the turning point x = |v|, H is measured against the larger of itself and the
        # modulus √(Jᵥ² + Yᵥ²) that Yᵥ, of which it is a sum, oscillates under; before it, and
        # L everywhere, against itself.
        size = abs(h)
        if x > abs(v):
            size = max(size, modulus)
        if 1e-300 < abs(h) < 1e300:
            add("struve", v, x, size, h)
        if 1e-300 < abs(l) < 1e300:
            add("modstruve", v, x, abs(l), l)


for _ in range(400):
    v = random.uniform(-10.0, 30.0)
    x = math.exp(random.uniform(math.log(1e-3), math.log(500.0)))
    record(v, x)
for _ in range(100):
    v = float(random.randint(-6, 12)) + random.choice([0.0, 0.5])
    x = math.exp(random.uniform(math.log(1e-2), math.log(200.0)))
    record(v, x)
for _ in range(60):
    v = random.uniform(-5.0, 5.0)
    x = math.exp(random.uniform(math.log(500.0), math.log(1e9)))
    record(v, x)
for _ in range(40):
    v = random.uniform(30.0, 100.0)
    x = math.exp(random.uniform(math.log(1.0), math.log(300.0)))
    record(v, x)
# The ends. At x = 0, the first term of the series: 0 above order −1, 2/π at −1, and below it
# an infinity with the sign of 1/Γ(v + 3/2), except at the orders −3/2, −5/2 and so on, where
# the function is ±J₋ᵥ(0) = 0 and SciPy gives ∞ or NaN. At x = ∞, H is 0 below order 1, 2/π at
# 1 and ∞ above, where SciPy gives ∞, and L is ∞, where SciPy gives NaN at order 0. A negative
# x is NaN unless v is whole, and NaN gives NaN.
inf, nan = math.inf, math.nan
ends = [(0.0, 0.0, 0.0, 0.0), (-1.0, 0.0, 2 / math.pi, 2 / math.pi), (-1.2, 0.0, inf, inf),
        (-1.5, 0.0, 0.0, 0.0), (-2.0, 0.0, -inf, -inf), (-2.5, 0.0, 0.0, 0.0),
        (-3.0, 0.0, inf, inf), (0.5, -2.0, nan, nan), (0.0, inf, 0.0, inf),
        (1.0, inf, 2 / math.pi, inf), (2.0, inf, inf, inf), (inf, 1.0, 0.0, 0.0),
        (-inf, 1.0, nan, 0.0), (nan, 1.0, nan, nan), (1.0, nan, nan, nan), (0.0, 1000.0, None, inf)]
for v, x, h, l in ends:
    for name, value in (("struve", h), ("modstruve", l)):
        if value is not None:
            add(name, v, x, abs(value) if math.isfinite(value) and value != 0 else 1.0, value)
# Whole orders at negative x, by Hᵥ(−x) = (−1)^{v+1} Hᵥ(x) and the same for L.
for v, x in ((0.0, -1.0), (1.0, -3.0), (2.0, -2.0), (-3.0, -4.5)):
    with mpmath.workdps(60):
        sign = (-1) ** (int(v) + 1)
        h = sign * mpmath.struveh(v, -x)
        l = sign * mpmath.struvel(v, -x)
        modulus = mpmath.hypot(mpmath.besselj(v, -x), mpmath.bessely(v, -x))
        add("struve", v, x, max(abs(h), modulus) if -x > abs(v) else abs(h), h)
        add("modstruve", v, x, abs(l), l)
with open("struve.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Hypergeometric functions

In `hypergeometric.txt` each case holds, before the reference value, the size its error is
measured against: the largest of |f| and |t ∂f/∂t| for each argument t, the error that rounding
the arguments to `f64` alone would make. ∂f/∂x is exact, (1/b) ₀F₁(; b + 1; x),
(a/b) M(a + 1, b + 1, x) or −a U(a + 1, b + 1, x); the others are central differences with a step
of 10⁻²⁵ at 60 digits. The parameters keep to the ranges where the reference says the functions
hold. The script below recorded the cases with its own seed, with mpmath 1.4.1's `hyp0f1`,
`hyp1f1` and `hyperu` at 60 digits; run it from this directory with `python3 -I`:

```python
import math
import random

import mpmath

random.seed(318081)
lines = []
STEP = mpmath.mpf(10) ** -25


def add(name, arguments, size, value):
    fields = " ".join(repr(float(t)) for t in arguments)
    lines.append(f"{name} {fields} {float(size)!r} {float(value)!r}")


def slope(f, point, index):
    # The derivative of f in its argument `index`, by a central difference at 60 digits.
    above = list(point)
    below = list(point)
    above[index] += STEP
    below[index] -= STEP
    return (f(*above) - f(*below)) / (2 * STEP)


def record(name, f, arguments, shifted):
    # Each case is measured against the largest of |f| and |t ∂f/∂t| for each argument t: the
    # error a rounding of the arguments alone would make. ∂/∂x is exact, `shifted`.
    try:
        with mpmath.workdps(60):
            point = [mpmath.mpf(t) for t in arguments]
            value = f(*point)
            if not 1e-300 < abs(value) < 1e300:
                return
            size = max(abs(value), abs(point[-1] * shifted(*point)))
            for index in range(len(point) - 1):
                size = max(size, abs(point[index] * slope(f, point, index)))
            if size > 1e300:
                return
    except (mpmath.libmp.NoConvergence, ZeroDivisionError, ValueError):
        return
    add(name, arguments, size, value)


def hyp0f1(b, x):
    return mpmath.hyp0f1(b, x, maxterms=10**6, maxprec=20000)


def hyp1f1(a, b, x):
    return mpmath.hyp1f1(a, b, x, maxterms=10**6, maxprec=20000)


def hyperu(a, b, x):
    return mpmath.hyperu(a, b, x, maxterms=10**6, maxprec=20000)


def parameter(limit):
    # Uniform, whole, near 0, near a whole number, and spread over decades.
    r = random.random()
    if r < 0.3:
        return random.uniform(-limit, limit)
    if r < 0.45:
        return float(round(random.uniform(-limit, limit)))
    if r < 0.6:
        return random.uniform(-2.0, 3.0)
    if r < 0.7:
        return round(random.uniform(-40.0, 40.0)) + random.choice([1e-9, -1e-9, 1e-5, -1e-5])
    return math.copysign(10 ** random.uniform(-4.0, math.log10(limit)), random.uniform(-1, 1))


for _ in range(400):
    b = random.choice([
        random.uniform(-30.0, 30.0),
        random.uniform(0.0, 3.0),
        math.copysign(10 ** random.uniform(-2.0, 3.0), random.uniform(-1, 1)),
    ])
    # b from −300 up: further down, I_{b−1} overflows where the series cancels.
    if b <= 0 and b == int(b) or b < -300.0:
        continue
    x = math.copysign(10 ** random.uniform(-3.0, 4.5), random.uniform(-1, 1))
    record("hyp0f1", hyp0f1, (b, x), lambda b, x: hyp0f1(b + 1, x) / b)
count = 0
while count < 1200:
    # |a| at most 100 and |x| at most 1,000, where the reference says hyp1f1 holds, except b
    # within 0.01 of 0 with x below −30.
    a, b = parameter(100.0), parameter(150.0)
    if b <= 0 and b == int(b):
        continue
    x = math.copysign(10 ** random.uniform(-4.0, 3.0), random.uniform(-1, 1))
    if abs(b) < 0.01 and x < -30.0:
        continue
    before = len(lines)
    record("hyp1f1", hyp1f1, (a, b, x), lambda a, b, x: a / b * hyp1f1(a + 1, b + 1, x))
    count += len(lines) - before
count = 0
while count < 800:
    # a from −35 up, where the reference says hyperu holds, except a below 0 and not whole with
    # b above a + 1 and within 0.001 of a whole number.
    a, b = parameter(150.0), parameter(150.0)
    if a < -35.0:
        continue
    if a < 0 and a != int(a) and b > a + 1 and abs(b - round(b)) < 1e-3:
        continue
    x = 10 ** random.uniform(-5.0, 3.5)
    before = len(lines)
    record("hyperu", hyperu, (a, b, x), lambda a, b, x: -a * hyperu(a + 1, b + 1, x))
    count += len(lines) - before
# The ends, with SciPy's value or, where it differs, the limit the reference documents.
inf = math.inf
nan = math.nan
ends = [
    ("hyp0f1", (-2.0, 1.0), nan),
    ("hyp0f1", (1.5, 0.0), 1.0),
    ("hyp0f1", (inf, 3.0), 1.0),
    ("hyp0f1", (2.0, inf), inf),
    ("hyp0f1", (-0.5, inf), -inf),
    ("hyp0f1", (1.0, -inf), 0.0),
    ("hyp0f1", (0.25, -inf), nan),
    ("hyp0f1", (nan, 1.0), nan),
    ("hyp1f1", (1.0, -2.0, 1.0), inf),
    ("hyp1f1", (-1.0, -2.0, 3.0), 2.5),
    ("hyp1f1", (1.0, 2.0, 0.0), 1.0),
    ("hyp1f1", (0.0, 2.0, 5.0), 1.0),
    ("hyp1f1", (2.0, 3.0, inf), inf),
    ("hyp1f1", (-2.0, 3.0, inf), inf),
    ("hyp1f1", (-3.0, 3.0, inf), -inf),
    ("hyp1f1", (2.0, 3.0, -inf), 0.0),
    ("hyp1f1", (-2.5, 3.0, -inf), inf),
    ("hyp1f1", (1.0, 2.0, nan), nan),
    ("hyp1f1", (inf, 1.0, 1.0), nan),
    ("hyp1f1", (1.0, inf, 1.0), 1.0),
    ("hyperu", (-2.0, 3.0, 0.0), 12.0),
    ("hyperu", (0.5, 0.5, 0.0), math.sqrt(math.pi)),
    ("hyperu", (-1.5, 0.5, 0.0), 0.0),
    ("hyperu", (1.0, 1.0, 0.0), inf),
    ("hyperu", (1.0, 2.0, inf), 0.0),
    ("hyperu", (-2.5, 1.0, inf), inf),
    ("hyperu", (1.0, 1.0, -1.0), nan),
    ("hyperu", (0.0, 2.0, 3.0), 1.0),
    ("hyperu", (inf, 1.0, 1.0), nan),
]
for name, arguments, value in ends:
    add(name, arguments, abs(value) if math.isfinite(value) and value != 0 else 1.0, value)
with open("hypergeometric.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## The Gauss hypergeometric function

In `gauss.txt` each case holds, before the reference value, the size its error is measured
against, as for the confluent functions: the largest of |F| and |t ∂F/∂t| for each of a, b, c
and x. ∂F/∂x is exact, (ab/c) F(a + 1, b + 1; c + 1; x); the others are central differences with
a step of 10⁻²⁵ at 60 digits. The script below recorded the cases with its own seed, with
mpmath 1.4.1's `hyp2f1` at 60 digits; run it from this directory with `python3 -I`:

```python
import math
import random

import mpmath

random.seed(318082)
lines = []
STEP = mpmath.mpf(10) ** -25


def add(arguments, size, value):
    fields = " ".join(repr(float(t)) for t in arguments)
    lines.append(f"hyp2f1 {fields} {float(size)!r} {float(value)!r}")


def f(a, b, c, x):
    return mpmath.hyp2f1(a, b, c, x, maxterms=10**6, maxprec=20000)


def record(arguments):
    # Measured against the largest of |F| and |t ∂F/∂t| for each argument t; ∂F/∂x is
    # (ab/c) F(a + 1, b + 1; c + 1; x), the others central differences at 60 digits.
    try:
        with mpmath.workdps(60):
            a, b, c, x = (mpmath.mpf(t) for t in arguments)
            value = f(a, b, c, x)
            if not 1e-300 < abs(value) < 1e300:
                return False
            size = max(abs(value), abs(x * a * b / c * f(a + 1, b + 1, c + 1, x)))
            point = [a, b, c, x]
            for index in range(3):
                above = list(point)
                below = list(point)
                above[index] += STEP
                below[index] -= STEP
                size = max(size, abs(point[index] * (f(*above) - f(*below)) / (2 * STEP)))
            if size > 1e300:
                return False
    except (mpmath.libmp.NoConvergence, ZeroDivisionError, ValueError):
        return False
    add(arguments, size, value)
    return True


def parameter():
    # Parameters up to 20 in size, where the reference says hyp2f1 holds: uniform, whole,
    # near 0, near a whole number, and spread over decades.
    r = random.random()
    if r < 0.35:
        return random.uniform(-20.0, 20.0)
    if r < 0.5:
        return float(round(random.uniform(-20.0, 20.0)))
    if r < 0.7:
        return random.uniform(-2.0, 3.0)
    if r < 0.8:
        return round(random.uniform(-20.0, 20.0)) + random.choice([1e-9, -1e-9, 1e-5, -1e-5])
    return math.copysign(10 ** random.uniform(-4.0, math.log10(20.0)), random.uniform(-1, 1))


def point():
    # Across (−1, 1), close to 1, and far below −1.
    r = random.random()
    if r < 0.3:
        return random.uniform(-1.0, 1.0)
    if r < 0.6:
        return 1.0 - 10 ** random.uniform(-8.0, 0.0)
    if r < 0.8:
        return -(10 ** random.uniform(-3.0, 4.0))
    return random.uniform(-0.6, 0.6)


count = 0
while count < 1200:
    a, b, c = parameter(), parameter(), parameter()
    if c <= 0 and c == int(c):
        continue
    if random.random() < 0.15:
        # c − a − b whole or near it, where the series in 1 − x needs its logarithm.
        c = a + b + random.choice([0.0, 1.0, 2.0, -1.0, 3.0, 1e-7, 0.01])
    count += record((a, b, c, point()))
# Polynomials past x = 1, where the function is real.
for _ in range(40):
    a = float(random.randint(-12, -1))
    b, c = parameter(), parameter()
    if c <= 0 and c == int(c):
        continue
    record((a, b, c, random.uniform(1.0, 50.0)))
# The ends, with SciPy's value or, where it differs, the one the reference documents.
inf = math.inf
nan = math.nan
ends = [
    ((1.0, 1.0, 2.0, 2.0), nan),
    ((0.5, 0.5, 1.5, 1.0), math.pi / 2),
    ((1.0, 1.0, 1.5, 1.0), inf),
    ((1.0, 1.0, 2.0, 1.0), inf),
    ((1.0, 2.0, -3.0, 0.5), inf),
    ((-2.0, 1.0, -3.0, 0.5), 17.0 / 12.0),
    ((-2.0, 1.0, -1.0, 0.5), inf),
    ((-2.0, 3.0, 4.0, 5.0), 8.5),
    ((1.0, 1.0, 2.0, 0.0), 1.0),
    ((1.0, 1.0, inf, 0.5), 1.0),
    ((inf, 1.0, 2.0, 0.5), nan),
    ((1.0, 1.0, 2.0, -inf), 0.0),
    ((-1.0, 1.0, 2.0, inf), -inf),
    ((-2.0, 1.0, 2.0, -inf), inf),
    ((1.0, 1.0, 2.0, nan), nan),
]
for arguments, value in ends:
    add(arguments, abs(value) if math.isfinite(value) and value != 0 else 1.0, value)
with open("gauss.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Mathieu functions

mpmath has no Mathieu functions, so the script below finds them at 60 digits itself, by
methods that need no care about rounding at that precision: the characteristic value by
bisection on the count of eigenvalues of the tridiagonal matrix the Fourier coefficients
satisfy (DLMF 28.4.5 to 28.4.8), with 60 rows past the ones that matter, and the coefficients
by a twisted factorization of that matrix less the value. Each is scaled and signed as SciPy
and DLMF 28.2.29 say. In `mathieu.txt` each case holds, before the reference value, the size its
error is measured against: for `mathieu_a` and `mathieu_b` the larger of |a| and |q ∂a/∂q|, and
for the functions and their derivatives (`_d`) the largest of |f|, |q ∂f/∂q|, |x ∂f/∂x| and the
sum of the sizes of the Fourier terms. The script recorded the cases with mpmath 1.4.1; run it
from this directory with `python3 -I`:

```python
import math
import random

import mpmath
from mpmath import mpf


def family(m, cosine):
    if cosine:
        return 'ce_odd' if m % 2 else 'ce_even'
    return 'se_odd' if m % 2 else 'se_even'


def rows(fam, q, n):
    start = {'ce_even': 0, 'ce_odd': 1, 'se_odd': 1, 'se_even': 2}[fam]
    orders = [start + 2 * i for i in range(n)]
    diag = [mpf(k) ** 2 for k in orders]
    if fam == 'ce_odd':
        diag[0] = 1 + q
    if fam == 'se_odd':
        diag[0] = 1 - q
    off = [q] * (n - 1)
    if fam == 'ce_even':
        off[0] = q * mpmath.sqrt(2)
    return orders, diag, off


def count(diag, off, x):
    c = 0
    p = mpf(1)
    for i in range(len(diag)):
        s = diag[i] - x - (off[i - 1] ** 2 / p if i else 0)
        if s == 0:
            s = mpf(-10) ** -200
        if s < 0:
            c += 1
        p = s
    return c


def solve(m, q, cosine):
    """Characteristic value and normalised coefficients at the working precision."""
    q = mpf(q)
    fam = family(m, cosine)
    index = m // 2 if cosine else (m - 1) // 2
    n = int(m / 2 + 3 * mpmath.sqrt(abs(q))) + 60
    orders, diag, off = rows(fam, q, n)
    low, high = -3 * abs(q) - 2, mpf(orders[-1]) ** 2 + 3 * abs(q) + 2
    for _ in range(mpmath.mp.prec + 40):
        mid = (low + high) / 2
        if count(diag, off, mid) > index:
            high = mid
        else:
            low = mid
    a = (low + high) / 2
    # The eigenvector by a twisted factorization of the matrix less a: pivots from the top and
    # from the bottom, set to 1 where they meet with the smallest residue and carried outward,
    # each step dividing by a pivot that is not small.
    up = [mpf(0)] * n
    down = [mpf(0)] * n
    for i in range(n):
        up[i] = diag[i] - a - (off[i - 1] ** 2 / up[i - 1] if i else 0)
    for i in range(n - 1, -1, -1):
        down[i] = diag[i] - a - (off[i] ** 2 / down[i + 1] if i < n - 1 else 0)
    twist = min(range(n), key=lambda i: abs(up[i] + down[i] - diag[i] + a))
    v = [mpf(0)] * n
    v[twist] = mpf(1)
    for i in range(twist - 1, -1, -1):
        v[i] = -off[i] * v[i + 1] / up[i]
    for i in range(twist + 1, n):
        v[i] = -off[i - 1] * v[i - 1] / down[i]
    norm = mpmath.sqrt(sum(t * t for t in v))
    v = [t / norm for t in v]
    sign = sum(v) if cosine else sum(k * t for k, t in zip(orders, v))
    if sign < 0:
        v = [-t for t in v]
    if fam == 'ce_even':
        v[0] /= mpmath.sqrt(2)
    return a, orders, v


def function(m, q, x_degrees, cosine):
    a, orders, v = solve(m, q, cosine)
    x = mpmath.mpf(x_degrees) * mpmath.pi / 180
    if cosine:
        f = sum(t * mpmath.cos(k * x) for k, t in zip(orders, v))
        d = sum(-k * t * mpmath.sin(k * x) for k, t in zip(orders, v))
    else:
        f = sum(t * mpmath.sin(k * x) for k, t in zip(orders, v))
        d = sum(k * t * mpmath.cos(k * x) for k, t in zip(orders, v))
    return f, d


def term_sizes(m, q, cosine):
    """Σ |A_k| and Σ k |A_k|, the sizes of the terms of the value and of the derivative."""
    _, orders, v = solve(m, q, cosine)
    return sum(abs(t) for t in v), sum(k * abs(t) for k, t in zip(orders, v))

random.seed(3181416)
lines = []
STEP = mpmath.mpf(10) ** -25


def characteristic(m, q, cosine):
    return solve(m, q, cosine)[0]


def record_value(name, m, q, cosine):
    # Measured against the larger of |a| and |q ∂a/∂q|, the error a rounding of q alone makes.
    with mpmath.workdps(60):
        pq = mpmath.mpf(q)
        value = characteristic(m, pq, cosine)
        up = characteristic(m, pq + STEP, cosine)
        down = characteristic(m, pq - STEP, cosine)
        along_q = (up - down) / (2 * STEP)
        size = max(abs(value), abs(pq * along_q))
    lines.append(f"{name} {float(m)!r} {q!r} {float(size)!r} {float(value)!r}")


def record_function(name, m, q, x, cosine):
    # Measured against the largest of |f|, |q ∂f/∂q|, |x ∂f/∂x| and the sum of the sizes of the
    # Fourier terms, for the value and for the derivative alike: where the function is
    # exponentially small, its terms cancel, and a sum keeps the precision of its largest term.
    with mpmath.workdps(60):
        pq, px = mpmath.mpf(q), mpmath.mpf(x)
        f, d = function(m, pq, px, cosine)
        f_up, d_up = function(m, pq + STEP, px, cosine)
        f_down, d_down = function(m, pq - STEP, px, cosine)
        _, d_right = function(m, pq, px + STEP, cosine)
        _, d_left = function(m, pq, px - STEP, cosine)
        radians = px * mpmath.pi / 180
        second = (d_right - d_left) / (2 * STEP) * 180 / mpmath.pi
        terms, slope_terms = term_sizes(m, pq, cosine)
        sizes = [
            max(abs(f), abs(pq * (f_up - f_down) / (2 * STEP)), abs(radians * d), terms),
            max(abs(d), abs(pq * (d_up - d_down) / (2 * STEP)), abs(radians * second),
                slope_terms),
        ]
    for suffix, value, size in (("", f, sizes[0]), ("_d", d, sizes[1])):
        lines.append(f"{name}{suffix} {float(m)!r} {q!r} {x!r} {float(size)!r} {float(value)!r}")


for _ in range(220):
    # Orders mostly small, some to 40; q of either sign, near 0 and out to 1,000 and beyond m²,
    # where the functions gather about one point; x across two periods in degrees.
    m = random.choice([random.randint(0, 5), random.randint(0, 15), random.randint(0, 40)])
    q = random.choice([
        random.uniform(-5.0, 5.0),
        math.copysign(10 ** random.uniform(-4.0, 3.0), random.uniform(-1, 1)),
        random.uniform(-200.0, 200.0),
    ])
    x = random.choice([
        random.uniform(-360.0, 360.0),
        random.uniform(0.0, 180.0),
        float(random.choice([0, 45, 90, 180, -90])),
    ])
    record_value("mathieu_a", m, q, True)
    record_function("mathieu_cem", m, q, x, True)
    if m >= 1:
        record_value("mathieu_b", m, q, False)
        record_function("mathieu_sem", m, q, x, False)
# The ends: an order that is not a whole number of the kind, or NaN, gives NaN.
nan = math.nan
for name, args in [("mathieu_a", (-1.0, 1.0)), ("mathieu_a", (1.5, 1.0)),
                   ("mathieu_b", (0.0, 1.0)), ("mathieu_a", (2.0, nan)),
                   ("mathieu_b", (nan, 1.0))]:
    lines.append(f"{name} {args[0]!r} {args[1]!r} 1.0 nan")
for name, args in [("mathieu_cem", (1.5, 1.0, 10.0)), ("mathieu_sem", (0.0, 1.0, 10.0)),
                   ("mathieu_cem", (1.0, 1.0, math.inf)), ("mathieu_sem", (1.0, nan, 10.0))]:
    for suffix in ("", "_d"):
        lines.append(f"{name}{suffix} {args[0]!r} {args[1]!r} {args[2]!r} 1.0 nan")
with open("mathieu.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Parabolic cylinder functions

In `parabolic.txt` each case holds, before the reference value, the size its error is measured
against, as in `hypergeometric.txt`: the largest of |f|, |v ∂f/∂v| and |x ∂f/∂x|, by central
differences with a step of 10⁻²⁵ at 60 digits. `pbdv_d` and `pbvv_d` are the derivatives, the
second place of each tuple. The script below recorded the cases with its own seed, with
mpmath 1.4.1's `pcfd` and `pcfv` at 60 digits; run it from this directory with `python3 -I`:

```python
import math
import random

import mpmath

random.seed(318141)
lines = []
STEP = mpmath.mpf(10) ** -25
HALF = mpmath.mpf(1) / 2


def d(v, x):
    return mpmath.pcfd(v, x)


def v_(v, x):
    # SciPy's V_v(x) is DLMF's V(a, x) with a = −v − ½.
    return mpmath.pcfv(-v - HALF, x)


FUNCTIONS = [
    ("pbdv", d),
    ("pbdv_d", lambda v, x: x / 2 * d(v, x) - d(v + 1, x)),
    ("pbvv", v_),
    ("pbvv_d", lambda v, x: v_(v - 1, x) - x / 2 * v_(v, x)),
]


def record(name, f, v, x):
    # Each case is measured against the largest of |f|, |v ∂f/∂v| and |x ∂f/∂x|, the error a
    # rounding of the arguments alone would make, by central differences at 60 digits.
    with mpmath.workdps(60):
        pv, px = mpmath.mpf(v), mpmath.mpf(x)
        value = f(pv, px)
        if not 1e-300 < abs(value) < 1e300:
            return
        along_v = (f(pv + STEP, px) - f(pv - STEP, px)) / (2 * STEP)
        along_x = (f(pv, px + STEP) - f(pv, px - STEP)) / (2 * STEP)
        size = max(abs(value), abs(pv * along_v), abs(px * along_x))
        if size > 1e300:
            return
    lines.append(f"{name} {v!r} {x!r} {float(size)!r} {float(value)!r}")


for _ in range(450):
    # Orders uniform, whole, near 0 and out to ±60; arguments near 0, across the turning
    # points and out to ±60, where e^(x²/4) passes 1e300.
    v = random.choice([
        random.uniform(-10.0, 10.0),
        random.uniform(-60.0, 60.0),
        float(random.randint(-15, 15)),
        random.uniform(-3.0, 3.0),
    ])
    x = random.choice([
        random.uniform(-6.0, 6.0),
        random.uniform(-35.0, 35.0),
        random.uniform(-60.0, 60.0),
        math.copysign(10 ** random.uniform(-4.0, 1.0), random.uniform(-1, 1)),
    ])
    for name, f in FUNCTIONS:
        record(name, f, v, x)
# The ends: the limit, where SciPy gives NaN.
inf = math.inf
nan = math.nan
ends = [
    ("pbdv", (2.5, inf), 0.0),
    ("pbdv_d", (2.5, inf), 0.0),
    ("pbdv", (2.0, -inf), 0.0),
    ("pbdv", (2.5, -inf), -inf),
    ("pbdv_d", (2.5, -inf), inf),
    ("pbdv", (-1.3, -inf), inf),
    ("pbdv", (nan, 1.0), nan),
    ("pbdv", (inf, 1.0), nan),
    ("pbvv", (2.5, inf), inf),
    ("pbvv_d", (2.5, inf), inf),
    ("pbvv", (2.0, -inf), -inf),
    ("pbvv_d", (2.0, -inf), inf),
    ("pbvv", (2.5, -inf), 0.0),
    ("pbvv", (1.0, -inf), inf),
    ("pbvv", (1.0, nan), nan),
    ("pbvv", (-inf, 1.0), nan),
]
for name, (v, x), value in ends:
    size = abs(value) if math.isfinite(value) and value != 0 else 1.0
    lines.append(f"{name} {v!r} {x!r} {size!r} {value!r}")
with open("parabolic.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Weber's function

In `weber.txt` the cases of `pbwa` hold, before the reference value, the size its error is
measured against, the largest of |f|, |a ∂f/∂a| and |x ∂f/∂x|, as in `parabolic.txt`.
`pbwa_d` is the derivative, by central differences of mpmath's `pcfw` with a step of 10⁻²⁵ at
60 digits. The script below recorded the cases with mpmath 1.4.1; run it from this directory
with `python3 -I`:

```python
import math
import random

import mpmath

random.seed(3181415)
lines = []
STEP = mpmath.mpf(10) ** -25


def w(a, x):
    return mpmath.pcfw(a, x)


def slope(a, x):
    return (w(a, x + STEP) - w(a, x - STEP)) / (2 * STEP)


FUNCTIONS = [("pbwa", w), ("pbwa_d", slope)]


def record(name, f, a, x):
    # Each case is measured against the largest of |f|, |a ∂f/∂a| and |x ∂f/∂x|, the error a
    # rounding of the arguments alone would make, by central differences at 60 digits.
    with mpmath.workdps(60):
        pa, px = mpmath.mpf(a), mpmath.mpf(x)
        value = f(pa, px)
        along_a = (f(pa + STEP, px) - f(pa - STEP, px)) / (2 * STEP)
        along_x = (f(pa, px + STEP) - f(pa, px - STEP)) / (2 * STEP)
        size = max(abs(value), abs(pa * along_a), abs(px * along_x))
    lines.append(f"{name} {a!r} {x!r} {float(size)!r} {float(value)!r}")


for _ in range(300):
    # Parameters across the domain, whole and near 0, and its edges; arguments across it,
    # through the turning points at ±2√a, near 0 and at its edges.
    a = random.choice([
        random.uniform(-5.0, 5.0),
        float(random.randint(-5, 5)),
        random.uniform(-0.5, 0.5),
        random.choice([-5.0, 5.0]),
    ])
    x = random.choice([
        random.uniform(-10.0, 10.0),
        random.uniform(-5.0, 5.0),
        math.copysign(10 ** random.uniform(-4.0, 0.0), random.uniform(-1, 1)),
        random.choice([-10.0, 10.0]),
    ])
    for name, f in FUNCTIONS:
        record(name, f, a, x)
# Outside the domain, |a| above 5 or |x| above 10, and at NaN, the answer is NaN.
inf = math.inf
nan = math.nan
for name in ("pbwa", "pbwa_d"):
    for a, x in [(5.5, 1.0), (-6.0, 1.0), (1.0, 10.5), (1.0, -inf), (inf, 0.0), (nan, 1.0),
                 (1.0, nan)]:
        lines.append(f"{name} {a!r} {x!r} 1.0 nan")
with open("weber.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```

## Fitted coefficients

Some functions are a ratio of fitted polynomials rather than a series (roadmap 3.18.17): one
rational function costs a dozen multiplications, where a series or an iteration costs many
more. This script, run with mpmath 1.4, prints the coefficients of each, lowest power first,
exactly as the library holds them: `_centre`, `_near_tail` and `_far_tail` in
`special_ndtri.byn`, `_k_fitted` and `_e_fitted` in `special_elliptic.byn`, the four
`_hankel_` functions in `special_bessel_jy.byn` and `_erfcx_far` in `special_error.byn`. The
tests in `error_functions.txt`, `elliptic_functions.txt` and `bessel_functions.txt` check the
result.

```python

import mpmath as mp


def fit(function, points, variable, numerator, denominator, steps):
    """Coefficients of P/Q, Q(0) = 1, fitting `function` at `points` in relative error:
    linear least squares on P − fQ, reweighted by 1/|Q| `steps` times (Loeb's method)."""
    weights = [mp.mpf(1)] * len(points)
    for _ in range(steps):
        rows, right = [], []
        for u, w0 in zip(points, weights):
            z, v = variable(u), function(u)
            w = 1 / (abs(v) * w0)
            rows.append([w * z**k for k in range(numerator + 1)]
                        + [-w * v * z**k for k in range(1, denominator + 1)])
            right.append(w * v)
        solution, _ = mp.qr_solve(mp.matrix(rows), mp.matrix(right))
        top = [solution[k] for k in range(numerator + 1)]
        bottom = [mp.mpf(1)] + [solution[numerator + k] for k in range(1, denominator + 1)]
        weights = [abs(mp.polyval(bottom[::-1], variable(u))) for u in points]
    return [float(c) for c in top], [float(c) for c in bottom]


def chebyshev(low, high, n):
    """`n` Chebyshev points on [low, high], which spread the error evenly."""
    return [(low + high) / 2 + (high - low) / 2 * mp.cos(mp.pi * (i + 0.5) / n) for i in range(n)]


def quantile(p):
    """The normal quantile Φ⁻¹(p), at 420 digits so 1 − 2p keeps the smallest `p`."""
    with mp.workdps(420):
        value = -mp.sqrt(2) * mp.erfinv(1 - 2 * mp.mpf(p))
    return +value


mp.mp.dps = 90
def centre(s):
    """x / q at s = 0.180625 − q², for p = ½ + q."""
    q = mp.sqrt(mp.mpf(0.180625) - s)
    return quantile(mp.mpf(0.5) + q) / q
def tail(t):
    """−x at t = √(−log p)."""
    return -quantile(mp.exp(-t * t))
print("_centre", fit(centre, chebyshev(0, mp.mpf(0.425) ** 2, 400), lambda s: s, 7, 7, 8))
near = chebyshev(mp.mpf(1.6094), 5, 400)
print("_near_tail", fit(tail, near, lambda t: t - mp.mpf(1.6), 8, 8, 8))
print("_far_tail", fit(tail, chebyshev(5, mp.mpf(27.3), 400), lambda t: t - 5, 7, 7, 8))


def fit_logarithmic(function, points, degree):
    """Coefficients of P and Q, polynomials of `degree`, fitting `function`(x) as
    P(x) − log(x) Q(x) at `points` in relative error, by linear least squares."""
    rows, right = [], []
    for x in points:
        v = function(x)
        w = 1 / abs(v)
        rows.append([w * x**k for k in range(degree + 1)]
                    + [-w * mp.log(x) * x**k for k in range(degree + 1)])
        right.append(w * v)
    solution, _ = mp.qr_solve(mp.matrix(rows), mp.matrix(right))
    return ([float(solution[k]) for k in range(degree + 1)],
            [float(solution[degree + 1 + k]) for k in range(degree + 1)])


mp.mp.dps = 80
points = [(1 - mp.cos(mp.pi * (i + 0.5) / 600)) / 2 for i in range(600)]
points += [mp.mpf(10) ** (-k) for k in range(2, 40, 2)]
print("_k_fitted", fit_logarithmic(lambda x: mp.ellipk(1 - x), points, 9))
print("_e_fitted", fit_logarithmic(lambda x: mp.ellipe(1 - x), points, 10))


def hankel(order, x):
    """P and x Q of Hankel's expansion of Jₙ and Yₙ, from mpmath's J and Y."""
    chi = x - (2 * order + 1) * mp.pi / 4
    j, y, s = mp.besselj(order, x), mp.bessely(order, x), mp.sqrt(mp.pi * x / 2)
    return s * (j * mp.cos(chi) + y * mp.sin(chi)), s * (y * mp.cos(chi) - j * mp.sin(chi)) * x


mp.mp.dps = 60
far = chebyshev(0, mp.mpf(1) / 25, 200)
for order in (0, 1):
    p = lambda z: hankel(order, 1 / mp.sqrt(z))[0] if z > 0 else mp.mpf(1)
    q = lambda z: hankel(order, 1 / mp.sqrt(z))[1] if z > 0 else mp.mpf(4 * order * order - 1) / 8
    print(f"_hankel_p{order}", fit(p, far, lambda z: z, 7, 7, 6))
    print(f"_hankel_q{order}", fit(q, far, lambda z: z, 7, 7, 6))


def scaled_erfc_rest(z):
    """(x√π exp(x²) erfc(x) − 1) / z at z = 1/x², from mpmath's U(½, ½, x²), which stays
    accurate where its `erfc` of a large argument does not."""
    if z == 0:
        return mp.mpf(-0.5)
    return (mp.hyperu(0.5, 0.5, 1 / z) / mp.sqrt(z) - 1) / z


mp.mp.dps = 60
print("_erfcx_far", fit(scaled_erfc_rest, chebyshev(0, mp.mpf(1) / 9, 300), lambda z: z, 6, 6, 8))
```
