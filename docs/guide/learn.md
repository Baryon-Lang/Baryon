# Learn Baryon

Baryon is a compiled language that reads like Python and runs like Rust, C and Fortran. It is
built for physics, mathematics and data science: simulations, solvers and analyses that should be
quick to write and quick to run, on data of any size. Every Baryon file is valid Python syntax,
and `baryon` compiles it to machine code through LLVM.

This site teaches the language. Every program on its guide pages is built and run in CI exactly
as written, and every output shown is what the program printed, so the pages cannot drift from
the compiler. The compiler is a prototype: [What builds today](../reference/status.md) says
what it builds and what it still rejects.

## A first look

Here is a whole program. It finds the area under `sin(x)` from 0 to π, which is exactly 2, by
adding up thin strips, and shows the error shrinking as the strips get thinner:

File `sine/main.byn`:

```python
from math import pi, sin


def midpoint_sine(steps: i64) -> f64:
    """The area under sin(x) from 0 to pi, by the midpoint rule with `steps` strips."""
    width = pi / steps
    total = 0.0
    for i in range(steps):
        total += sin((i + 0.5) * width)
    return total * width


def main():
    for steps in [10, 100, 1000]:
        area = midpoint_sine(steps)
        print(f"{steps:4d} strips: {area:.8f}, error {area - 2.0:+.1e}")
```

```text
$ baryon run sine
  10 strips: 2.00824841, error +8.2e-03
 100 strips: 2.00008225, error +8.2e-05
1000 strips: 2.00000082, error +8.2e-07
```

Ten times as many strips cut the error a hundredfold, as the midpoint rule promises. A Python
programmer can read every line. What is new is small: the parameter and return types, `i64` for
a 64-bit integer and `f64` for a 64-bit float, which let the compiler keep the loop's numbers in
registers. Local variables need no types; they keep the type of their first value.

## Where to start

| If you are | Read |
| --- | --- |
| New to Baryon | The [beginner's guide](README.md): install `baryon`, run a first program, let the compiler catch a mistake, and simulate a mass on a spring, with tests |
| A Python programmer | [A tour of the language](language-tour.md), which teaches the language as Python with types and points out each of the four new ideas, then [Differences from Python](../reference/differences-from-python.md), every place the same code means something else, with the reason |
| A NumPy user | [Arrays and numbers](arrays-and-numbers.md): `Array`, parallel and vector loops, `math`, random numbers, `linalg`, `fft` and `stats`; then the [Arrays](../reference/arrays.md) reference |
| A pandas user | [A worked analysis with DataFrame](dataframes.md), one pandas analysis written again line for line, then [Data, files and plots](data-files-and-plots.md) |
| A SciPy user | [Special functions](special-functions.md), with a section on moving from SciPy; the reference pages on [integration](../reference/integration.md), [linear algebra](../reference/linear-algebra.md) and [statistics](../reference/statistics.md) |
| Setting up an editor | [Coding Baryon in VS Code](vscode.md) |

The guide pages build on each other, so read them in the order the sidebar lists them. The
[language reference](../reference/README.md) defines everything the guide leaves out, and the
[example programs](../examples/README.md) are larger programs that build and run today:
molecular dynamics, a heat equation, conjugate gradient, Runge-Kutta and Monte Carlo.

## How fast is it

Only measured numbers appear on this site, each beside how it was measured:

- Eight numeric kernels, from n-body gravity to an FFT, each written in both Baryon and
  Fortran, take between 0.78 and 1.51 times as long in Baryon as with `gfortran -O3`, and five
  of them take no longer. The
  [benchmark suite](../../bench/README.md#the-baryon-kernels-and-the-vectorizer-audit) has the
  table and the method.
- `DataFrame` against pandas and Polars on 10 million rows:
  [How fast is it](dataframes.md#how-fast-is-it). Some operations are faster than pandas and
  some slower, and the page says which.
- Special functions against SciPy: [How fast](special-functions.md#6-how-fast).

## How this site is made

Every page is a Markdown file in the repository's
[`docs/`](https://github.com/Baryon-Lang/Baryon/tree/main/docs) directory, and the edit button at
the top of each page opens its file on GitHub. A change to the language ships with its
documentation in the same pull request, so the site always describes the compiler on the `main`
branch. A page that is wrong or unclear is a bug: please open an
[issue](https://github.com/Baryon-Lang/Baryon/issues).
