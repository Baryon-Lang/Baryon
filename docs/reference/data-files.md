# Data files

[What builds today](status.md#data-files) says which parts of this chapter the compiler builds
into programs yet.

Data comes in files other programs wrote and goes out in files other programs read. Baryon reads
and writes arrays with NumPy's names and NumPy's bytes, as `Array` functions beside
[`Array.fromfile`](files-and-processes.md#array-files)
([decision 0043](../decisions/0043-array-data-files.md)): a program moved from NumPy changes
`numpy.` to `Array.` and gets the same numbers. FITS files keep AstroPy's names, in a `fits`
module ([decision 0044](../decisions/0044-fits-images-and-headers.md)), and NetCDF files keep
Unidata's `netCDF4` names ([decision 0047](../decisions/0047-netcdf-classic-files.md)). Roadmap
row 3.14 adds the
formats one at a time, those that need no outside library first.

| Format | Read | Write |
| --- | --- | --- |
| [Text tables](#text-tables) of numbers: CSV, whitespace-separated columns | `Array.loadtxt(fname)` | `Array.savetxt(fname, X)` |
| [`.npy` files](#npy-files), NumPy's own format | `Array.load(file)` | `Array.save(file, arr)` |
| [FITS images and headers](#fits-files), astronomy's format | `fits.getdata(filename)`, `fits.getheader(filename)` | `fits.writeto(filename, data, header)` |
| [NetCDF classic files](#netcdf-files), the format of climate and weather data | `Dataset(filename).variables[name][:]` | `ds.variables[name][:] = values` |
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
([Text files](files-and-processes.md#text-files)). NumPy also accepts `comments=None` and
`delimiter=None`; a `str` argument cannot be `None` yet, so for now they are written
`comments=""` and `delimiter=""`.

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

## FITS files

FITS, the Flexible Image Transport System, is the file format of astronomy: telescope images,
spectra and catalogues arrive as FITS. A FITS file is a list of HDUs, header and data units. Each
HDU's header is 80-character text cards, such as `EXPTIME =                 30.0 / seconds`, and
its data is an image of big-endian numbers. The `fits` module reads and writes images and their
headers with the names of AstroPy's `astropy.io.fits`, byte for byte as AstroPy does
([decision 0044](../decisions/0044-fits-images-and-headers.md)).

```python
import fits


def main() -> Raises[None, OSError | KeyError]:
    image: Array[f32, 2] = fits.getdata("m31.fits")
    header = fits.getheader("m31.fits")
    exposure: float = header["EXPTIME"]
    image /= f32(exposure)
    header["BUNIT"] = ("counts / s", "rate")
    fits.writeto("m31_rate.fits", image, header, overwrite=True)
```

This reads an image as `f32`s, divides it by the exposure time its header gives, and writes it
with the header and a new card, as the same lines with `from astropy.io import fits` would.

| Call | Gives | Does |
| --- | --- | --- |
| `fits.getdata(filename, ext=0)` | `Array[T, rank]` | reads the image of HDU `ext`, the primary HDU 0, into a new array |
| `fits.getheader(filename, ext=0)` | `fits.Header` | reads the cards of HDU `ext` |
| `fits.writeto(filename, data, header=None, overwrite=False)` | `None` | writes `data` as the image of a new file, with `header`'s cards after the ones the image needs |

- **The image's element type and rank come from the context**, as `Array.load`'s do:
  `image: Array[f32, 2] = fits.getdata(path)`. With no context the result is an
  `Array[f64, 2]`. The shape is the FITS axes reversed, as AstroPy gives it, so `image[y, x]` is
  the pixel at `NAXIS1 = x + 1`, `NAXIS2 = y + 1`.
- **Pixels are converted as AstroPy converts them.** A float array reads any image, with
  `BSCALE` and `BZERO` applied in its own precision: an `Array[f32, 2]` of a 16-bit image gives
  the `float32` values AstroPy gives. An integer array reads an image stored as its type, or an
  unsigned one stored with AstroPy's `BZERO` shift, as `Array[u16, 2]` reads a `uint16` image.
  An image another type cannot hold, of another rank, or an HDU with no image raises `OSError`
  with the kind `INVALID_DATA`, saying what the file holds.
- **Images hold integers or floats**: `u8`, `i8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32`
  and `f64`. `fits.writeto` writes each with AstroPy's `BITPIX` and, for the unsigned types and
  `i8`, its `BZERO`, so AstroPy reads back the array that was written.
- **Writing never replaces a file by accident**: a file already there raises `OSError` with the
  kind `FILE_EXISTS` unless `overwrite=True`, as in AstroPy.
- **Cost.** Reading skips the data of the HDUs before the one asked for, then makes one pass
  over the pixels, converting them from big-endian as they are copied into the array. Writing
  makes one pass over the array.

### Headers

A `fits.Header` holds an HDU's cards, in order, with their comments. Its values are read and set
as AstroPy's are; keywords match in any case.

| Code | Gives | Does |
| --- | --- | --- |
| `header[key]` | the type the context needs | the card's value, as a `str`, an `int`, a `float` or a `bool` |
| `header.get(key, default)` | the type of `default` | the card's value, or `default` when there is no card `key` |
| `key in header`, `key not in header` | `bool` | whether there is a card `key` |
| `header[key] = value` | | sets the card's value, keeping its comment, or adds the card at the end |
| `header[key] = (value, comment)` | | sets the card's value and comment |

- **A value's type comes from where it goes**, since a card can hold text, an integer, a float
  or a logical value, and a Baryon value has one type:
  `exposure: float = header["EXPTIME"]`, `name: str = header["OBJECT"]`. A `float` reads an
  integer card too, as Python's numbers mix. Where nothing gives a type, as in
  `print(header["EXPTIME"])`, the checker asks for one.
- **A missing card raises `KeyError`** with AstroPy's message, `Keyword 'EXPTIME' not found.`,
  and so does a card of another type, such as a `str` read as a `float`. A program handles it, or
  reads with `get` and a default.
- **Values are written as AstroPy writes them**: a string in quotes, at least 8 characters,
  a float as Python's shortest text for it, right-justified to column 30, so a file written
  from a header matches AstroPy's byte for byte.
- **The header of a change, or of `in`, is a variable**, such as `header` here. A header kept
  in a struct's field is read with `header[key]` but changed through a variable.

### Not built yet

`fits.open` and its list of HDUs, binary and ASCII tables, compressed images, memory maps, a
`(data, header)` pair from `getdata(header=True)`, a new empty header from `fits.Header()`,
string values longer than one card (the `CONTINUE` convention) and complex values in cards are
roadmap 4.2.5's or come later. Each is an error that names it, never silently ignored.

## NetCDF files

NetCDF is the file format of climate, weather, ocean and much of earth science: model output,
reanalyses and satellite products arrive as `.nc` files. A NetCDF file holds named dimensions,
named variables, each an array over some of the dimensions, and attributes, small named values
on the file or on a variable such as `units = "K"`. The `netCDF4` module reads and writes the
classic formats, CDF-1 and the 64-bit offset CDF-2, with the names of Unidata's `netCDF4`
package ([decision 0047](../decisions/0047-netcdf-classic-files.md)). One dimension may be
unlimited: the records, usually time, that a model appends as it runs.

```python
from netCDF4 import Dataset


def main() -> Raises[None, OSError | KeyError | AttributeError]:
    ds = Dataset("air.nc")
    air: Array[f32, 3] = ds.variables["air"][:]
    units: str = ds.variables["air"].getncattr("units")
    steps = ds.dimensions["time"].size
    print(f"{steps} steps, mean {air.mean()} {units}")
    ds.close()
```

This reads a variable over `(time, lat, lon)` as `f32`s, its units and the number of time steps,
as the same lines would in Python.

| Code | Gives | Does |
| --- | --- | --- |
| `Dataset(filename, mode="r")` | `Dataset` | opens the file and reads its header |
| `ds.variables[name]` | `Variable` | the variable `name` |
| `var[:]` | `Array[T, rank]` | reads every value of the variable into a new array |
| `var[0, :, ::2]` | `Array[T, rank]` | reads the values a subscript picks, as `netCDF4` picks them |
| `ds.dimensions[name]` | `Dimension` | the dimension `name`: its `name`, its `size` and `isunlimited()` |
| `ds.getncattr(name)`, `var.getncattr(name)` | the type the context needs | an attribute of the file or of a variable |
| `ds.ncattrs()`, `var.ncattrs()` | `list[str]` | the names of the attributes, in the file's order |
| `var.name`, `var.dimensions` | `str`, `list[str]` | the variable's name and its dimensions' names, slowest first |
| `ds.close()` | `None` | closes the file, as `netCDF4` asks |

- **A variable's element type and rank come from the context**, as `fits.getdata`'s do:
  `air: Array[f32, 3] = ds.variables["air"][:]`. With no context the result is an
  `Array[f64, 2]`. The axes are in the file's order, as `netCDF4` gives them, the unlimited one
  first. A variable of another rank, or one whose values the type cannot hold, such as `i4`
  values in an `Array[i16, 1]`, raises `OSError` with the kind `INVALID_DATA`. A classic file
  holds `i1`, `i2`, `i4`, `f4` and `f8` values, so an array of `i8`, `i16`, `i32`, `i64`, `f32` or
  `f64` reads them, and text variables are not read yet.
- **Packed values are unpacked as `netCDF4` unpacks them.** A float array of a variable with
  `scale_factor` or `add_offset` gets `value * scale_factor + add_offset`, in `float32` where
  NumPy computes in `float32`, so the numbers match `netCDF4`'s bit for bit. Reading a packed
  variable into an integer array raises `OSError`.
- **An attribute's type comes from where it goes**: `units: str = var.getncattr("units")`,
  `version: int = ds.getncattr("version")`. A `float` reads an integer attribute too. Where
  nothing gives a type, as in `print(ds.getncattr("title"))`, the checker asks for one.
- **A missing name raises what `netCDF4` raises**: `KeyError` for a variable or a dimension,
  with the name quoted, `'pressure'`, and `AttributeError` for an attribute, with `netCDF4`'s
  message, `NetCDF: Attribute not found`. An attribute that does not hold the type read, such
  as text read as a `float` or an attribute of two numbers, raises `AttributeError` too.
- **A subscript picks part of a variable** as `netCDF4` and NumPy pick it: an integer takes one
  position and drops its axis, counting from the end when negative, a slice `start:stop:step`
  takes the positions Python's slices take and keeps its axis, and the axes after the last item
  are taken whole. `temp[0, :, ::2]` of a variable over `(time, lat, lon)` is an
  `Array[T, 2]`. The rank in the program's type is the number of axes kept; another rank
  raises `OSError`. Items are integers or slices of integers, constant or not.
- **A subscript that picks nothing raises what `netCDF4` raises**: an index out of bounds
  raises `IndexError` with `netCDF4`'s message, `index 5 is out of bounds for the dimension time
  of length 2`, and a step of zero raises `ValueError`, `slice step cannot be zero`. So a
  subscripted read or write can fail with `OSError | IndexError | ValueError`; `var[:]` only
  with `OSError`.
- **Cost.** Opening reads the header only. `var[:]` reads the header again, then makes one pass
  over the variable's values, converting them from big-endian as they are copied into the
  array; a record variable is read one record at a time. A subscript reads only the stretches
  of the file it picks, one run of values along the last axis at a time.

### Differences from `netCDF4`

- **Fill values are not masked.** `netCDF4` gives a masked array, with each point that holds
  the `_FillValue` masked. Baryon has no masked arrays yet (roadmap D.21), so fill values are
  read as stored, as `netCDF4` gives them after `ds.set_auto_mask(False)`. A program tests for
  the fill value itself.
- **Types come from the program**: a variable is read as the array type it goes into, and an
  attribute as the type it goes into, where `netCDF4` gives NumPy's type for each.
- **Attributes are read with `getncattr`.** `netCDF4` also gives them as attributes, `ds.title`,
  which needs the name known only when the file is read; Baryon reads
  `ds.getncattr("title")`.
- **`var.dimensions` is a `list[str]`**, where `netCDF4` gives a tuple of the same names.
- **A subscript takes integers and slices.** `netCDF4` also takes arrays of indices, of
  booleans and `...`; those are compile errors so far.

### Writing a NetCDF file

`Dataset(path, "w", format=...)` makes a new classic file, replacing any file there, and the
same calls as in Python define and fill it:

```python
from netCDF4 import Dataset


def main() -> Raises[None, OSError]:
    ds = Dataset("grid.nc", "w", format="NETCDF3_CLASSIC")
    ds.setncattr("title", "test grid")
    ds.createDimension("time", None)
    ds.createDimension("lat", 2)
    temp = ds.createVariable("temp", "f4", ("time", "lat"))
    temp.setncattr("units", "K")
    values: Array[f32, 2] = Array.zeros((3, 2))
    temp[:] = values
    ds.close()
```

This writes three records of a two-point grid, and its bytes are the bytes `netCDF4` writes for
the same calls, fill values and padding included; the tests compare them.

| Code | Gives | Does |
| --- | --- | --- |
| `Dataset(filename, "w", format=...)` | `Dataset` | makes a new file: `format` is `"NETCDF3_CLASSIC"` or `"NETCDF3_64BIT_OFFSET"` |
| `Dataset(filename, "a")` or `"r+"` | `Dataset` | opens a file to add to it |
| `ds.createDimension(dimname, size=None)` | `Dimension` | a new dimension; `None` or `0` makes it the unlimited one |
| `ds.createVariable(varname, datatype, dimensions=())` | `Variable` | a new variable over the dimensions named, slowest first |
| `ds.setncattr(name, value)`, `var.setncattr(name, value)` | `None` | sets an attribute of the file or of a variable |
| `var[:] = values` | `None` | writes an array as all the variable's values |
| `var[1, ::2] = values` | `None` | writes an array as the values a subscript picks |
| `with Dataset(...) as ds:` | | binds `ds` for the block, as `ds = Dataset(...)` does |

- **Each call changes the file at once.** The file is the only state, so `ds.close()` and the
  end of a `with` block have nothing left to write, and a program that stops halfway leaves a
  valid file with what it defined and wrote so far. Each change writes the new file beside the
  old one and then renames it over it, so even a crash or a full disk in the middle of a change
  leaves the old file or the new one, never one cut short, and the file keeps its permissions.
  A variable is filled with its type's fill value until it is
  written. Each change and each write reads the whole file into memory and writes it again,
  which is quick for the small headers of classic files but makes many small definitions of a
  large file slow: define everything first, then write the values. The file must fit in
  memory, records added included: a write that would not fit raises `OSError` with the kind
  `INVALID_DATA`, or, on a system that promises more memory than it has, can end the program.
  Writing in place is idea I.53 on the roadmap.
- **A type is named as `netCDF4` names it** (`"f4"`, `"f8"`, `"i1"`, `"i2"`, `"i4"`) or as NumPy
  names or codes it (`"float32"`, `"f"`, `"int16"`, `"h"` and so on). `dimensions` is a tuple of
  names, or one name, and up to four of them so far.
- **An attribute is a `str`, an `int` or a `float`**, stored as `netCDF4` stores a Python value
  of that type: text, one 32-bit integer, or one 64-bit float. An integer too big for 32 bits
  raises `OSError` with the kind `INVALID_DATA`.
- **`var[:] = values` takes an array of signed integers or floats** of the variable's rank,
  converted to the variable's type as NumPy converts, and packed by `scale_factor` and
  `add_offset` as `netCDF4` packs. A record variable takes as many records as the array's first
  length, adding records to the file; every other length must be the dimension's.
  `ds.variables[name][:] = values` writes a variable found by name.
- **A subscript writes part of a variable**, picked as a read picks it. The array's rank is the
  number of axes the subscript keeps and its lengths the positions it picks. Along the
  unlimited dimension a write may go past the last record, as `netCDF4`'s may: `temp[5] = row`
  of a file of two records adds four, filling the ones not written, and a slice there with no
  stop, `temp[2:] = values`, takes as many records as the array has rows. A slice with a
  negative step along the unlimited dimension is refused with `OSError`, where `netCDF4` writes
  the wrong positions.
  Along any other dimension an index out of bounds raises `IndexError`, and a step of zero
  raises `ValueError`, as a read's does.
- **What the format refuses raises `OSError`** with the kind `INVALID_DATA` and `netCDF4`'s
  message: a name already in use, a second unlimited dimension, or the unlimited dimension
  anywhere but first.

### Not built yet

Listing `ds.variables` and `ds.dimensions`, attributes set as `var.units = "K"`, `fill_value=` and
compression in `createVariable`, subscripts of arrays of indices, booleans or `...`, text
variables, attributes of several values, CDF-5 files and
NetCDF-4 files, which are HDF5 files (roadmap 3.14.6), come later. Each is an error that names
it, never silently ignored.
