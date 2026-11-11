# 0040. `linalg` and `fft` follow NumPy and are written in Baryon

Status: Proposed
Date: 2026-11-11
Roadmap: 3.10

## Context

Roadmap 3.10 adds linear algebra and fast Fourier transforms. Collins settled the split on
2026-11-10: matrix multiply, solve, LU, Cholesky, QR, symmetric eigenvalues and the FFT are
written natively, in Baryon and its runtime, and tested against NumPy and SciPy; LAPACK is used
underneath only for general eigenvalues and the singular value decomposition; and the system
BLAS, or Accelerate on macOS, is an opt-in for peak matrix-multiply speed. Users see only
Baryon. The design plan names the modules `linalg` and `fft`.

That leaves the questions every user of the modules meets, and that are hard to change once
programs depend on them:

1. What the functions are called and what they return.
2. What happens when a matrix cannot be solved or factored: a singular matrix in `solve`, a
   matrix that is not positive definite in `cholesky`.
3. Which element types they take.
4. What a result is tested against, and whether it is reproducible.
5. Where the code lives.
6. How LAPACK and the opt-in BLAS reach a program.

## Decision

### Names: NumPy's, then SciPy's

The modules are `linalg` and `fft`, and each function has NumPy's name, arguments and result:
`linalg.solve(a, b)`, `linalg.inv(a)`, `linalg.det(a)`, `linalg.cholesky(a)`, `linalg.qr(a)`,
`linalg.eigh(a)`, `linalg.eigvalsh(a)`, `linalg.eig(a)`, `linalg.svd(a)`, `linalg.norm(x)`;
`fft.fft(x)`, `fft.ifft(x)`, `fft.rfft(x)`, `fft.irfft(x)`, `fft.fftfreq(n, d)`. A function
NumPy lacks takes SciPy's name from `scipy.linalg`: `linalg.lu_factor(a)` and
`linalg.lu_solve(lu, piv, b)`. Porting `np.linalg.solve(a, b)` means writing
`linalg.solve(a, b)`. A function that returns several arrays returns a tuple of them, as NumPy
does: `q, r = linalg.qr(a)`, `w, v = linalg.eigh(a)`.

Where a function differs from NumPy, its reference entry says so in a "Differences from NumPy"
line. `linalg.lu_solve` takes the factors as two arguments, `lu` and `piv`, where SciPy takes the
tuple `(lu, piv)`.

### Failures: shapes panic, singular matrices raise `LinAlgError`

Shapes that do not fit, such as a matrix that is not square or a right-hand side of the wrong
length, are bugs, and panic, as mismatched shapes in whole-array arithmetic do. The message says
what does not fit, such as `solve needs b as long as a has rows`; it names the lengths once
`assert` can format its message, which today it prints as written.

A matrix the algorithm cannot handle is an expected failure: a stiffness matrix with a loose
node, a covariance matrix that rounding made indefinite. It raises `linalg.LinAlgError`, as
NumPy raises `numpy.linalg.LinAlgError`, with NumPy's message:

```python
class LinAlgError(Exception):
    """A matrix that a linear algebra function cannot solve or factor."""

    message: str  # NumPy's text, such as "Singular matrix"
```

So `solve` is `def solve(a: Array[f64, 2], b: Array[f64, 1]) -> Raises[Array[f64, 1],
LinAlgError]`, and a caller handles the error or lists it in its own `Raises`, as for
`ValueError` from `float(text)` ([decision 0037](0037-reading-numbers-from-text.md)). `solve`,
`inv` and `lu_factor` raise it when a pivot is exactly zero, as LAPACK's `getrf` reports and NumPy
raises; a matrix that is merely ill-conditioned gives a result, as in NumPy. `det` of a singular
matrix is `0.0`, not an error. `cholesky` raises it when a diagonal entry is not positive. `eigh`,
`eig` and `svd` raise it when the iteration does not converge.

### Element types: `f64` first

The first slices take `f64` matrices and vectors, and `fft` takes `f64` or `complex128` arrays and
returns `complex128`. `f32`, `complex64` and `complex128` matrices follow once library functions
can be generic over float types. NumPy converts integer arrays to `float64`; here an integer array
is a type error that suggests converting it, since every conversion is a call
([decision 0014](0014-number-conversions-are-calls.md)).

### Results: checked against NumPy, reproducible everywhere

Each function is tested against results recorded from NumPy and SciPy, to a tolerance stated in
its test and scaled by the matrix's condition number: the last bits differ, because LAPACK's
blocked algorithms add in their own order. Eigenvectors and singular vectors are compared up to
sign, which NumPy does not fix either. `fft` matches NumPy's pocketfft to within a few units in
the last place relative to the largest magnitude.

The native code fixes its order of operations, so a Baryon result has the same bits on every
machine and every run, as `sum` and `dot` do. The opt-in BLAS and the LAPACK functions give up that
guarantee, as NumPy does, and their reference entries say so.

### Code: Baryon library files

`linalg` and `fft` are standard-library files written in Baryon, `<std>/linalg.byn` and
`<std>/fft.byn`, shipped in the compiler as [decision 0038](0038-standard-library-in-baryon.md)
describes. They are ordinary Baryon: loops over arrays, compiled with the program, inlined and
vectorized like its own code. Where they are slower than Fortran, the gap is a gap in the compiler,
and goes to roadmap 3.1 to 3.3, not around them. The matrix product stays the operator `@`.

### LAPACK and BLAS: linked only when used

`eig` and `svd` call LAPACK's `dgeev` and `dgesdd`: the system's `liblapack` on Linux and the
Accelerate framework on macOS. The driver links LAPACK only into a program that calls one of them,
so every other program builds and runs with no LAPACK installed.

The opt-in BLAS is the build option `--blas`: `baryon build --blas` and `baryon run --blas` make
`@` of `f64` and `f32` matrices call `cblas_dgemm` and `cblas_sgemm` from OpenBLAS on Linux and
Accelerate on macOS. Without it, `@` is compiled code with a fixed order.

## Alternatives

- **Panic on a singular matrix.** Shorter to call: `x = linalg.solve(a, b)` with no `Raises`. But
  a singular matrix often comes from data, not from a bug, and a panic cannot be caught, so a
  sweep over many systems would die at the first bad one. NumPy users expect `LinAlgError`.
- **`numpy.linalg` as the module path.** Ports would change less, but Baryon is not NumPy, and the
  design plan's `linalg` and `fft` read as Baryon's own.
- **Everything in the Rust runtime.** Faster to write the first time, but it would hide the
  compiler's speed gaps behind hand-tuned Rust, and Collins asked for true Baryon.
- **LAPACK for everything.** Fastest to ship, but it is the wrapped C library Collins ruled out, it
  needs LAPACK installed for every program that solves a system, and its results vary by machine.

## Consequences

- Every function costs one Baryon file section and its tests, with no new compiler code, until a
  function needs something the language cannot express yet.
- A caller of `solve`, `inv`, `cholesky` and the decompositions writes `Raises[..., LinAlgError]` or
  a `try`, as for any fallible call.
- Programs that call `eig` or `svd` need LAPACK on Linux (`liblapack-dev`); macOS has Accelerate.
- `--blas` is the first build option after `--release`.
