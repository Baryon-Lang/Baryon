# 0054. The libraries adapted from SciPy are top-level modules named as SciPy's submodules

Status: Accepted
Date: 2026-11-13
Roadmap: 3.18, 3.19, 3.20, 3.21, 3.22, 3.23

## Context

Roadmap rows 3.18 to 3.23 adapt six parts of SciPy into native Baryon: special functions
(`scipy.special`), integration (`scipy.integrate`), optimization and root finding
(`scipy.optimize`), interpolation (`scipy.interpolate`), spatial search and image processing
(`scipy.spatial` and `scipy.ndimage`), and probability distributions (`scipy.stats`). Each is
written in Baryon, and inside each module the names are SciPy's wherever the behavior matches, so
a program ports line for line. AstroBaryon (4.2) is built on all six.

What the modules themselves are called is still open. Two rules bear on it:

- **No "py" in a Baryon name** (Collins, 2026-11-13). A Python library adapted into Baryon never
  carries "py" in its name; `pyplot` became `byplot` and `h5py` became `h5by`. `scipy` cannot be
  kept as it is.
- **The NumPy precedent.** Baryon already writes NumPy's submodules without the `numpy.` prefix:
  `linalg` and `fft` (decision 0040), and `stats` for NumPy's statistics functions (decision
  0042). Decision 0042 also says that 3.23's distributions join `stats` under SciPy's names.

A name is chosen once: every program imports it, and changing it later breaks them all.

## Options

### A. Top-level modules named as SciPy's submodules (chosen)

`special`, `integrate`, `optimize`, `interpolate`, `spatial` and `ndimage`, with the
distributions in the existing `stats`.

```python
from scipy.special import gamma, jv        # Python
from special import gamma, jv              # Baryon
```

- Matches what 0040 and 0042 did for NumPy: one rule for both libraries, drop the package
  prefix and keep the submodule's name.
- Shortest to write, and nothing new to learn: every name is a word SciPy users already read.
- `scipy.stats` and Baryon's `stats` meet in one module, as 0042 planned. SciPy's `stats` has
  no `median` or `percentile` of its own, so nothing in the two collides.
- Cost: six more names in the standard library's flat namespace. A program with its own
  `optimize.byn` or `spatial.byn` cannot import it alongside the standard module; the compiler
  says so and asks for the file to be renamed, as it already does for `stats.byn`.

### B. One `sciby` package, as `scipy` with the "by" of `byplot` and `h5by`

```python
from sciby.special import gamma, jv
```

- One more word per import, and the module path reads as SciPy's with one letter changed.
- Keeps the standard library's top level small and leaves `special`, `optimize` and the rest
  free for programs' own modules.
- Breaks the NumPy precedent: `linalg` is top-level but `sciby.linalg` would be the obvious
  home for SciPy's larger linear algebra, so the two libraries would follow different rules.
- Splits statistics: NumPy's functions in `stats`, SciPy's distributions in `sciby.stats`, two
  modules with the same last name.
- Needs `import sciby.special` to resolve through a standard-library package, which is idea
  I.56 and not built yet; `from sciby import special` works today, as `from compression import
  zstd` does.

### C. Descriptive English names

`specfun`, `quadrature`, `minimize` and the like.

- Every name differs from SciPy's, so every import must be looked up when porting. Rejected: it
  fails the line-for-line rule for no gain over A.

## Decision

**A** (Collins, 2026-11-13): top-level `special`, `integrate`, `optimize`, `interpolate`,
`spatial` and `ndimage`, with SciPy's distributions in the existing `stats`.

Inside each module:

1. **Names are SciPy's where behavior matches**: `gamma`, `gammaln`, `jv`, `hyp2f1`,
   `quad`, `solve_ivp`, `curve_fit`, `CubicSpline`, `KDTree` and the rest. Where a Baryon
   function behaves differently, its reference entry says how.
2. **Everything is written in Baryon** (decision 0038), never a wrapper around SciPy, Cephes,
   Boost or the C library's own `tgamma` and `erf`. Only the elementary functions the compiler
   already provides, such as `exp`, `log` and `sin`, are called.
3. **Results are IEEE `f64` and tested against recorded values.** Each test compares against
   values computed with mpmath to 30 digits and states its tolerance in units in the last
   place; SciPy's own value is recorded beside it. Neither is a dependency: the values are
   fixtures in the tests.
4. **Special functions follow SciPy at the edges**: a pole or an argument outside the domain
   gives NaN or infinity as SciPy's does, never a panic, since `gamma(-1.0)` in the middle of an
   array computation is data, not a mistake in the program.

## Consequences

- Porting a SciPy program changes `from scipy.X import` to `from X import` and nothing else,
  and the rule is the same one NumPy programs already follow.
- The standard library's top level gains six names. A program's own `special.byn` or
  `optimize.byn` must be renamed, and the compiler says so.
- The kernels of 3.18 are written in a module whose name is one line in the driver's library
  table and one in the resolver's list of standard modules, so building them does not wait on
  this record.
