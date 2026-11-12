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

Every `linalg` function except `eig` and `eigvals` is [generic](types.md#generics) over `T: Float`,
so it takes `f64` or `f32` matrices and computes in that type: an `f32` matrix gives an `f32`
result, at half the memory and with about seven significant digits, as NumPy's `float32` does.
Complex matrices are not built yet (idea I.57). The tests compare `f32` results to NumPy's to 4
significant digits.

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
def solve[T: Float](a: Array[T, 2], b: Array[T, 1]) -> Raises[Array[T, 1], LinAlgError]: ...
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

Differences from NumPy: `b` is one vector, not a matrix of several right-hand sides (roadmap
3.10.3), and `a` holds `f64` or `f32`, not complex numbers. NumPy converts an integer matrix to
`float64`; here it is a type error, since every conversion is a call.

## `linalg.lu_factor` and `linalg.lu_solve`

```python
def lu_factor[T: Float](a: Array[T, 2]) -> Raises[tuple[Array[T, 2], Array[int, 1]], LinAlgError]: ...
def lu_solve[T: Float](lu: Array[T, 2], piv: Array[int, 1], b: Array[T, 1]) -> Array[T, 1]: ...
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
def det[T: Float](a: Array[T, 2]) -> T: ...
def inv[T: Float](a: Array[T, 2]) -> Raises[Array[T, 2], LinAlgError]: ...
```

`det(a)` is the determinant: the product of `U`'s diagonal, negated for each row swap. A singular
matrix gives `0.0` and an empty one `1.0`, as in NumPy. `inv(a)` is the inverse, one `lu_solve`
per column of the identity, and raises `LinAlgError("Singular matrix")` as `solve` does. Both
cost about n³ operations and panic when `a` is not square.

To apply an inverse, call `solve(a, b)` rather than `inv(a) @ b`: it does a third of the work
and rounds less.

## `linalg.cholesky`

```python
def cholesky[T: Float](a: Array[T, 2]) -> Raises[Array[T, 2], LinAlgError]: ...
```

The lower triangular matrix `l` with `l @ lᵀ == a`, for a symmetric positive definite `a`, such
as a covariance or a stiffness matrix. It reads only `a`'s lower triangle, as NumPy does, and
costs about n³/3 multiply-adds, half of `lu_factor`'s. Raises
`LinAlgError("Matrix is not positive definite")` when `a` is not; panics when `a` is not square.

## `linalg.qr`

```python
def qr[T: Float](a: Array[T, 2]) -> tuple[Array[T, 2], Array[T, 2]]: ...
```

`q, r = qr(a)` factors an m × n matrix as `q @ r`, NumPy's default `mode="reduced"`: `q` is
m × k with orthonormal columns and `r` is k × n and upper triangular, for k the smaller of m and
n. It uses Householder reflections with LAPACK's signs, so `q` and `r` match NumPy's entry by
entry, not only up to sign. It never fails. Cost: about 4mn² - 4n³/3 multiply-adds.

QR is the stable way to fit least squares: for a tall `a`, the `x` that minimizes the length of
`a @ x - b` solves `r @ x == qᵀ b`, with no squaring of `a`'s condition number.

Differences from NumPy: only the default mode; `mode="complete"`, `"r"` and `"raw"` are not
built.

## `linalg.eigh` and `linalg.eigvalsh`

```python
def eigh[T: Float](a: Array[T, 2]) -> Raises[tuple[Array[T, 1], Array[T, 2]], LinAlgError]: ...
def eigvalsh[T: Float](a: Array[T, 2]) -> Raises[Array[T, 1], LinAlgError]: ...
```

`w, v = eigh(a)` gives the eigenvalues of a symmetric matrix in ascending order and, as the
columns of `v`, their unit eigenvectors: `a @ v[:, i]` is `w[i] * v[:, i]`. `eigvalsh(a)` gives
`w` alone. Both read only `a`'s lower triangle, as NumPy does, so a matrix that rounding made
slightly unsymmetric is still treated as symmetric. Use them for normal modes, the inertia
tensor, a Hamiltonian, or a covariance matrix's principal axes.

The algorithm reduces `a` to a tridiagonal matrix with Householder reflections, then finds its
eigenvalues by the implicit QL iteration, EISPACK's `tred2` and `tql2`: about 9n³ multiply-adds,
0.15 s for n = 400 on one core. An eigenvector's sign is not part of the answer, and it can
differ from NumPy's. Raises `LinAlgError("Eigenvalues did not converge")` if the iteration does
not converge within 30 steps for an eigenvalue, as LAPACK does; panics when `a` is not square.

Differences from NumPy: no `UPLO="U"` argument; `eigvalsh` costs as much as `eigh`.

## `linalg.svd` and `linalg.svdvals`

```python
def svd[T: Float](a: Array[T, 2], full_matrices: bool = True) -> Raises[tuple[Array[T, 2], Array[T, 1], Array[T, 2]], LinAlgError]: ...
def svdvals[T: Float](a: Array[T, 2]) -> Raises[Array[T, 1], LinAlgError]: ...
```

`u, s, vh = svd(a)` is the singular value decomposition of an m × n matrix: `a` is
`u @ diag(s) @ vh`, where `s` holds the k = min(m, n) singular values in descending order and the
columns of `u` and the rows of `vh` are orthonormal. `u` is m × m and `vh` is n × n, or m × k and
k × n with `full_matrices=False`, NumPy's reduced form. `svdvals(a)` gives `s` alone. Use them for
a matrix's rank and condition number (the ratio of the largest singular value to the smallest),
principal component analysis, and least squares on a matrix that may be rank deficient.

The algorithm is one-sided Jacobi (Hestenes): it rotates pairs of columns until every pair is
orthogonal to working precision. Each sweep over the pairs costs about 3mn² multiply-adds and 6 to
10 sweeps are typical, about 0.3 s for a 300 × 300 matrix on one core, ten times NumPy's time.
Like LAPACK's, its error is small relative to the largest singular value, so a singular value
near 10⁻¹⁰ of the largest keeps only about six digits. A singular vector's sign is not
part of the answer, and it can differ from NumPy's. Raises `LinAlgError("SVD did not converge")`
after 60 sweeps, as NumPy raises the same message.

Differences from NumPy: no `compute_uv` argument, so call `svdvals` for the values alone;
`svdvals` costs about as much as `svd`, where NumPy's skips the vectors.

## `linalg.eig` and `linalg.eigvals`

```python
def eig(a: Array[f64, 2]) -> Raises[tuple[Array[complex128, 1], Array[complex128, 2]], LinAlgError]: ...
def eigvals(a: Array[f64, 2]) -> Raises[Array[complex128, 1], LinAlgError]: ...
```

`w, v = eig(a)` gives the eigenvalues of a square matrix that need not be symmetric, and as the
columns of `v` their eigenvectors: `a @ v[:, i]` is `w[i] * v[:, i]`. Both are `complex128`,
since a real matrix can have complex eigenvalues; they come in conjugate pairs, the one with
positive imaginary part first. Each eigenvector has length 1 and its largest entry real, as
LAPACK's `geev` returns them. `eigvals(a)` gives `w` alone. Use them for the stability of a
linear system, a Markov chain's stationary distribution, or the roots of a polynomial, which are
the eigenvalues of its companion matrix. For a symmetric matrix call [`eigh`](#linalgeigh-and-linalgeigvalsh),
which is faster and returns real values in order.

```python
from linalg import LinAlgError, eigvals, svdvals


def main():
    spring = Array.zeros((2, 2))  # x'' = -4x as a first-order system
    spring[0, 1] = 1.0
    spring[1, 0] = -4.0
    a = Array.zeros((3, 2))
    a[0, 0] = 3.0
    a[1, 1] = 2.0
    a[2, 0] = 4.0
    try:
        print(eigvals(spring))
        print(svdvals(a))
    except LinAlgError as error:
        print("failed:", error.message)
```

prints

```text
[0.+2.j 0.-2.j]
[5. 2.]
```

The algorithm balances `a` by powers of two, so that rows and columns of very different sizes do
not lose digits, reduces it to Hessenberg form with Householder reflections, then runs the shifted
double QR iteration and finds the eigenvectors by back-substitution: EISPACK's `balanc`, `orthes`
and `hqr2`. It costs about 25n³ multiply-adds. Raises
`LinAlgError("Array must not contain infs or NaNs")` for such an entry, as NumPy does, and
`LinAlgError("Eigenvalues did not converge")` if the iteration does not converge within 100 steps
for an eigenvalue; panics when `a` is not square.

Accuracy: the eigenvalues match NumPy's on matrices that break simple eigenvalue codes, among them
Jordan blocks, a matrix with entries from 10⁻¹⁰ to 10¹⁰, two eigenvalues 10⁻¹² apart and the
companion matrix of Wilkinson's polynomial, whose roots it finds closer to the exact ones than
NumPy does. A defective matrix, one without a full set of eigenvectors, gives eigenvalues with
error near the square root of the rounding error, in NumPy as here.

Differences from NumPy: the order of the eigenvalues is the algorithm's, and can differ from
NumPy's; sort them when the order matters. `eig` and `eigvals` take `f64` matrices only: an `f32`
matrix, whose eigenvalues NumPy gives as `complex64`, is not built yet (idea I.57).

## `fft`

```python
def fft(x: Array[complex128, 1]) -> Array[complex128, 1]: ...
def ifft(x: Array[complex128, 1]) -> Array[complex128, 1]: ...
def rfft(x: Array[f64, 1]) -> Array[complex128, 1]: ...
def irfft(x: Array[complex128, 1], n: int | None = None) -> Array[f64, 1]: ...
def fftfreq(n: int, d: f64 = 1.0) -> Array[f64, 1]: ...
def rfftfreq(n: int, d: f64 = 1.0) -> Array[f64, 1]: ...
```

The module `fft` holds the discrete Fourier transform and its inverse, with NumPy's names and
its default scaling, `norm="backward"`:

- `fft(x)[k]` is the sum over `j` of `x[j] * exp(-2πi jk / n)`, not scaled.
- `ifft(x)` is the same sum with `+2πi`, divided by n, so `ifft(fft(x))` gives back `x` up to
  rounding.
- `rfft(x)` transforms real samples and keeps the first n // 2 + 1 frequencies, since the rest
  are their complex conjugates. `irfft(x, n)` turns such a half spectrum back into n real
  samples; `n` defaults to 2 * (len(x) - 1), so give it for an odd number of samples.
- `fftfreq(n, d)` and `rfftfreq(n, d)` give the frequency of each entry, in cycles per unit of
  `d`, the spacing of the samples.

```python
from math import cos, pi
from fft import rfft, rfftfreq


def main():
    n = 64
    samples = Array.zeros(n)
    for j in range(n):
        samples[j] = cos(2.0 * pi * 5.0 * j / n)  # 5 cycles over the window
    spectrum = rfft(samples)
    frequencies = rfftfreq(n, 1.0 / n)
    peak = 0
    for k in range(len(spectrum)):
        if abs(spectrum[k]) > abs(spectrum[peak]):
            peak = k
    print(frequencies[peak], round(abs(spectrum[peak]), 6))
```

prints

```text
5.0 32.0
```

Any length works. A power of two uses the radix-2 algorithm, 0.08 s for 2²⁰ points on one core,
about twice NumPy's time. Any other length uses Bluestein's algorithm, three power-of-two
transforms of at least twice the length, so it costs about six times as much. The twiddle factors
come from `cos` and `sin` for each angle, never from repeated multiplication, so the error stays
within a few units in the last place of the largest entry, as NumPy's does.

Differences from NumPy: `fft` and `ifft` take a `complex128` array, so real samples go through
`rfft`, or into a complex array first; there is no `n` to pad or cut the input, no `axis` and no
`norm`; an empty input panics, where NumPy raises `ValueError`.

## `fft.fft2` and the other two-dimensional transforms

```python
def fft2(x: Array[complex128, 2]) -> Array[complex128, 2]: ...
def ifft2(x: Array[complex128, 2]) -> Array[complex128, 2]: ...
def rfft2(x: Array[f64, 2]) -> Array[complex128, 2]: ...
def irfft2(x: Array[complex128, 2], s: tuple[int, int] | None = None) -> Array[f64, 2]: ...
```

The transforms of a matrix, such as an image or a field on a grid, with NumPy's names and
scaling. `fft2(x)` is `fft` of each row, then of each column of the result; `ifft2` inverts it,
divided by the number of entries. `rfft2(x)` transforms a real m × n array and keeps n // 2 + 1
columns, the rest being complex conjugates; `irfft2(x, s)` turns it back into a real array of
shape `s`, which defaults to `x`'s rows and 2 * (columns - 1) columns, as for `irfft`. Each side can have any length, a power
of two being the cheapest; an m × n transform costs about m n (log m + log n) operations.

```python
from math import cos, pi
from fft import irfft2, rfft2


def main():
    n = 8
    image = Array.zeros((n, n))
    for i in range(n):
        for j in range(n):
            image[i, j] = cos(2.0 * pi * (2 * i + 3 * j) / n)  # a wave 2 rows, 3 columns per window
    spectrum = rfft2(image)
    peak = (0, 0)
    for k in range(spectrum.shape[0]):
        for m in range(spectrum.shape[1]):
            if abs(spectrum[k, m]) > abs(spectrum[peak[0], peak[1]]):
                peak = (k, m)
    print(peak, round(abs(spectrum[peak[0], peak[1]]), 6))
    back = irfft2(spectrum)
    print(round(abs(back[5, 1] - image[5, 1]), 12))
```

prints

```text
(2, 3) 32.0
0.0
```

Differences from NumPy: no `axes` or `norm` argument. `fftn` and `ifftn`,
for any number of dimensions, wait for functions that take an array of any rank (idea I.51).

