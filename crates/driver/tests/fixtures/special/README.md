# Special-function reference values

`gamma_family.txt`, `error_functions.txt`, `bessel_functions.txt`, `incomplete_functions.txt`,
`distribution_functions.txt`, `helper_functions.txt`, `integral_functions.txt` and `lambertw.txt`
hold the cases the build tests check the `special` module against: those in
[`../../build/special.rs`](../../build/special.rs) its gamma family (roadmap 3.18.1), error
functions (3.18.3), Bessel functions of whole order (3.18.4), incomplete gamma and beta
functions (3.18.2) and distribution functions (3.18.12), and those in
[`../../build/special_everyday.rs`](../../build/special_everyday.rs) its everyday helpers,
exponential integrals, zeta and Spence's function, and Lambert W function (3.18.11). Each line is a
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
