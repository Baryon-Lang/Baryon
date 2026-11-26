# Integration against SciPy

This benchmark times `integrate` against `scipy.integrate` 1.18.1 on six workloads (roadmap
3.19.10). [Integration](../../docs/guide/integration.md#6-how-fast) in the guide shows the results
and what they mean.

`main.byn` runs the workload its first argument names and prints a number that sums its results;
for `solve_ivp` it first prints the total calls of the right-hand side. The script below builds
it, times each workload as the fastest of three runs of the whole program, then times SciPy
running the same workload, and checks that both print the same numbers: the calls exactly, the
sums to 1e-9. Both run on one thread. The Baryon time includes starting the program, about 1 ms.

| Workload | What it runs |
| --- | --- |
| `quad` | 100,000 calls of `quad` on exp(−a x) sin²x over [0, 10], a from 0.1 to 2.1 |
| `quad_inf` | 10,000 calls of `quad` on exp(−a x²) cos x over [0, ∞), a from 0.5 to 2.5 |
| `simpson` | `simpson` of 10 million samples of sin x on [0, π], the samples made first |
| `trapezoid` | `trapezoid` of the same samples |
| `rk45` | 100 runs of `solve_ivp` on the van der Pol oscillator, μ = 2, to t = 100, RK45 at `rtol=1e-6`, `atol=1e-9` |
| `dop853` | The same 100 runs with DOP853 at `rtol=1e-10`, `atol=1e-12` |

Run it from this directory with NumPy and SciPy installed:

```python
import subprocess, time
import numpy as np
from scipy.integrate import quad, simpson, solve_ivp, trapezoid


def van_der_pol(t, y, mu):
    return np.array([y[1], mu * (1 - y[0] * y[0]) * y[1] - y[0]])


def scipy_side(name):
    """The numbers `main.byn` prints for the workload `name`, computed by SciPy."""
    if name == "quad":
        n = 100_000
        f = lambda x, a: np.exp(-a * x) * np.sin(x) * np.sin(x)
        return [sum(quad(f, 0.0, 10.0, args=(0.1 + 2.0 * ((i + 0.5) / n),))[0] for i in range(n))]
    if name == "quad_inf":
        n = 10_000
        f = lambda x, a: np.exp(-a * x * x) * np.cos(x)
        return [sum(quad(f, 0.0, np.inf, args=(0.5 + 2.0 * ((i + 0.5) / n),))[0] for i in range(n))]
    if name in ("simpson", "trapezoid"):
        n = 10_000_000
        y = np.sin(np.pi * np.arange(n) / (n - 1))
        rule = simpson if name == "simpson" else trapezoid
        return [rule(y, dx=np.pi / (n - 1))]
    method, rtol, atol = ("RK45", 1e-6, 1e-9) if name == "rk45" else ("DOP853", 1e-10, 1e-12)
    calls, total = 0, 0.0
    for k in range(100):
        r = solve_ivp(van_der_pol, (0.0, 100.0), np.array([1.0 + 0.01 * k, 0.0]), method,
                      args=(2.0,), rtol=rtol, atol=atol)
        calls, total = calls + r.nfev, total + r.y[0, -1]
    return [calls, total]


subprocess.run(["baryon", "build", "--release", ".", "integrate-bench"], check=True)
print("| Workload | Baryon (s) | SciPy (s) | SciPy / Baryon |")
print("| --- | --- | --- | --- |")
for name in ["quad", "quad_inf", "simpson", "trapezoid", "rk45", "dop853"]:
    ours, theirs = 1e9, 1e9
    for _ in range(3):
        start = time.perf_counter()
        run = subprocess.run(
            ["./integrate-bench", name], capture_output=True, text=True, check=True
        )
        ours = min(ours, time.perf_counter() - start)
    for _ in range(3):
        start = time.perf_counter()
        expected = scipy_side(name)
        theirs = min(theirs, time.perf_counter() - start)
    found = [float(v) for v in run.stdout.split()]
    for want, got in zip(expected, found):
        assert abs(want - got) <= 1e-9 * max(abs(want), 1.0), (name, want, got)
    print(f"| `{name}` | {ours:.3f} | {theirs:.3f} | {theirs / ours:.1f} |")
```
