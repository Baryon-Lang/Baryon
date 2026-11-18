# Special functions against SciPy

This benchmark times `special` against `scipy.special` on 10 million arguments for each family
of functions (roadmap 3.18.15). [Special functions](../../docs/guide/special-functions.md#6-how-fast)
in the guide shows the results and what they mean.

`main.byn` calls the function its first argument names on 10 million evenly spaced arguments,
x = lo + (hi − lo)(i + ½)/n, and prints the sum of the results. The script below builds it,
times each function as the fastest of three runs of the whole program, then times SciPy applying
the same function to one NumPy array of the same arguments, and checks that both sums agree to
1e-6. Both run on one thread. The Baryon time includes starting the program, which is about 1 ms.

Run it from this directory with NumPy and SciPy installed:

```python
import subprocess, time
import numpy as np
import scipy.special as sc

# The name main.byn takes, the SciPy call, and the range of x.
CASES = [
    ("gamma", lambda x: sc.gamma(x), 0.1, 20.0),
    ("gammaln", lambda x: sc.gammaln(x), 0.1, 100.0),
    ("digamma", lambda x: sc.digamma(x), 0.1, 100.0),
    ("erf", lambda x: sc.erf(x), -5.0, 5.0),
    ("erfc", lambda x: sc.erfc(x), -5.0, 25.0),
    ("ndtri", lambda x: sc.ndtri(x), 0.0, 1.0),
    ("j0", lambda x: sc.j0(x), 0.0, 100.0),
    ("jv", lambda x: sc.jv(2.5, x), 0.0, 100.0),
    ("kv", lambda x: sc.kv(1.3, x), 0.01, 50.0),
    ("airy", lambda x: sc.airy(x)[0], -20.0, 20.0),
    ("gammainc", lambda x: sc.gammainc(3.7, x), 0.0, 20.0),
    ("betainc", lambda x: sc.betainc(2.5, 3.5, x), 0.0, 1.0),
    ("stdtr", lambda x: sc.stdtr(5.0, x), -10.0, 10.0),
    ("ellipk", lambda x: sc.ellipk(x), -10.0, 1.0),
    ("ellipe", lambda x: sc.ellipe(x), -10.0, 1.0),
    ("expi", lambda x: sc.expi(x), -50.0, 50.0),
    ("zeta", lambda x: sc.zeta(x), 1.01, 50.0),
    ("eval_legendre", lambda x: sc.eval_legendre(10, x), -1.0, 1.0),
    ("hyp1f1", lambda x: sc.hyp1f1(1.5, 2.5, x), -50.0, 50.0),
    ("hyp2f1", lambda x: sc.hyp2f1(0.5, 1.5, 2.5, x), -10.0, 0.99),
]
N = 10_000_000

subprocess.run(["baryon", "build", "--release", ".", "special-bench"], check=True)
print("| Function | Baryon (s) | SciPy (s) | Baryon / SciPy |")
print("| --- | --- | --- | --- |")
i = np.arange(N, dtype=np.float64)
for name, call, lo, hi in CASES:
    ours, theirs = 1e9, 1e9
    for _ in range(3):
        start = time.perf_counter()
        run = subprocess.run(["./special-bench", name], capture_output=True, text=True, check=True)
        ours = min(ours, time.perf_counter() - start)
    x = lo + (hi - lo) * ((i + 0.5) / N)
    for _ in range(3):
        start = time.perf_counter()
        y = call(x)
        theirs = min(theirs, time.perf_counter() - start)
    total = float(run.stdout)
    assert abs(total - np.sum(y)) <= 1e-6 * max(abs(total), 1.0), (name, total, np.sum(y))
    print(f"| `{name}` | {ours:.2f} | {theirs:.2f} | {ours / theirs:.1f} |")
```

Some sums differ in their last digits: NumPy adds in pairs and `main.byn` in order, and for
`erf` and `ndtri`, whose values cancel to a sum near 0, the check above is on 1e-6 of 1.
