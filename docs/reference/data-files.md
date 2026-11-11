# Data files

[What builds today](status.md#data-files) says which parts of this chapter the compiler builds
into programs yet.

Data comes in files other programs wrote and goes out in files other programs read. Baryon reads
and writes them with NumPy's names and NumPy's bytes, as `Array` functions beside
[`Array.fromfile`](files-and-processes.md#array-files)
([decision 0043](../decisions/0043-array-data-files.md)): a program moved from NumPy changes
`numpy.` to `Array.` and gets the same numbers. Roadmap row 3.14 adds the formats one at a time,
those that need no outside library first.

| Format | Read | Write |
| --- | --- | --- |
| [Text tables](#text-tables) of numbers: CSV, whitespace-separated columns | `Array.loadtxt(fname)` | `Array.savetxt(fname, X)` |
| [`.npy` files](#npy-files), NumPy's own format | `Array.load(file)` | `Array.save(file, arr)` |
| Raw binary arrays | [`Array.fromfile(path, dtype)`](files-and-processes.md#array-files) | [`a.tofile(path)`](files-and-processes.md#array-files) |

## Text tables

A text table holds one row of numbers per line, the numbers split by whitespace or by a
delimiter such as a comma. `Array.loadtxt` reads one into an array, and `Array.savetxt` writes
an array as one. Both read and write exactly what `numpy.loadtxt` and `numpy.savetxt` do: a file
either writes, the other reads, and the same array gives the same bytes.

```python
def main() -> Raises[None, OSError | ValueError]:
    run = Array.loadtxt("run.csv", delimiter=",", skiprows=1)
    energy = run[:, 1].copy()
    print(run.shape, energy.sum() / len(energy))
    Array.savetxt("energy.txt", energy, header="energy in eV")
```

With `run.csv` holding the lines `time,energy`, `0.0,-1.5` and `0.5,-1.25`, this prints
`(2, 2) -1.375` and writes `energy.txt` as NumPy would:

```text
# energy in eV
-1.500000000000000000e+00
-1.250000000000000000e+00
```

### Reading: `Array.loadtxt`

| Argument | Default | Means |
| --- | --- | --- |
| `fname` | | the file's path, a `str` |
| `dtype` | `f64` | the element type; `f64` is the only one so far |
| `comments` | `"#"` | the text that starts a comment, which runs to the end of the line; `""` for none |
| `delimiter` | `""` | the text between two numbers; `""` means any run of whitespace |
| `skiprows` | `0` | how many lines to skip first, whatever they hold |
| `max_rows` | `-1` | how many rows to read at most; a negative number means all |

`fname` and `dtype` may be given by position; the rest are given by keyword.

- **The rank comes from the context.** The result is an `Array[f64, 2]`, one row per line, as
  NumPy's `ndmin=2` gives, unless the context needs a rank-1 array:
  `x: Array[f64, 1] = Array.loadtxt("x.txt")`. A rank-1 result reads a file of one column or one
  row, as NumPy squeezes it; a file of more rows and columns raises `ValueError`. NumPy decides
  the rank from the file, which a compiled program cannot know before it runs.
- **Lines are read as NumPy reads them.** After the first `skiprows` lines, everything from
  `comments` to the end of a line is dropped, and a line with nothing left is skipped. Each field
  is read with [`float(text)`](types.md#indexing-slicing-and-methods) after its whitespace is
  stripped, so `inf`, `-Infinity` and `nan` are numbers, and `0x10` and `1_000` are not.
- **Bad data raises `ValueError`, with NumPy's message.** A field that is not a number gives
  `could not convert string 'x' to float64 at row 1, column 2.`, counting data rows from 0 and
  columns from 1 as NumPy does. A row with another number of columns than the first gives `the
  number of columns changed from 3 to 2 at row 2; use ...`. A file that cannot be opened or read
  raises [`OSError`](files-and-processes.md#errors-oserror).
- **A file with no data** gives an array of shape `(0, 1)`, as NumPy's does, or `(0,)` at rank 1.
- **Cost.** The file is read one line at a time. The numbers are kept in a list until the last
  line, then copied into the array, so for a moment a table of n numbers takes about 2n numbers
  of memory.

### Writing: `Array.savetxt`

| Argument | Default | Means |
| --- | --- | --- |
| `fname` | | the file's path; the file is made, or emptied when it exists |
| `X` | | the array, an `Array[f64, 2]` written one row per line, or an `Array[f64, 1]` written one number per line |
| `delimiter` | `" "` | the text between two numbers in a row |
| `newline` | `"\n"` | the text that ends each line |
| `header` | `""` | text written before the table, each of its lines behind `comments` |
| `footer` | `""` | text written after the table, the same way |
| `comments` | `"# "` | the text put before each line of `header` and `footer` |

`fname` and `X` may be given by position; the rest are given by keyword. Every number is written
as NumPy's default format, `%.18e`, writes it: 19 significant digits, enough to read the same
`f64` back. A failure to write raises [`OSError`](files-and-processes.md#errors-oserror).

### Not built yet

The first version reads and writes `f64` only. NumPy's other keywords, `usecols`, `unpack`,
`converters`, `ndmin`, `encoding`, `quotechar` and `like` for `loadtxt`, and `fmt` and `encoding`
for `savetxt`, are compile errors that name the keyword, never ignored. Until they come, `open`,
the `str` methods and f-strings do the same work by hand
([Text files](files-and-processes.md#text-files)). `comments=None` and `delimiter=None`, which
NumPy accepts, are written `comments=""` and `delimiter=""`.

## `.npy` files

A `.npy` file is NumPy's own format for one array: a short header that names the element type
and the shape, then the elements as they lie in memory. `Array.save` writes one and
`Array.load` reads one, byte for byte as `numpy.save` and `numpy.load` do, so an array moves
between Baryon and NumPy with its type and shape intact.

```python
def main() -> Raises[None, OSError]:
    grid = Array.zeros((64, 64))
    grid[32, 32] = 1.0
    Array.save("heat.npy", grid)
    again: Array[f64, 2] = Array.load("heat.npy")
    print(again.shape, again[32, 32])
```

This prints `(64, 64) 1.0`, and `numpy.load("heat.npy")` gives the same 64 by 64 array of
`float64`.

| Call | Gives | Does |
| --- | --- | --- |
| `Array.save(file, arr)` | `None` | makes the file, or empties the one there, and writes NumPy's header, then every element of `arr` in row-major order; a view writes the elements it sees |
| `Array.load(file)` | `Array[T, rank]` | reads the file into a new array, with the shape the file names |

- **The element type and rank come from the context**, as they do for `Array.zeros`:
  `grid: Array[f64, 2] = Array.load("heat.npy")`. With no context the result is an
  `Array[f64, 1]`. The file names its own type and shape, and `Array.load` checks them: a file
  of another element type or rank raises `OSError` with the kind `INVALID_DATA`, saying what the
  file holds, where NumPy would hand back whatever the file holds.
- **Elements are numbers, `bool`s or complex numbers**, each written with NumPy's name for it:
  `'<f8'` for `f64`, `'<i4'` for `i32`, `'|b1'` for `bool`, `'<c16'` for `complex128`. Arrays
  of structs need NumPy's structured types, which roadmap 3.25 adds.
- **Files NumPy writes are read**: format versions 1.0, 2.0 and 3.0, little-endian, in row-major
  order. A file in Fortran order, of big-endian numbers, of records or of Python objects raises
  `OSError` with the kind `INVALID_DATA`; `numpy.ascontiguousarray(a).astype("<f8")` makes one
  Baryon reads. `.npz` archives, which hold several arrays zipped, are not read yet.
- **Cost.** One pass over the bytes, straight into or out of the array's memory, as
  [`Array.fromfile`](files-and-processes.md#array-files) reads and `a.tofile` writes. The header
  is 128 bytes for most arrays.
