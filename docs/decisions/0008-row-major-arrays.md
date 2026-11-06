# 0008. Arrays are row-major by default

Status: Proposed
Date: 2026-11-06
Roadmap: D.4

## Context

The design plan makes arrays column-major by default, as Fortran does, for compatibility with
BLAS, LAPACK and Fortran code. Against that:

- Fortran interop is not a goal, and CBLAS and LAPACKE accept row-major arrays directly.
- Python programmers write `for i in range(n): for j in range(m): a[i, j]`. Under column-major
  layout that inner loop strides through memory, which is the slow order. The language would look
  slow to exactly the people it is for.
- NumPy and Arrow, which the data-science half must exchange arrays with, are row-major.

## Decision

`Array` and `Fixed` are row-major by default: the last index varies fastest, as in C and NumPy. A
per-type attribute selects column-major layout for code that needs it.

## Consequences

- Loops written the natural Python way run at full speed.
- Arrays can be shared with NumPy and Arrow without copying.
- Calls into BLAS and LAPACK pass a row-major flag or transpose; both libraries support it at no
  cost.
- Code ported from Fortran must swap its loop order, or opt into column-major arrays.
