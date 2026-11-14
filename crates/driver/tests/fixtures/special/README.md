# Special-function reference values

`gamma_family.txt`, `error_functions.txt`, `bessel_functions.txt` and `incomplete_functions.txt`
hold the cases the build tests in [`../../build/special.rs`](../../build/special.rs) check the
`special` module's gamma family (roadmap 3.18.1), error functions (3.18.3), Bessel functions of
whole order (3.18.4) and incomplete gamma and beta functions (3.18.2) against. Each line is a
function's name, its arguments and the value the function should give, separated by spaces and
written as Python's `repr` writes a float:

```text
gamma -16.481439370601617 -3.8732193888774417e-14
polygamma 3 -8.996117952868211 26418492688.871017
```

Most values were computed with mpmath 1.4.1 at 50 digits and rounded to `f64`, at arguments drawn
at random, with a fixed seed, over each function's range. The last lines are the poles, zeros and
infinities, where the value is SciPy 1.18.1's, which the module gives exactly, except the
Bessel functions' limits at infinity, where SciPy gives NaN and the file holds the limit. The
incomplete beta function's values were computed at 80 digits, so a value far in a tail keeps its
own, and each inverse's value is the root mpmath finds of the forward function. Neither mpmath
nor SciPy is needed to run the tests.

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

Run this in this directory, with mpmath, NumPy and SciPy installed. It writes all three files;
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
edges = [
    ("gammainc", (1.0, 0.0)), ("gammainc", (0.0, 1.0)), ("gammainc", (1.0, inf)),
    ("gammainc", (-1.0, 1.0)), ("gammaincc", (1.0, 0.0)), ("gammaincc", (1.0, inf)),
    ("gammaincinv", (1.0, 0.0)), ("gammaincinv", (1.0, 1.0)), ("gammaincinv", (1.0, 2.0)),
    ("gammainccinv", (1.0, 0.0)), ("gammainccinv", (1.0, 1.0)),
    ("betainc", (1.0, 2.0, 0.0)), ("betainc", (1.0, 2.0, 1.0)), ("betainc", (1.0, 2.0, 1.5)),
    ("betaincc", (1.0, 2.0, 0.0)), ("betaincc", (1.0, 2.0, 1.0)),
    ("betaincinv", (1.0, 2.0, 0.0)), ("betaincinv", (1.0, 2.0, 1.0)),
]
for name, arguments in edges:
    value = float(getattr(scipy.special, name)(*arguments))
    lines.append(" ".join([name] + [repr(x) for x in arguments] + [repr(value)]))
with open("incomplete_functions.txt", "w") as out:
    out.write("\n".join(lines) + "\n")
```
