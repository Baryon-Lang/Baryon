# Release notes

What each release of Baryon holds, newest first. A release is a tag on `main`; nothing is
published to a package registry yet. Releases are time-based: one every two weeks, cut from what
`main` holds that day, and numbered 0.x, one step of 0.1 each.
[`ROADMAP.md`](ROADMAP.md#releases) tracks the steps of the release in progress.

## Unreleased

Changes merged since 0.3.0. They become the notes of 0.4.0, due 2026-12-09.

- The documentation has a website, the [Learn site](https://baryon-lang.github.io/Baryon/): every
  page of `docs/`, with a sidebar and search, opening on
  [Learn Baryon](docs/guide/learn.md), which says where each kind of reader should start. It is
  published from `main`, so it always describes the compiler `main` builds.
- The array and numerics layer has a name, NumBaryon: built-in arrays plus the `math`, `random`,
  `stats`, `linalg` and `fft` modules. It is a name for the documentation, not a package, so no
  program changes ([Arrays](docs/reference/arrays.md#numbaryon)).
- `groupby` takes more of the forms pandas has: `as_index=False` and `sort=False`, `size()` and
  the other reductions as methods, a choice of columns as `df.groupby(k)[["a", "b"]].sum()`,
  `agg` with a dict or with one reduction's name, and a loop over the groups,
  `for key, group in df.groupby(k):`
  ([Sorting, grouping and joining DataFrames](docs/reference/dataframe-operations.md)).
- `groupby` with a sum and a mean of 10 million rows takes 0.24 s, down from 0.87 s, with the same
  results bit for bit.
- `read_csv` reads a cell of a plain decimal, such as `-12.345`, with no copy. The benchmark's
  10 million rows read in 3.96 s, down from 4.74 s, with the same values bit for bit
  ([the DataFrame benchmark](bench/dataframe/README.md)).
- `lzma.compress` writes xz data that Python's `lzma.decompress` reads. Presets 4 to 9 compress
  like preset 3 with their larger dictionary for now, so their output runs about 4% to 28% larger
  than Python's; an optimal parser to close the gap is row 3.27.12
  ([`lzma.compress`](docs/reference/lzma.md#lzmacompress)).
- `DataFrame.read_csv` and `to_csv` take `sep=`, one character written out, such as `";"` or
  `"\t"`. `read_csv` takes `na_values=`, more cells to read as missing, `skiprows=`, lines to
  pass over before the header, and `nrows=`, the most rows to read, and `to_csv` takes `na_rep=`,
  the text written for a missing value, `header=False`, to write the rows alone, and `columns=`,
  the columns to write and their order
  ([CSV files](docs/reference/csv-files.md#keywords)).
- `DataFrame.read_parquet(path)` reads a Parquet file into a DataFrame, with the schema written in
  the program as for `read_csv`: columns of floats, whole numbers, `bool` and `str`, with missing
  values, from files compressed with Snappy, gzip, Zstandard or LZ4 or not at all. The reader is
  written in Baryon, so a program needs no other library. Dates, decimals and nested columns are
  not read yet. The encodings writers choose for the format's version 2 are read too: packed
  differences, text as lengths or shared prefixes, and split byte streams
  ([Parquet files](docs/reference/parquet-files.md)).
- `df.to_parquet(path)` writes a DataFrame as a Parquet file that pandas, polars and other
  readers open with the same columns and types, compressed with Snappy. Text is written
  through a dictionary, as those libraries write it, and so are numbers that repeat. Each
  column of each row group carries its statistics, the count of missing values and the
  smallest and largest value, so a reader that filters can pass over row groups
  ([Parquet files](docs/reference/parquet-files.md#writing-a-file)).
- Reading and writing Parquet files is about a fifth faster: a million rows of fifteen columns
  read in 0.32 s where they took 0.39 s, and are written in about 0.7 s where they took
  about 0.9 s ([Parquet files](docs/reference/parquet-files.md#cost)).
- Each release now comes with the VS Code extension, `baryon-<version>.vsix`, attached beside the
  archives, so trying the extension no longer needs Node.js
  ([Coding Baryon in VS Code](docs/guide/vscode.md#3-install-the-extension)).
- A function generic over a `VectorSpace` state reads it by position: `len(y)` is its number of
  elements and `y[i]` element `i`, an `f64`, so one function weighs every element of an `f64`,
  a `Fixed` vector or an `Array`. An `f64` has length 1. Elements are only read
  ([Generics](docs/reference/types.md#traits)).
- `integrate.solve_ivp` solves an ordinary differential equation with SciPy's default method,
  RK45, on an `f64` or `Fixed` state, with `args=`, `rtol=`, `atol=`, `first_step=` and
  `max_step=`, and with `t_eval=` for the state at chosen times. `method=` is chosen when
  compiling: an unknown name written as a literal is a compile error. It takes SciPy's steps
  and returns an `OdeResult` with SciPy's fields. An `Array` state, DOP853, dense output and
  events come later
  ([`solve_ivp`](docs/reference/integration.md#solve_ivp-solving-an-ode)).

- A long list literal of numbers or `bool`s builds in a fraction of the time with `--release`:
  its elements are copied from one table, not stored one by one. A literal of 100,000 floats
  took 110 s and now takes 0.13 s. A program holding a literal of 10,000 short lists went from
  260 s to 49 s, and is still slower than it should be.
- A `time` module for timing a program's phases: `time.perf_counter()`, `time.monotonic()` and
  `time.time()`, each an `f64` of seconds with Python's meaning ([Clocks](docs/reference/time.md)).

## 0.3.0: missing values, CSV files and SciPy's special functions

The first release since 0.1.0 to be published: 0.2.0 was prepared but never published, and the chief
designer folded it into this one on 2026-11-21, so 0.3.0 holds everything in the 0.2.0 notes below
and the 143 changes merged since, among them missing values in arrays and tables, CSV files, the
rest of SciPy's special functions, numerical integration, and the fixes from two bug sweeps. [What
builds today](docs/reference/status.md) lists, chapter by chapter, what the compiler does not build
yet. Please [open an issue](https://github.com/Baryon-Lang/Baryon/issues) for anything wrong,
unclear or missing, with what `baryon --version` prints.

### Platforms and install

As in the 0.2.0 notes below: Linux on x86_64 with glibc 2.35 or newer and `clang` 15 or newer, and
macOS 13.5 or newer on Apple silicon. Download `baryon-0.3.0-x86_64-unknown-linux-gnu.tar.gz` or
`baryon-0.3.0-aarch64-apple-darwin.tar.gz` as the [beginner's
guide](docs/guide/README.md#1-install-baryon) shows, or build from source with `cargo`. Each archive
holds both licenses, `THIRD-PARTY-NOTICES.md` and `CRATE-LICENSES.md`.

### Missing values

A value that is absent is `None`, kept apart from the float `NaN`, as pandas keeps them apart
([missing values](docs/reference/missing-values.md)).

- **Arrays with gaps**, `Array[T | None, 1]` of numbers, `bool`s or text: built from a
  `list[T | None]`, read and assigned by position, mask or list of positions, looped over, with
  `isna`, `notna`, `dropna` and `fillna`; sums, means, `min`, `max`, `var` and `std` that skip
  `None`; arithmetic, `**`, bitwise operators on whole numbers and in-place operators, where
  `None` on either side gives `None`; [comparisons](docs/reference/missing-comparisons.md) and
  masks combined with `&`, `|`, `^` and `~`; `any`, `all`, `argsort`, `unique` and `nunique`; and
  `print` in pandas's form, also inside tuples, lists, dicts and unions.
- **Columns with gaps** in a `DataFrame`, a field `bid: f64 | None`
  ([missing values in a DataFrame](docs/reference/missing-values-in-frames.md)): built, chosen,
  sorted with `na_position=`, grouped, joined, stacked, compared, described and printed;
  `df.dropna()` with `how=` and `subset=`, and `df.fillna()`.

### DataFrame

- **[CSV files](docs/reference/csv-files.md)**: `DataFrame.read_csv(path)` into a frame whose
  columns the type names, of `f64`, `f32`, every whole number type, `bool` and text, with gaps
  where a cell is empty, and `df.to_csv(path)`.
- **Joins**: `merge` with `how="left"`, `"right"` and `"outer"`, whose rows without a partner hold
  `None`, and with `left_on=`, `right_on=` and `suffixes=`.
- **Grouping**: `size` and `nunique` in `groupby(...).agg(...)`.
- **pandas's arguments** on summaries and `sort_values`, `assign` of one value for every row, and
  `drop` and `rename` with `axis=1` or `axis="columns"`.
- **Speed**: sorting float and integer keys in about n steps, and `read_csv` reading number cells
  where they lie instead of copying each one, 7.7 s to 4.0 s for 10 million rows (#556). The
  [DataFrame guide](docs/guide/dataframes.md#how-fast-is-it) compares nine operations on 10
  million rows with pandas and Polars: Baryon is ahead of pandas in five and behind in
  `groupby`, `merge`, missing values and `read_csv`.

### Numerical libraries

- **[Special functions](docs/reference/special-functions.md)**: SciPy's special functions beyond
  the gamma family and the error functions: Bessel functions of whole, real and spherical order,
  Airy, Kelvin and Struve functions; incomplete gamma and beta functions; distribution functions,
  Kolmogorov's and Owen's T; exponential integrals, zeta, the dilogarithm and Lambert W; elliptic
  integrals and Jacobi's elliptic functions; orthogonal polynomials and spherical harmonics; the
  nodes and weights of Gauss quadrature; the confluent and Gauss hypergeometric functions;
  parabolic cylinder, Mathieu and spheroidal wave functions; and the everyday helpers and
  counting functions. Each is tested against high-precision values. A new
  [guide](docs/guide/special-functions.md) works through four physics problems with them and
  compares their speed with SciPy's.
- **[Numerical integration](docs/reference/integration.md)**
  ([decision 0065](docs/decisions/0065-integrate-api.md)): `integrate.quad` on finite and infinite
  intervals, QUADPACK's QAGS, QAGI and, with `points=`, QAGP; and the fixed rules `trapezoid`,
  `cumulative_trapezoid`, `simpson` and `romb`, with SciPy's bits.
- **[Interpolation](docs/reference/arrays.md#interpolation)**: `Array.interp`, NumPy's
  one-dimensional linear interpolation, with NumPy's bits. The rest of `scipy.interpolate` is
  planned ([decision 0069](docs/decisions/0069-interpolate-api.md)).
- **Matrix products through BLAS**: with
  [`--blas`](docs/reference/commands.md#matrix-products-through-blas), `@` of two `f32` or `f64`
  matrices calls the system's BLAS, such as OpenBLAS or Accelerate.
- **FFT** of any length whose prime factors are small, through a mixed-radix transform.
- **`f64x2.of(x)`** builds a double-double float from an `f64`.

### Files and compression

- `gzip.open` in the binary and text modes, and `lzma.open` to read xz and `.lzma` files as bytes
  or text.
- `text.encode()` into an `Array[u8, 1]` of UTF-8 bytes, and `data.decode()` back
  ([decision 0064](docs/decisions/0064-utf-8-text-from-bytes.md)).
- NetCDF files stream their definitions instead of loading the whole file, and write values in
  place.
- `byplot` compresses the images inside SVG files with `zlib`.

### New in the language

- **Variadic generics**
  ([extra arguments for a callback](docs/reference/types.md#extra-arguments-for-a-callback)):
  a type parameter `*Ts` that stands for a list of types, and `f(*args)` on a tuple, so a
  library can pass a caller's extra arguments on to a callback, as `quad(f, a, b, args=(k,))`
  does.
- **Converting to a type parameter**, `T(x)`, and type arguments written on a struct's
  constructor, `Box[f64]([])`
  ([decision 0066](docs/decisions/0066-converting-to-and-naming-type-parameters.md)).
- **A struct called like a function** through its `__call__` method.
- **A list written with `None`** and no annotation takes its type from the other elements.
- **A local that holds a function** can be annotated and called.
- **`match` captures** that borrow a read-only subject.
- **Standard-library modules split into files** re-export their names
  ([decision 0062](docs/decisions/0062-standard-library-reexports.md)).

### Fixed

- The two known bugs of 0.2.0: a false "moved" error on `entry.name` in a `for` loop over paths,
  and a `try` that reuses one name in two `except ... as` clauses (#439).
- Array arithmetic: integer and true division, and `min` and `max` of `bool` arrays (#443).
- A `Callable` called inside a `prange` loop (#485), a local that holds a function outside
  generic code (#471), and an optional frame read after it was checked (#509).
- `**` on a value whose type is a parameter bound by `Float`, which passed the checker and then
  stopped the build (#555).
- Shortest float text breaks ties to the even digit, as Python does (#524), and an f-string
  writes any value as `print` does (#513).
- Mistakes that reached the backend and failed there are now compile errors that say what is
  wrong: `Array.zeros` of a union, callbacks decision 0026 leaves out, calls on type parameters
  the checker cannot type, members a standard module lacks, a non-integer index, `None` in an
  in-place operator, shifts and bitwise operators on arrays that are not whole numbers, type
  arguments that break their bounds or are written on built-in constructors, and values or
  number conversions a type parameter cannot show, `=`, `!r` and `!a` in f-strings, and tuples
  and dicts that hold the bare type `None` (#559).

### Decisions made for this release

Decisions [0062](docs/decisions/0062-standard-library-reexports.md),
[0064](docs/decisions/0064-utf-8-text-from-bytes.md),
[0065](docs/decisions/0065-integrate-api.md),
[0066](docs/decisions/0066-converting-to-and-naming-type-parameters.md) and
[0069](docs/decisions/0069-interpolate-api.md) were accepted since 0.2.0; the
[decision index](docs/decisions/README.md) lists each with its status.

### Known limits

- [What builds today](docs/reference/status.md) lists what is not built yet, among them Parquet
  files, writing HDF5 files, `df[mask] = ...`, `concat` of frames with different columns, `quad`
  with `weight=`, `solve_ivp` and the rest of `scipy.interpolate`.
- A list of only `None` with no type written, `[None, None]`, passes `baryon check` but fails when
  building (roadmap idea I.74). Write its type, such as `list[f64 | None]`.
- `groupby`, `merge` and `read_csv` are slower than pandas on large tables, and every DataFrame
  operation runs on one core.
- No packages outside the standard library, no interop with Python or C++, and no REPL yet.

## 0.2.0: Linux and macOS, real data and the first libraries

Never published: its notes are kept here because 0.3.0 is the first release to carry these
changes.

Cut early, on the chief designer's call of 2026-11-13, as the first run of the full release process;
the two-week schedule continues from it (roadmap R.5 and R.6). It holds everything merged to `main`
since 0.1.0: the Linux and macOS checkpoint (R.2), the real-data checkpoint (R.3), the native
numeric and data libraries that followed, and every fix since, among them the double frees closed
by #175 and #176.

The second release, and the first to download and use without building the compiler: prebuilt
`baryon` archives for Linux and macOS, programs written the way the reference describes them
without the 0.1.0 workarounds, files and other programs, and libraries for arrays, linear algebra,
statistics, data files, tables, plots and compression, written in Baryon. `baryon check` knows the
whole of version 0 of the language, and the compiler builds most of it.
[What builds today](docs/reference/status.md) lists, chapter by chapter, what it does not build
yet. Please [open an issue](https://github.com/Baryon-Lang/Baryon/issues) for anything wrong,
unclear or missing, with what `baryon --version` prints.

### License

Baryon is now dual-licensed: Apache-2.0 OR MIT, at your choice, as most Rust projects are
([decision 0055](docs/decisions/0055-dual-license.md)). 0.1.0 was Apache-2.0 only. Third-party
content the compiler carries, such as the font `byplot` draws text in, is listed with its license
in [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md). Each archive holds both licenses, those
notices, and `CRATE-LICENSES.md`, the licenses of the Rust crates compiled into `baryon` and
`libruntime.a`.

### Platforms

| Platform | Needs |
| --- | --- |
| Linux on x86_64 with glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36 and later | `clang` 15 or newer (`sudo apt install clang-15` on Ubuntu 22.04, whose default is 14) |
| macOS 13.5 or newer on Apple silicon | The Command Line Tools 15 or newer, from `xcode-select --install` |

`clang` 15 is the floor because the LLVM backend writes opaque pointers
([decision 0020](docs/decisions/0020-llvm-backend-emits-text.md)). Both platforms are tested in CI
on every change. The release is tested on Ubuntu 22.04 and macOS 15; macOS 13.5 is the oldest
version whose `clang` is new enough, but no older Mac has run this release yet. Intel Macs and
musl Linux such as Alpine have no download and build from source. Windows is the next platform
(roadmap R.4).

### Install

The [beginner's guide](docs/guide/README.md#1-install-baryon) shows both ways:

- **Download** `baryon-0.2.0-x86_64-unknown-linux-gnu.tar.gz` or
  `baryon-0.2.0-aarch64-apple-darwin.tar.gz` from the release, with `curl` so macOS does not
  quarantine it, check it against its `.sha256` file, unpack it and put the directory on your
  `PATH`. `baryon` and the runtime library `libruntime.a` travel together in it. Before the
  archives are attached, the release workflow unpacks each on a fresh machine without Rust and
  builds, runs and tests the example programs and the guide's first program with it, in debug and
  `--release`, and checks that a missing `clang` gives the message that says how to install it.
- **Build from source** with `cargo`, as in 0.1.0.

### New in the language

- **Traits** ([decision 0031](docs/decisions/0031-traits-are-protocols.md)): declare one as a
  `Protocol` class, implement it by naming it as a base, and bound a type parameter with it,
  `[T: Body]`; calls resolve when compiling, with no run-time cost. `Embed[T]` fields reach an
  embedded struct's fields and methods through the outer one.
- **Sum types** ([decision 0032](docs/decisions/0032-sum-types-are-unions-of-structs.md)): unions
  of structs, `Circle | Square`, with class patterns in `match`, and methods and trait bounds on
  them.
- **A struct's methods called through its class**, `Point.origin()`, generic structs included,
  `Pair.both(1.5)` or `Pair[i64].both(2)`.
- **`with` and `for` on your own classes**
  ([decision 0053](docs/decisions/0053-with-and-for-on-user-classes.md)): `with` works on any class
  with `__enter__` and `__exit__`, and `for` on any class with `__iter__` and `__next__`.
- **Optional arguments of any type**: an `Array` or a `str` passes where a read-only parameter
  takes `T | None`, as `yerr=errors` does.
- **Array slices**: `a[i:j]`, `a[i:j:k]` and `a[:, k]` are views that copy nothing. Read them,
  pass them to read-only and `InOut` parameters, use them in whole-array arithmetic, and assign
  into them with `=` and `+=`, including `forces[a:b] += v` in a `prange` loop with
  `accumulate=`.
- **Arrays as NumPy has them**: [broadcasting](docs/reference/arrays.md#broadcasting) between arrays
  of different ranks, new axes with `None`, comparisons that give arrays of `bool`s, `a[mask]` and
  `a[positions]`, `sum`, `min` and `max` along an axis, `mean`, `var` and `std` with NumPy's bits,
  `argsort`, `unique` and `nunique`, the matrix product `@`, and `print(a)` in [NumPy's
  form](docs/decisions/0033-printing-arrays.md).
- **Arrays of text**, `Array[str, 1]` ([text arrays](docs/reference/text-arrays.md)): built from a
  list, sliced, masked, compared, sorted, searched with `in` and printed as NumPy prints them.
- **Complex numbers**: `complex128` (`complex`) and `complex64`, `2j` literals, arithmetic,
  `z.real`, `z.imag`, `z.conjugate()`, `abs(z)`, `complex(re, im)`, `print` and f-strings, with
  CPython's results bit for bit. Division by zero gives IEEE infinities and NaNs, as NumPy does,
  where Python raises.
- **Statements**: `for` over arrays, tuples, lists, `enumerate`, `zip` and `reversed`, a `range`
  step known only at run time, `**` and `**=` on integers, and nested unpacking `a, (b, c) = e`.
  `raise ... from` is rejected with a clear error
  ([decision 0034](docs/decisions/0034-no-error-chaining.md)).
- **Generics**: methods of generic structs, a `Callable` parameter passed on to another call or
  kept in a local, default parameter values computed at each call
  ([decision 0027](docs/decisions/0027-default-parameter-evaluation.md)), and `[T: Float]`
  functions that call `math` and divide.
- **`print` of every value**: `None`, optional values, tuples, enum members as `Phase.GAS`,
  structs as `Type(field=value, ...)`, lists and dicts as Python writes them, arrays as NumPy
  writes them, with the `sep=`, `end=`, `file=` and `flush=` keywords.
- **Reproducible floating point**: float sums and products in a `simd` loop add in 8 fixed lanes,
  `a.sum()` adds in NumPy's pairwise order, and scalar float loops are vectorized only in their
  written order, so a program gives the same bits on every machine.
- **Integer overflow** panics in debug builds, generic code, whole-array arithmetic and integer
  `sum`, `dot` and `@` included, and wraps with `--release`
  ([decision 0030](docs/decisions/0030-integer-overflow.md)).

### New in the standard library

Every library below is written in Baryon or the runtime from its specification, with no outside
library at run time, and keeps the Python or NumPy names its users already know.

- **Core**: the rest of Python's `math` for real numbers, `math.fsum`, and
  [`f64x2`](docs/decisions/0051-double-double-floats.md), a double-double float of about 32
  digits; `str` methods, indexing and slicing, `int(text)` and `float(text)` raising
  `ValueError`; lists of any type with slices and comprehensions; `dict`
  ([decision 0035](docs/decisions/0035-dict.md)); `sorted`, `round(x, digits)`, `min`, `max` and
  `sum` of lists.
- **[Files and processes](docs/reference/files-and-processes.md)**
  ([decision 0036](docs/decisions/0036-files-and-processes.md)): `sys.argv`, `sys.exit`, the
  environment, read only; text files with `open`, `with` and `for line in f`; the standard streams
  and `input`; `pathlib.Path` and the directory functions of `os`; `subprocess.run` with no shell;
  one `OSError` with `os.ErrorKind`.
- **[Linear algebra and FFT](docs/reference/linear-algebra.md)**
  ([decision 0040](docs/decisions/0040-linear-algebra-and-fft.md)): `linalg.solve`, `lu_factor`,
  `lu_solve`, `det`, `inv`, `cholesky`, `qr`, `eigh`, `eigvalsh`, `svd`, `svdvals`, `eig` and
  `eigvals`, at `f64` and most at `f32`, raising `LinAlgError`; `fft`, `ifft`, `rfft`, `irfft`,
  `fftfreq`, `rfftfreq` and the two-dimensional transforms.
- **[Statistics](docs/reference/statistics.md)** ([decision
  0042](docs/decisions/0042-statistics.md)): `stats.median`, `quantile`, `percentile`, `average`,
  `histogram`, `cov` and `corrcoef`, with NumPy's bits.
- **[Special functions](docs/reference/special-functions.md)**
  ([decision 0054](docs/decisions/0054-scipy-module-names.md)): a top-level `special` module with
  SciPy's gamma family (`gamma`, `gammaln`, `loggamma`, `digamma`, `polygamma`, `beta`, `poch` and
  more) and error functions (`erf`, `erfc`, `erfcx`, `erfinv`, `ndtr`, `ndtri`, `dawsn` and more),
  tested against high-precision values. The rest of SciPy's special functions follow (roadmap
  3.18).
- **[Data files](docs/reference/data-files.md)**: text tables with `Array.loadtxt` and `savetxt`,
  raw arrays with `Array.fromfile` and `a.tofile`, NumPy's `.npy` with `Array.load` and `save`
  ([decision 0043](docs/decisions/0043-array-data-files.md)); FITS images and headers with
  `fits.getdata`, `getheader` and `writeto` ([decision
  0044](docs/decisions/0044-fits-images-and-headers.md)); NetCDF classic files read and written
  through `netCDF4.Dataset`, and NetCDF-4 files read, compressed variables included, ([decision
  0047](docs/decisions/0047-netcdf-classic-files.md)); HDF5 files read through `h5by.File`
  ([decision 0052](docs/decisions/0052-hdf5-files.md)).
- **[DataFrame](docs/reference/dataframes.md)** ([decision 0045](docs/decisions/0045-dataframe.md)):
  a table whose schema is part of its type, `DataFrame[S]`, built from arrays or rows, with
  number, `bool` and text columns; rows chosen by mask, slice, `head` and `tail`; columns chosen,
  assigned, dropped and renamed; `sort_values`, `groupby(...).agg(...)`, the inner `merge`,
  `concat`, `drop_duplicates`, `value_counts` and `describe`; printed as pandas prints it.
- **[Plotting](docs/reference/plotting.md)** ([decision 0046](docs/decisions/0046-plotting.md)):
  `import byplot as plt` with matplotlib's names: line and scatter plots, bars, histograms, error
  bars, filled areas, images with colormaps and colorbars, contours, logarithmic axes, grids of
  plots, legends and text, saved as SVG, PNG or PDF.
- **[Compression](docs/reference/compression.md)**
  ([decision 0049](docs/decisions/0049-compression.md)): `zlib` and `gzip` in both directions,
  `zlib.decompressobj`, `compression.zstd` and `bz2` in both directions, and `lzma.decompress`.

### Faster programs and faster builds

- **Faster compiled code**: whole-array temporaries fused into the statement that reads them,
  arrays updated in place, bounds checks made once per loop, unit-stride inner loops, number locals
  kept in registers, and `@` run row by row. The [benchmark suite](bench/README.md) compares each
  kernel with its C++ and Fortran twin; the slowest is now within about 1.5 times of
  `gfortran -O3`.
- **Faster builds** ([decision 0048](docs/decisions/0048-debug-builds-compile-fast.md)): debug
  builds compile at `-O0`, in parallel modules, linked with `lld` where it is installed;
  `--timings` prints each stage's time. [`baryon watch`](docs/reference/commands.md#baryon-watch)
  keeps the package loaded and rebuilds it on each save, reusing the machine code of what did not
  change ([decision 0050](docs/decisions/0050-incremental-builds.md)).

### Learning Baryon

The [beginner's guide](docs/guide/README.md) is now the first of four pages of "How to code in
Baryon": a [language tour](docs/guide/language-tour.md),
[arrays and numbers](docs/guide/arrays-and-numbers.md), and
[data, files and plots](docs/guide/data-files-and-plots.md). CI builds and runs every example in
them and checks what it prints, so they cannot drift from the compiler.

### Editor support

`baryon lsp` shows a name's type and documentation on hover, goes to a definition across a
package's modules, completes names, members, fields and methods, shows inlay hints for inferred
types, and renames a name everywhere it is used. The [VS Code extension](editors/vscode/README.md),
now version 0.3.0, runs one test from the editor with `baryon test -k`, and the
[setup guide](docs/guide/vscode.md#10-debug-it) shows how to debug a program from VS Code.

### Fixed

- Double frees of a dict key read from a variable and of tuple parts (#175, #176), and the other
  memory-safety bugs found by the new ownership sweep, which builds every kind of owned value
  through every place a program can own, borrow, move or drop one and runs it under Valgrind on
  Linux in CI (#306, #311, #315, #352, #368, #375).
- An `InOut` argument must now have exactly its parameter's type: a `str` no longer passes as an
  `InOut[str | None]`, which the function could set to `None`
  ([values and references](docs/reference/values-and-references.md)). This closes a memory-layout
  bug (#411).
- A conditional expression that is only lent, such as `f(a if c else b)` where one branch names a
  variable whose value moves, is now a compile error instead of a use-after-free (#368).

### Decisions made for this release

Decisions 0031 to 0061 were accepted between the two releases; the
[decision index](docs/decisions/README.md) lists each with its status. Those that shape what comes
next, not built yet: operators on your own types through Python's dunder methods
([0058](docs/decisions/0058-operators-are-dunder-methods.md)), physical units checked at compile
time first ([0059](docs/decisions/0059-units-compile-time-first.md)), C++ interop as a C-shaped
subset ([0056](docs/decisions/0056-cpp-interop-is-a-c-shaped-subset.md)), TLS from the operating
system ([0060](docs/decisions/0060-tls-from-the-operating-system.md)), and AstroBaryon shipped with
Baryon ([0061](docs/decisions/0061-astrobaryon-ships-with-baryon.md)). The VS Code extension stays
a `.vsix` attached to each release, not on the Marketplace
([0057](docs/decisions/0057-vscode-extension-stays-a-vsix.md)).

### Known limits

- [What builds today](docs/reference/status.md) lists what `baryon check` accepts that the compiler
  does not build yet, such as CSV and Parquet files, missing values in tables, writing HDF5, and
  most of the libraries SciPy users expect beyond `special`. Each is reported as an error that names
  it, never miscompiled.
- Two bugs found while checking the documentation stay open: in a `for` loop over a list of paths,
  reading `entry.name` gives a false "moved" error (roadmap 1.9.31), and a `try` cannot reuse one
  name in two `except ... as error` clauses, which Python allows (1.9.32).
- No packages outside the standard library, no interop with Python or C++, and no REPL yet.
- Speed: the benchmark kernels run within about 1.5 times of `gfortran -O3`; within 5% is the
  Phase 1 gate, still ahead.

## 0.1.0: review release, 2026-11-09

The first release, made so that people outside the project can try the language and review its
documentation, code and design. It is a prototype: `baryon check` knows the whole of version 0 of
the language, but the compiler builds only part of it into programs. Reports of anything wrong,
unclear or missing are the point of this release: please
[open an issue](https://github.com/Baryon-Lang/Baryon/issues), and paste what
`baryon --version` prints into it.

### Where to start

- [Beginner's guide](docs/guide/README.md): install `baryon`, run a first program, let the
  compiler catch a mistake, write tests, and simulate a mass on a spring.
- [Coding Baryon in VS Code](docs/guide/vscode.md): set up the editor and write, run and test a
  first program in it.
- [What builds today](docs/reference/status.md): exactly which parts of the language this release
  compiles.
- [Language reference](docs/reference/README.md) and
  [differences from Python](docs/reference/differences-from-python.md): the language itself.
- [Decision records](docs/decisions/README.md): why it is designed this way.

CI builds and runs every program in both guides exactly as written, on macOS (Apple silicon) and
Linux.

### The `baryon` command

[The `baryon` command](docs/reference/commands.md) documents each one.

| Command | What it does |
| --- | --- |
| `baryon check <directory>` | Reports every syntax, module, name, type, ownership and parallel-loop error in a package, with Rust-style messages |
| `baryon build [--release] <directory> <output>` | Compiles a package through LLVM and `clang` into a native executable that starts at `main` in `main.byn` |
| `baryon run [--release] <directory>` | Builds and runs a package in one step |
| `baryon test [--release] <directory>` | Runs every `@test` function and reports which passed, cargo-style |
| `baryon doc <directory> <output>` | Writes the package's documentation strings as Markdown pages |
| `baryon lsp` | A language server that publishes `baryon check`'s diagnostics as the user types |
| `baryon --version`, `baryon --help` | Print the release, `baryon 0.1.0`, and the usage line |

### What compiles

[What builds today](docs/reference/status.md) is the full list. In short:

- Numbers of every width with Python's mixing rules, `bool`, `str` with f-strings and Python's
  escapes, tuples, optional values `T | None`, structs, enums, `match`, `Final` constants, and
  constants and functions read through their module.
- Generic functions, compiled once per type they are called at, and functions passed as
  arguments to a `Callable` parameter, compiled once per function passed so calls inline.
- Lists of numbers and other copy types: `[a, b]`, `append`, `len` and bounds-checked indexing.
- The `math` functions and constants (`sqrt`, `exp`, `log`, `sin`, `cos`, `pow`, `pi`, `e`,
  `tau`, `inf`, `nan`), `abs`, `round`, and the reproducible random number generator
  `random.Philox` with `uniform()`.
- Ownership as the reference describes it: moves, copies, read-only borrows, `InOut` and `Owned`
  parameters, and every value destroyed on every path out of a function.
- Errors: `Raises`, `raise`, `try` and `except`, and panics that report their location and a
  backtrace of `.byn` lines.
- Arrays: `Array` on the heap and `Fixed` inline, `Array(values)` from a list, bounds-checked
  indexing, `shape` and `size`, whole-array arithmetic, and a pairwise floating-point `sum` that
  matches NumPy.
- Parallel and vector loops: `prange` over a thread pool, with inferred reductions that combine
  in a fixed order, so results are reproducible; `accumulate=` for loops that add into shared
  arrays, including struct fields such as `atoms.forces`; and `simd` loops, which ask LLVM to
  vectorize.
- All five [example programs](docs/examples/README.md) build and run: a heat equation solver,
  Lennard-Jones molecular dynamics, the Lorenz system with Runge-Kutta, conjugate gradient, and
  Monte Carlo estimation of pi. A test checks what each prints against an independent NumPy run
  of the same steps, and they agree to every printed digit.

### Fixed during review testing

Testing the release before the tag turned up these, all fixed (roadmap 1.9.15):

- `baryon --version` (or `-V`) and `baryon --help` (or `-h`, `help`) print the release and the
  usage line, with exit status 0 (#116).
- Clearer checker errors: a method or member that does not exist on `str`, a number or a tuple
  is reported by `baryon check` instead of failing at build, and `y = None` followed by `y = 3`
  suggests declaring `y` optional (#116).
- A module-level `N = 10` gets the module-level variable error with a note to write `Final`
  (#114).
- `abs` builds, with Python's results for `-0.0` and `nan`; `a.size` and `a.shape[k]` build
  (#114).
- Methods of a generic struct and `float("nan")` are reported as not built yet instead of failing
  inside the backend or `clang` (#114).
- `Array.zeros` of a `Copy` struct zeroes every field; `zeros` and `ones` of an enum, whose
  members are not numbers, are reported instead of picking a member (#112).
- String literals and f-string text take Python's backslash escapes and raw strings (#115).
- A literal assigned to a field or element takes that place's type, so `p.w = 1.5` works on an
  `f32` field (#115).
- Constants and enum members read through their module, `units.G` and `units.Phase.GAS`, build
  (#115).

### Editor support

The [VS Code extension](editors/vscode/README.md) (version 0.2.0) opens `.byn` files as Baryon,
highlights them with VS Code's Python grammar plus Baryon's own types and loop functions, and
runs `baryon lsp` so errors are underlined as you type. It installs from a `.vsix` file; it is not
on the Visual Studio Marketplace (roadmap decision D.16). Any other editor with a Language Server
Protocol client can run `baryon lsp` directly.

### Known limits

- Not built yet, though `baryon check` accepts them: complex numbers, array and list slices,
  `for` over an array or list, the matrix product `@`, `==`, `in` and `len` on `str`, `**` on
  integers, lists of non-copy types such as `list[str]`, methods of generic structs, and
  `print` of tuples, arrays and structs. [What builds today](docs/reference/status.md) has the
  full list. Each is reported as an error that says it is not built yet, never miscompiled.
- Integer overflow panics in a debug build, the default, and wraps around with `--release`
  ([decision 0030](docs/decisions/0030-integer-overflow.md)). Some operations are not checked
  yet and wrap in both, such as arithmetic in generic functions and integer `a.sum()`; the panic
  names the operator but not its operands. [What builds today](docs/reference/status.md) lists
  them.
- The language server reports diagnostics only: no go to definition, hover or completion yet
  (roadmap 1.16).
- Speed is not yet compared with the C++ and Fortran baselines in [`bench/`](bench/README.md).
  Running within 5% of Fortran is the Phase 1 gate, still ahead.
- Tested on macOS (Apple silicon) and Linux. Windows is untested.
- Open design questions that may change the language are listed under
  [Decisions needed](ROADMAP.md#decisions-needed) in the roadmap.
