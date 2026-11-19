# 0069. `interpolate` keeps SciPy's names, with scalar calls, 2-D grids and panics for bad input

Status: Accepted
Date: 2026-11-19
Roadmap: 3.21

## Context

Row 3.21 ports `scipy.interpolate`, which AstroPy's `modeling` uses, under SciPy's names
([decision 0054](0054-scipy-module-names.md)). A study on SciPy 1.18.1 and Baryon main 6358dd5
compiled each API shape the port needs. It found four limits of the language today:

- **No overloading.** SciPy's `cs(xs)` takes a scalar or an array, and a Baryon method takes
  one type.
- **No `str` in a union** ([decision 0032](0032-sum-types-are-unions-of-structs.md)), so
  `bc_type` cannot take a string or a tuple.
- **No rank-generic functions**, so one type cannot take both a 2-D and a 3-D grid.
- **No constructor that computes.** Record 0068, proposed, adds one, `__init__`, so
  `CubicSpline(x, y)` keeps SciPy's name.

Collins accepted the seven points of the study on 2026-11-19 (card 47, "Accept all seven").
Point 1 is record 0068. This record holds the other six.

## Decision

### 1. A call takes one `f64`

An interpolator's `__call__` takes one `f64` and returns one `f64`: `cs(0.5)`. SciPy gives a
0-d array there. Calling it on an array waits for idea I.61, the element-by-element mechanism
`special` waits for too, and no second method name is added in the meantime.

### 2. `bc_type` takes its four names

- `bc_type` takes `"not-a-knot"`, `"clamped"`, `"natural"` or `"periodic"`. SciPy's
  `((order, value), (order, value))` form waits for an idea row on string unions.
- `extrapolate` is `bool | None`. `bc_type="periodic"` makes extrapolation periodic by itself,
  as SciPy does.

### 3. `numpy.interp` is `Array.interp`

`Array.interp(x, xp, fp, left=, right=, period=)` is NumPy's 1-D linear interpolation, as
`numpy.loadtxt` became `Array.loadtxt`. `x` is an array. A scalar `x` waits for I.61, as in
point 1.

### 4. Grids are 2-D in this row

- `RegularGridInterpolator` and `interpn` take 2-D grids: `points` is a tuple of two arrays,
  and a point is `(x, y)`.
- A single point gives an `f64`, where SciPy gives an array of shape `(1,)`.
- Grids of 3 or more dimensions wait for an idea row on rank generics.

### 5. Bad input panics with SciPy's message

SciPy raises `ValueError` for each of these, and Baryon panics with SciPy's message instead, as
decision [0040](0040-linear-algebra-and-fft.md) does for wrong shapes:

- an `x` that is not strictly increasing;
- too few points for the order;
- an unknown `bc_type`;
- a grid query out of bounds while `bounds_error=True`, the default.

`bounds_error=False` gives `fill_value`, which defaults to NaN, as in SciPy.

### 6. Scope

In this row, in this order (rows 3.21.1 to 3.21.5):

1. `Array.interp`;
2. `CubicSpline`, `PchipInterpolator`, `Akima1DInterpolator` and `PPoly`;
3. `BSpline`, `make_interp_spline`, `make_lsq_spline` and `make_smoothing_spline`;
4. the FITPACK 1-D family AstroPy uses: `splrep`, `splev`, `UnivariateSpline`,
   `LSQUnivariateSpline` and `InterpolatedUnivariateSpline`, ported from Dierckx's `curfit` to
   SciPy's bits;
5. `RegularGridInterpolator` and `interpn` on 2-D grids.

Later rows:

- `make_splrep`, a different algorithm from `splrep`: at `s=1.0` on 50 points, `splrep` chose 54
  knots and `make_splrep` chose 47, so one port cannot match both;
- 2-D FITPACK, `RectBivariateSpline` and `bisplrep`;
- scattered data, `griddata` and the `ND` interpolators, which need a Delaunay triangulation or a
  k-d tree (row 3.22).

Left out: `interp1d`, which SciPy marks legacy and AstroPy does not use.

### Precision

Each routine aims at SciPy's bits on Linux x86_64, as `stats` and `integrate` do. LAPACK's
`gtsv`, de Boor's recursion and Dierckx's `curfit` are fixed sequences of `+ - * /` that a port
can follow. Tests record their expected values from SciPy on Linux.

## Alternatives considered

- **A second method for arrays, such as `cs.many(xs)`.** It would ship a name that has to go
  once I.61 lands.
- **Letting `str` into unions now, amending 0032.** It changes the language for one parameter
  that four names cover.
- **`Array.interp` as a free function, `interp(x, xp, fp)`.** NumPy's functions on arrays are
  `Array.` functions in Baryon.

## Consequences

- SciPy's constructors and calls port unchanged for scalars: `CubicSpline(x, y)(0.5)`.
- Five differences from SciPy, each flagged in the reference:
  - a scalar call gives an `f64`, not a 0-d array;
  - array calls wait for I.61;
  - `bc_type`'s tuple form waits;
  - grids are 2-D;
  - bad input panics where SciPy raises.
