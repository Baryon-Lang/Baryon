# 0043. Text tables and `.npy` files are read and written with NumPy's names, as `Array` functions

Status: Proposed
Date: 2026-11-11
Roadmap: 3.14.1, 3.14.2

## Context

Roadmap 3.14 reads and writes data file formats. The two a physicist or data scientist meets
first need no outside library: a text table of numbers, one row per line with the columns split
by spaces or commas, and NumPy's own binary format, `.npy`. NumPy reads and writes them with
`numpy.loadtxt` and `numpy.savetxt`, and `numpy.load` and `numpy.save`. Their names and their
files are what users of Baryon already know, and NumPy's output is what tests compare against.

Baryon already has NumPy's raw array files: `Array.fromfile(path, dtype)` and `a.tofile(path)`
([decision 0036](0036-files-and-processes.md), part 4). They follow a rule the language uses
for every NumPy function that makes an array: `numpy.zeros` is `Array.zeros`, `numpy.fromfile`
is `Array.fromfile`. These four functions need a home, and they need answers to two questions
NumPy settles at run time but a compiled language must settle when compiling: the rank of the
array read, and its element type.

Two homes were considered:

- **`Array` functions**, `Array.loadtxt(path)`, `Array.savetxt(path, a)`, `Array.load(path)`
  and `Array.save(path, a)`, beside `Array.fromfile`. A program moved from NumPy changes
  `numpy.` to `Array.` and nothing else, as it already does for `zeros` and `fromfile`, and every
  function that reads or writes an array file is in one place.
- **A module**, `from io import loadtxt, savetxt`, as `scipy.io` holds SciPy's file formats.
  It needs an import, splits array files across two places, and takes a name Python's own `io`
  module gives another meaning (`io.StringIO`, `io.open`).

## Decision

1. **They are `Array` functions with NumPy's names and argument order:**
   `Array.loadtxt(path, ...)`, `Array.savetxt(path, a, ...)`, `Array.load(path)` and
   `Array.save(path, a)`. Each keyword means what NumPy's does. A keyword NumPy has that is not
   built yet is a compile error that names it, never ignored.
2. **The rank comes from the context, as an element type does.** `Array.loadtxt(path)` gives an
   `Array[f64, 2]`, a table, unless the context needs a rank-1 array:
   `x: Array[f64, 1] = Array.loadtxt(path)`. A rank-1 result reads a file of one column or one
   row, as NumPy's squeezed result does; a table of more than one row and column raises
   `ValueError`. NumPy picks the rank from the file, which a compiled program cannot know.
3. **Text tables are read and written as NumPy does, to the byte.** `loadtxt` splits on runs of
   whitespace by default, or on `delimiter`; drops everything from `comments` (`"#"`) to the end
   of a line; skips `skiprows` lines and blank lines; and reads at most `max_rows` rows. Each
   field is read with `float(text)` ([decision 0037](0037-reading-numbers-from-text.md)).
   `savetxt` writes each number with NumPy's default format, `%.18e`, separated by `delimiter`
   (`" "`), one row per line, and writes `header` and `footer` behind `comments` (`"# "`). A
   file written by either reads in the other, and the same array gives the same bytes.
4. **Bad data raises `ValueError` with NumPy's message.** A field that is not a number gives
   `could not convert string 'x' to float64 at row 0, column 2.`, and a row with another number
   of columns gives `the number of columns changed from 2 to 1 at row 2; use ...`, word for word
   as NumPy 2 writes them. A file that cannot be opened, read or written raises `OSError`.
5. **`.npy` files carry their element type and shape, and both are checked.** `Array.save`
   writes NumPy's format version 1.0: the magic bytes, a header naming the type in NumPy's
   spelling (`'<f8'`), the shape and `'fortran_order': False`, then the elements in row-major
   order. `Array.load(path)` takes the element type and rank from the context, as `loadtxt`
   does, defaulting to `f64`, and raises `ValueError` when the file holds another type or rank,
   where NumPy would return whatever the file holds. It reads files NumPy writes in version 1.0,
   2.0 and 3.0, little-endian, in row-major order; anything else (Fortran order, big-endian,
   object arrays, `.npz`) raises `ValueError` saying what it found.
6. **Written in Baryon where Baryon can say it.** `loadtxt` and `savetxt` are Baryon source in
   the standard library ([decision 0038](0038-standard-library-in-baryon.md)), built on `open`,
   `for line in f`, `split` and f-strings. `.npy` needs bytes, which Baryon has no type for
   (decision 0036), so its header is read and written by the runtime in Rust, as
   `Array.fromfile` is.

## Consequences

- `numpy.loadtxt("run.csv", delimiter=",")` becomes `Array.loadtxt("run.csv", delimiter=",")`,
  and the two give the same numbers. A `.npy` file moves between Baryon and NumPy unchanged.
- The rank is in the program, so `data[:, 1]` of a one-column file is caught by the checker or
  by the `ValueError`, not by a wrong shape later.
- The first version reads and writes `f64` text tables; `.npy` takes every element type
  `Array.fromfile` takes. Other `loadtxt` element types, `usecols`, `unpack`, `converters` and
  `savetxt`'s `fmt` come later; until then each is a compile error that names it.
- Reading a table holds its numbers in a list before the array is made, so a file of n numbers
  costs about 2n numbers of memory for a moment; reading a `.npy` file costs the array alone.
- `Array` grows four more functions; nothing new is imported, and no name is taken from the
  program.
