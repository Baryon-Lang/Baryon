# AstroBaryon roadmap

AstroBaryon is Baryon's astronomy library: AstroPy's capability, written natively in Baryon. No
Python runs underneath it and no AstroPy code is wrapped. It is designed from the astronomy
standards (IAU SOFA and ERFA, the FITS and WCS papers, the IVOA formats) and checked against
results recorded from AstroPy, so that an astronomer who knows AstroPy can move to it with a
guide in hand (Collins, 2026-11-10).

This file is the plan: what AstroBaryon covers, the order it is built in, every Baryon library it
needs first, and the documentation each piece must ship with. The work items themselves, with
their status, owners and pull requests, are rows in [`ROADMAP.md`](../../ROADMAP.md), which stays
the single source of truth ([decision 0002](../decisions/0002-roadmap-source-of-truth.md)):
row 4.2 and its sub-rows for AstroBaryon, and rows 3.18 to 3.32 for the base libraries it needs.
This file cites rows by ID and never copies their status, so the two cannot disagree. A pull
request that changes the scope or order of an AstroBaryon row changes this file too.

The sizes come from the study "Recreating AstroPy natively in Baryon" (10 November 2026,
AstroPy 8.0.2 measured against Baryon's `main`): about 22 to 28 expert person-years for
AstroBaryon once the Phase 3 libraries exist, and about 50 for a full port of AstroPy's Python
API, which is not the goal. The roadmap's rows count about 45 weeks to a person-year.

## What AstroBaryon covers

Each AstroPy subpackage, the row that provides it, and what changes on the way. This table is
also the first outline of the [transition guide](#the-transition-guide-from-astropy).

| AstroPy | Row | Notes |
| --- | --- | --- |
| `pyerfa` (`erfa`) | 3.31 | A base library: all 251 ERFA functions in Baryon, usable without AstroBaryon |
| `astropy.time` | 4.2.2 | Two-part Julian dates, leap seconds, Earth orientation tables |
| `astropy.units`, `astropy.constants` | 4.2.3 | Units checked at compile time in code, a run-time unit for file data; the form waits on D.19 and D.20 |
| `astropy.coordinates` | 4.2.4 | Frames as data, not a class hierarchy: Baryon has no inheritance ([decision 0015](../decisions/0015-no-class-inheritance.md)) |
| `astropy.wcs` | 3.32 | A base library built from the WCS papers; header parsing by hand-written parsers |
| `astropy.io.fits` | 3.14, 4.2.5 | Plain FITS reading in the file formats row; compression, binary tables and memory mapping in 4.2.5 |
| `astropy.table`, `astropy.io.ascii`, `astropy.io.votable` | 4.2.6 | A table on Baryon's DataFrame (3.13), with columns of units, times and coordinates |
| `astropy.modeling` | 4.2.7 | Models as traits; compound models need operators on user types (D.19) |
| `astropy.cosmology` | 4.2.8 | The standard FLRW models |
| `astropy.stats`, `astropy.convolution`, periodograms in `astropy.timeseries` | 4.2.9 | Sigma clipping, Bayesian blocks, convolution, Lomb-Scargle, box least squares |
| `astropy.nddata`, containers in `astropy.timeseries` | 4.2.15 | `NDData`, `CCDData`, cutouts, `TimeSeries`, `BinnedTimeSeries` |
| `astropy.visualization` | 4.2.10 | Stretches and normalizations; sky-coordinate axes on Baryon's plotting (3.15) |

[Not planned](#not-planned) lists the parts of AstroPy that AstroBaryon leaves out, and why.

How close AstroBaryon's names and calls stay to AstroPy's is open decision D.24. The proposal is
to keep AstroPy's module, class, function and argument names wherever the behavior is the same,
and to differ only where Baryon's types force it, with every difference in the transition guide.

## Documentation is a deliverable

Collins set documentation as the top priority for AstroBaryon: an astronomer must be able to move
from AstroPy without reading AstroBaryon's source. So documentation is part of every row's
definition of done, not a follow-up. No AstroBaryon row is marked `Done` until its pull requests
ship all five of these:

1. **Reference documentation** for every public item, as [rule 2](../../CONTRIBUTING.md#rule-2-documented-extensively)
   asks of all Baryon code: what it is for, units, precision, limits, and a tested example.
2. **Transition guide entries** for every AstroPy name the row covers: the AstroBaryon
   equivalent, or "not provided" with the reason and the workaround.
3. **Every behavior difference flagged**, in the entry and in a list per module: a different
   default, precision, error, unit rule, or a check that moves from run time to compile time.
4. **A worked example program**, run in CI, that does a task astronomers do with AstroPy, with
   the AstroPy version beside it.
5. **Validation against reference data** recorded from AstroPy (4.2.1), with the tolerance stated
   in the documentation: for example, 1 microarcsecond on a frame transform, 1 nanosecond on a
   time scale conversion.

A row that touches large data also adds a benchmark against AstroPy to 4.2.14.

### The transition guide from AstroPy

Row 4.2.11 builds the guide's frame; each row then fills its own part. The guide has one page per
AstroPy subpackage, each with a table like this one:

| AstroPy | AstroBaryon | Differences |
| --- | --- | --- |
| The AstroPy call, as a user writes it | The AstroBaryon call that does the same | Every way the result or behavior differs, or "none" |

The guide also has:

- **A first page for the impatient:** the ten things that change for every AstroPy user, such as
  units checked before the program runs and frames as values rather than subclasses.
- **Side-by-side programs:** each worked example from rule 4 above, AstroPy on the left and
  AstroBaryon on the right.
- **A coverage list,** checked in CI by 4.2.11, of every public name in the AstroPy subpackages in
  scope, each marked mapped, not provided, or not yet built. An AstroPy release that adds a name
  shows up as an unmapped entry.

Until D.24 is decided, the example rows in the guide are written against AstroPy's names.

## Prerequisites, in build order

Every Baryon library and language feature AstroBaryon needs, in the order they unblock it. Each
row's status is in [`ROADMAP.md`](../../ROADMAP.md). Rows marked *base* are general-purpose
libraries that the rest of Baryon uses too, which is why they live in Phase 3 and not under 4.2.

### Decisions first

These settle the shape of whole modules, so they come before the code that depends on them.

| Row | What it settles | Blocks |
| --- | --- | --- |
| D.19 | Operators on user types (`5 * u.km`, `Time + TimeDelta`, `Gaussian1D() + Const1D()`) | 4.2.3, 4.2.7, every number-like value |
| D.20 | Physical units: compile-time, run-time, or both | 4.2.3, and through it most of AstroBaryon |
| D.21 | Missing values: masked arrays, validity bitmaps, or one form for both | 3.24, 3.13, 4.2.6, 4.2.9 |
| D.22 | Networking and TLS: written in Baryon or taken from the operating system | 3.30, and through it 4.2.2 and 4.2.4 |
| D.23 | Where AstroBaryon lives: with Baryon's standard distribution, or a separate package | 4.2.13 and, if separate, 4.3 |
| D.24 | How close AstroBaryon's names and calls stay to AstroPy's | 4.2.11 and every AstroBaryon row's API |
| D.1 | Team size, which sets every estimate below | Calendar dates |

### Language and arrays

| Row | Why AstroBaryon needs it |
| --- | --- |
| 1.7.13 | Sum types: frames, table columns and file headers whose kind is known only at run time |
| 1.13.6 | Files and processes: reading data files and writing results |
| 1.14 | N-dimensional arrays, views and parallel loops: every vectorized coordinate, time and image operation |
| 3.1, 3.2, 3.3 | Compiled-code speed: catalog-scale transforms are where AstroBaryon beats AstroPy |
| 3.5 | `Dyn[Trait]`: transform graphs, model registries and readers chosen at run time |
| 3.9 | Broadcasting and axis reductions: arrays of coordinates against arrays of times |
| I.39 | `bytes`, binary files and memory mapping: FITS is big-endian binary and often larger than memory. An idea today; it must be accepted into a phase before 4.2.5 starts |
| 4.3 | Packages, if D.23 makes AstroBaryon a separate package |

### Base libraries

In the order AstroBaryon's milestones need them.

| Row | Library | Replaces | First needed by |
| --- | --- | --- | --- |
| 3.31 | ERFA | `pyerfa` and the ERFA C library | 4.2.2 |
| 3.25 | Record, fixed-width text and date arrays | NumPy structured dtypes, `datetime64` | 4.2.2, 4.2.5 |
| 3.10 | Linear algebra and FFT | NumPy and SciPy linear algebra and FFT | 4.2.4, 4.2.9 |
| 3.21 | Interpolation | `scipy.interpolate` | 4.2.4 |
| 3.29 | JSON, YAML subset, base64 | `json`, PyYAML, `base64` | 4.2.6 |
| 3.30 | Networking and download cache | AstroPy's data downloads | 4.2.2 |
| 3.27 | Compression | zlib and cfitsio's tile compression | 4.2.5 |
| 3.13 | DataFrame | pandas-like tables under `Table` | 4.2.6 |
| 3.14 | File formats, FITS included | AstroPy's FITS core, CSV, HDF5 | 3.32, 4.2.5 |
| 3.32 | World coordinate systems | wcslib | 4.2.4 (pixel to sky), 4.2.10 |
| 3.24 | Missing values | NumPy masked arrays | 4.2.6, 4.2.9 |
| 3.26 | Regular expressions | Python's `re` | 4.2.6 |
| 3.28 | XML | expat | 4.2.6 (VOTable) |
| 3.18 | Special functions | `scipy.special` | 4.2.8 |
| 3.19 | Integration | `scipy.integrate` | 4.2.8 |
| 3.20 | Optimization and fitting | `scipy.optimize` | 4.2.7 |
| 3.16 | Statistics | NumPy and SciPy statistics | 4.2.9 |
| 3.22 | Spatial search and image processing | `scipy.spatial`, `scipy.ndimage` | 4.2.4 (catalog matching), 4.2.9 |
| 3.23 | Distributions | `scipy.stats` | 4.2.9 |
| 3.15 | Plotting | Matplotlib | 4.2.10 |

## Building AstroBaryon

Five milestones, each a usable library on its own. A milestone is done when its rows are `Done`
in `ROADMAP.md`, which includes their documentation.

1. **Groundwork.** Reference data recorded from AstroPy (4.2.1), the package skeleton with its
   documentation site and CI (4.2.13), and the transition guide's frame and coverage list
   (4.2.11). ERFA (3.31) is built in parallel: it needs little beyond what builds today and is the
   best early showcase. Starts once D.23 and D.24 are decided.
2. **Astrometry core.** Time (4.2.2), units and constants (4.2.3), coordinates (4.2.4), world
   coordinate systems (3.32) and FITS images without compression (3.14). This covers most
   day-to-day astrometry and is the study's 10 to 12 person-year core, prerequisites included.
3. **Data.** FITS compression, binary tables and memory mapping (4.2.5), and tables with ECSV,
   IPAC, CDS, fixed-width and VOTable (4.2.6).
4. **Analysis.** Models and fitting (4.2.7), cosmology (4.2.8), statistics and periodograms
   (4.2.9), and data containers (4.2.15).
5. **Visualization.** Image stretches and sky-coordinate axes (4.2.10).

The user guide and tutorials (4.2.12) and the benchmarks against AstroPy (4.2.14) grow with every
milestone: each adds its tutorials and its benchmarks.

## Where AstroBaryon should beat AstroPy

These are the reasons to switch, so the documentation leads with them and 4.2.14 measures them.

- **Unit errors before the program runs.** Adding a distance to a velocity is a compile error, and
  units compile away to plain floats, so they cost nothing at run time. AstroPy checks units on
  every operation, at run time.
- **Large catalogs.** Transforming a billion sources between frames is ERFA in a parallel loop
  over arrays, with no temporary arrays and no per-call Python overhead.
- **Reproducible results.** Baryon's fixed reduction order gives the same bits on every run and
  every thread count, which matters when matching published astrometry.

## Not planned

Left out of AstroBaryon, each for a reason. Any of them can come back through the
[Ideas](../../ROADMAP.md#ideas) table.

| AstroPy part | Why it is left out |
| --- | --- |
| `astropy.samp` | Desktop message passing between applications over XML-RPC; little use in compiled pipelines |
| `astropy.io.misc` (HDF5, Parquet, ASDF, pandas bridges) | Covered by Baryon's own file formats (3.14) and DataFrame (3.13) |
| `astropy.config`, `astropy.utils`, `astropy.io.registry` | Python plumbing; Baryon packages, the download cache (3.30) and `Dyn` (3.5) replace them |
| `astropy.uncertainty` | Small and little used; an idea once 3.23 exists |
| NumPy subclassing and ufunc overrides | Python mechanisms with no Baryon equivalent; compile-time units (D.20) do their job |

## Risks and open estimates

- **The DataFrame row is far too small.** 3.13 is 5 weeks; the pandas study (10 November 2026)
  puts a minimal DataFrame at about 90 weeks. Tables (4.2.6) and FITS binary tables (4.2.5) sit on
  it, so the data milestone moves with it. 3.14 (3 weeks) is low for the same reason.
- **The estimates assume people, not agents.** The rows count expert person-weeks. Agents shorten
  the writing; they do not shorten the prerequisite chain or numerical validation, which is where
  most of the time goes. D.1 turns weeks into dates.
- **No lambdas or closures yet.** Integrands, fitters and custom models pass functions as
  arguments ([decision 0026](../decisions/0026-functions-as-arguments.md)) and carry parameters
  in a struct. If that proves clumsy when 4.2.7 starts, it raises a language decision then.
- **No inheritance.** 686 of AstroPy's 774 classes inherit from another. AstroBaryon models the
  same ideas with traits ([decision 0031](../decisions/0031-traits-are-protocols.md)) and sum
  types ([decision 0032](../decisions/0032-sum-types-are-unions-of-structs.md)), which changes
  how user code extends frames and models. The transition guide must say how for each one.
