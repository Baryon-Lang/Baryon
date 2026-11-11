# Linear algebra and FFT

[What builds today](status.md#linear-algebra-and-fft) says which parts of this chapter the
compiler builds into programs yet.

The standard modules `linalg` and `fft` solve linear systems, factor matrices and take Fourier
transforms, with NumPy's names and results ([decision 0040](../decisions/0040-linear-algebra-and-fft.md)).
A NumPy program ports by writing `linalg.solve(a, b)` where it wrote `np.linalg.solve(a, b)`.

Both modules are written in Baryon, in the standard library's source files
([decision 0038](../decisions/0038-standard-library-in-baryon.md)), and compile with the program
like its own functions: inlined, vectorized, and left out of a program that never calls them. They
fix their order of operations, so a result has the same bits on every machine and every run.
NumPy calls LAPACK, which adds in its own order, so the last bits of a result can differ from
NumPy's; the tests compare to 12 significant digits.

The matrix product is the operator [`@`](arrays.md#the-matrix-product), not a function.

## Errors

A shape that does not fit, such as a matrix that is not square, is a bug and
[panics](errors.md#panics). A matrix the algorithm cannot handle is an
[error](errors.md) the program can catch: `linalg.LinAlgError`, as NumPy's
`numpy.linalg.LinAlgError`.

```python
class LinAlgError(Exception):
    message: str  # NumPy's text, such as "Singular matrix"
```

A function that can raise it returns `Raises[..., LinAlgError]`, so its caller handles the error
with `try` or lists it in its own `Raises`.

## `linalg.solve`

```python
def solve(a: Array[f64, 2], b: Array[f64, 1]) -> Raises[Array[f64, 1], LinAlgError]: ...
```

The vector `x` with `a @ x == b`, for a square matrix `a`: Gaussian elimination with partial
pivoting, which at each step swaps up the row with the largest entry in the column. `a` and `b`
are left unchanged.

- Raises `LinAlgError("Singular matrix")` when a pivot is exactly zero, as NumPy does. A matrix
  that is nearly singular gives a result with large error and no exception, as in NumPy.
- Panics when `a` is not square, or `b`'s length is not `a`'s number of rows.
- Cost: about 2n³/3 multiply-adds, and a copy of `a` and `b`; about 0.05 s for n = 600 on one core.
  To solve many systems with one matrix, factor it once with [`lu_factor`](#linalglu_factor-and-linalglu_solve).

```python
import linalg
from linalg import LinAlgError


def currents(conductance: Array[f64, 2], injected: Array[f64, 1]) -> Raises[Array[f64, 1], LinAlgError]:
    return linalg.solve(conductance, injected)


def main():
    g = Array.zeros((3, 3))
    g[0, 0] = 2.0
    g[0, 1] = 1.0
    g[1, 0] = 1.0
    g[1, 1] = 3.0
    g[2, 0] = 1.0
    g[2, 2] = 4.0
    try:
        print(currents(g, Array([1.0, 2.0, 3.0])))
    except LinAlgError as error:
        print("no solution:", error.message)
```

prints

```text
[0.2 0.6 0.7]
```

Differences from NumPy: `b` is one vector, not a matrix of several right-hand sides, and `a`
holds `f64`s only; both come in later slices of roadmap 3.10. NumPy converts an integer matrix to
`float64`; here it is a type error, since every conversion is a call.

## `linalg.lu_factor` and `linalg.lu_solve`

```python
def lu_factor(a: Array[f64, 2]) -> Raises[tuple[Array[f64, 2], Array[int, 1]], LinAlgError]: ...
def lu_solve(lu: Array[f64, 2], piv: Array[int, 1], b: Array[f64, 1]) -> Array[f64, 1]: ...
```

NumPy has no LU factorization, so these are SciPy's `scipy.linalg.lu_factor` and `lu_solve`.
`lu_factor(a)` returns `(lu, piv)`, the factors `solve` computes: `lu` holds the lower triangle
`L`, whose diagonal of ones is not stored, below its diagonal, and the upper triangle `U` on and
above it; at step `k`, row `k` was swapped with row `piv[k]`. `lu_solve(lu, piv, b)` then solves
`a @ x == b` in about 2n² multiply-adds instead of 2n³/3, so a time-stepping loop with a fixed
matrix factors it once:

```python
from linalg import LinAlgError, lu_factor, lu_solve


def implicit_steps(system: Array[f64, 2], start: Array[f64, 1], steps: int) -> Raises[Array[f64, 1], LinAlgError]:
    lu, piv = lu_factor(system)
    u = start.copy()
    for _ in range(steps):
        u = lu_solve(lu, piv, u)
    return u
```

Differences from SciPy: `lu_solve` takes `lu` and `piv` as two arguments, where SciPy takes the
tuple `(lu, piv)`; `lu_factor` of a singular matrix raises `LinAlgError`, where SciPy warns and
returns factors with a zero on the diagonal.

## `linalg.det` and `linalg.inv`

```python
def det(a: Array[f64, 2]) -> f64: ...
def inv(a: Array[f64, 2]) -> Raises[Array[f64, 2], LinAlgError]: ...
```

`det(a)` is the determinant: the product of `U`'s diagonal, negated for each row swap. A singular
matrix gives `0.0` and an empty one `1.0`, as in NumPy. `inv(a)` is the inverse, one `lu_solve`
per column of the identity, and raises `LinAlgError("Singular matrix")` as `solve` does. Both
cost about n³ operations and panic when `a` is not square.

To apply an inverse, call `solve(a, b)` rather than `inv(a) @ b`: it does a third of the work
and rounds less.
