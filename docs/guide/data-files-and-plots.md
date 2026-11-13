# Data, files and plots

A scientific program reads data, computes and writes results. This page shows the libraries that
do the reading, writing and drawing: tables of data with `DataFrame`, text files and paths, the
data file formats of NumPy and the sciences, compression and plots. Each keeps the names of the
Python library it replaces, so a Python program changes its imports and little else. Every
program here builds and runs today, as CI checks. It assumes you have read the
[tour of the language](language-tour.md) and [Arrays and numbers](arrays-and-numbers.md).

| Python library | Baryon | Reference |
| --- | --- | --- |
| pandas | `DataFrame`, built in | [DataFrames](../reference/dataframes.md) |
| `open`, `os`, `pathlib`, `sys`, `subprocess` | the same names | [Files and processes](../reference/files-and-processes.md) |
| `numpy.loadtxt`, `numpy.save` and the rest | `Array.loadtxt`, `Array.save` | [Data files](../reference/data-files.md) |
| `astropy.io.fits` | `fits` | [FITS files](../reference/data-files.md#fits-files) |
| `netCDF4` | `netCDF4` | [NetCDF files](../reference/data-files.md#netcdf-files) |
| `h5py` | `h5by` | [HDF5 files](../reference/data-files.md#hdf5-files) |
| `zlib`, `gzip`, `bz2`, `lzma`, `compression.zstd` | the same names | [Compression](../reference/compression.md) |
| `matplotlib.pyplot` | `byplot` | [Plotting](../reference/plotting.md) |

A library adapted from a Python package whose name says `py` says `by` instead, as `h5by` and
`byplot` do; the names inside it stay the original's.

## 1. Tables of data

A `DataFrame` is a table: named columns of one length, each an array. Its columns are declared
by a struct, the *schema*, so a misspelled column is a compile error rather than a `KeyError` an
hour into a run.

File `stars/main.byn`:

```python
class Star(Struct):
    name: str
    cluster: str
    magnitude: f64
    distance: f64


class Cluster(Struct):
    cluster: str
    age: f64


def main():
    stars: DataFrame[Star] = DataFrame({
        "name": Array(["Alcyone", "Atlas", "Electra", "Hyades A", "Hyades B"]),
        "cluster": Array(["Pleiades", "Pleiades", "Pleiades", "Hyades", "Hyades"]),
        "magnitude": Array([2.87, 3.62, 3.70, 3.40, 3.53]),
        "distance": Array([136.2, 136.2, 136.2, 47.0, 47.0]),
    })
    print(stars)
    bright = stars[stars.magnitude < 3.6]
    print(bright.name)

    summary = stars.groupby("cluster").agg(
        brightest=("magnitude", "min"),
        members=("name", "count"),
    )
    print(summary)

    clusters: DataFrame[Cluster] = DataFrame({
        "cluster": Array(["Hyades", "Pleiades"]),
        "age": Array([625.0, 100.0]),
    })
    print(summary.merge(clusters, on="cluster").sort_values("age"))
    print(stars.value_counts("cluster"))
    print(len(concat([stars.copy(), stars]).drop_duplicates()))
```

```text
$ baryon run stars
       name   cluster  magnitude  distance
0   Alcyone  Pleiades       2.87     136.2
1     Atlas  Pleiades       3.62     136.2
2   Electra  Pleiades       3.70     136.2
3  Hyades A    Hyades       3.40      47.0
4  Hyades B    Hyades       3.53      47.0
['Alcyone' 'Hyades A' 'Hyades B']
    cluster  brightest  members
0    Hyades       3.40        2
1  Pleiades       2.87        3
    cluster  brightest  members    age
0  Pleiades       2.87        3  100.0
1    Hyades       3.40        2  625.0
    cluster  count
0  Pleiades      3
1    Hyades      2
5
```

The calls and the printed tables are pandas's. What differs:

- **A column is an array.** `stars.magnitude` is an `Array[f64, 1]` with every array operation,
  and `stars.name` an `Array[str, 1]`. There is no `Series` type.
- **There is no row index.** Rows are numbered from 0, and nothing else.
- **The compiler works out each result's columns.** `summary` has the columns `cluster`,
  `brightest` and `members`, so `summary.age` would be a compile error until the `merge` adds
  it.
- **A frame moves**, like an array: `concat` takes the frames it stacks, so the program passes a
  copy of `stars` to keep using it.
- Missing values, and the joins and file readers that need them, are not built yet
  ([What builds today](../reference/status.md#dataframes)).

[DataFrames](../reference/dataframes.md) and
[Sorting, grouping and joining](../reference/dataframe-operations.md) list every operation.

## 2. Files, data formats and compression

File `signal/main.byn`:

```python
from math import exp, sin
import os
from pathlib import Path
from zlib import compress, crc32, decompress


def main() -> Raises[None, OSError | ValueError]:
    out = Path("results")
    os.makedirs(out, exist_ok=True)

    # A decaying signal, sampled 101 times over 10 seconds.
    n = 101
    table = Array.zeros((n, 2))
    for i in range(n):
        t = 0.1 * f64(i)
        table[i, 0] = t
        table[i, 1] = exp(-0.3 * t) * sin(2.0 * t)

    # Text tables and .npy files, with NumPy's names and bytes.
    Array.savetxt(str(out / "signal.csv"), table, delimiter=",", header="t,x")
    back: Array[f64, 2] = Array.loadtxt(str(out / "signal.csv"), delimiter=",")
    print(back.shape, back[100, 0])
    Array.save(str(out / "signal.npy"), table)

    # Compress the text with zlib, as Python's zlib does.
    raw = Array.fromfile(str(out / "signal.csv"), u8)
    packed = compress(raw, 9)
    print(len(packed) < len(raw), crc32(decompress(packed)) == crc32(raw))

    # A text file, written and read back one line at a time.
    with open(str(out / "notes.txt"), "w") as notes:
        notes.write(f"samples: {n}\n")
        notes.write(f"peak: {back[:, 1].max():.4f}\n")
    with open(str(out / "notes.txt")) as notes:
        for line in notes:
            print(line.strip())
    print(sorted(os.listdir(out)))
```

```text
$ baryon run signal
(101, 2) 10.0
True True
samples: 101
peak: 0.7988
['notes.txt', 'signal.csv', 'signal.npy']
```

- **Reading and writing can fail**, so `main` declares `OSError`, and `ValueError` for a table
  that holds something other than numbers.
- **`Array.savetxt` and `Array.save` write NumPy's bytes**, so `numpy.loadtxt` and `numpy.load`
  read these files back, and the other way round.
- **Compression works on byte arrays**, `Array[u8, 1]`, since there is no `bytes` type yet.
  `zlib`, `gzip`, `bz2` and `compression.zstd` compress and decompress; `lzma` decompresses.
- `sys.argv`, `sys.exit`, `os.getenv` and `subprocess.run` work as in Python;
  [Files and processes](../reference/files-and-processes.md) has them.

## 3. Climate, simulation and astronomy files

NetCDF, HDF5 and FITS are read, and NetCDF and FITS written, by code written in Baryon, with no C
library to install. Here a program writes a NetCDF file and reads it back:

File `climate/main.byn`:

```python
from netCDF4 import Dataset


def main() -> Raises[None, OSError | KeyError | IndexError | ValueError | AttributeError]:
    with Dataset("air.nc", "w", format="NETCDF3_CLASSIC") as ds:
        ds.createDimension("time", None)
        ds.createDimension("lat", 3)
        temp = ds.createVariable("temp", "f8", ("time", "lat"))
        temp.setncattr("units", "K")
        values = Array.zeros((2, 3))
        values[0, :] = Array([271.5, 288.0, 299.25])
        values[1, :] = Array([270.0, 287.5, 300.0])
        temp[:] = values

    with Dataset("air.nc") as ds:
        temp = ds.variables["temp"]
        last: Array[f64, 1] = temp[1, :]
        units: str = temp.getncattr("units")
        print(temp.dimensions, ds.dimensions["time"].size, units, last.mean())
```

```text
$ baryon run climate
['time', 'lat'] 2 K 285.8333333333333
```

- **A value read from a file has the type its use gives it.** The file decides at run time
  what `temp[1, :]` and the attribute hold, so the program says what it expects, with
  `last: Array[f64, 1]` and `units: str`. A file that holds something else raises an error
  rather than giving wrong numbers.
- **`Dataset(path)` reads NetCDF-4 files too**, which are HDF5 files underneath; writing them is
  not built yet.

An HDF5 file reads with h5py's names through `h5by`, and a FITS image through `fits`, AstroPy's
names:

```python
import fits
import h5by


def main() -> Raises[None, OSError | KeyError | IndexError | ValueError]:
    with h5by.File("run.h5", "r") as f:
        temp: Array[f32, 3] = f["run1/temp"][...]
        print(f.keys(), temp.mean())
    image: Array[f32, 2] = fits.getdata("m31.fits")
    print(image.shape)
```

[Data files](../reference/data-files.md) covers each format, with what is read, what is written
and where each differs from its Python library.

## 4. Plots

`byplot` draws with matplotlib's names and matplotlib's default look, and saves SVG, PNG or PDF,
chosen by the file's extension. Import it as matplotlib users do, `as plt`:

File `plot/main.byn`:

```python
from math import exp, sin
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    n = 101
    t = Array.zeros(n)
    x = Array.zeros(n)
    for i in range(n):
        t[i] = 0.1 * f64(i)
        x[i] = exp(-0.3 * t[i]) * sin(2.0 * t[i])

    ax = plt.axes()
    ax.plot(t, x, label="x(t)")
    ax.set_xlabel("time (s)")
    ax.set_ylabel("displacement (m)")
    ax.legend()
    ax.savefig("signal.png")
    print("wrote signal.png")
```

```text
$ baryon run plot
wrote signal.png
```

The image is matplotlib's: a 640 by 480 pixel figure, the line in its blue, its ticks and its
DejaVu Sans labels.

- **`ax = plt.axes()`** takes the place of `fig, ax = plt.subplots()`, and `ax.savefig` of
  `fig.savefig`. A grid of plots is `fig = plt.subplots(2, 2)`, reaching each plot as
  `fig.axes[i]`.
- **There is no hidden current figure**: `plt.plot` and `plt.savefig` do not exist, so a
  matplotlib script that uses them fails to compile rather than drawing in the wrong place.
- `scatter`, `bar`, `hist`, `errorbar`, `fill_between`, `text`, `imshow` with colormaps and a
  colorbar, `contour` and `contourf`, logarithmic axes and legends all work, with matplotlib's
  keyword arguments.

[Plotting](../reference/plotting.md) lists every method and each difference from matplotlib.

## Where to go next

- [Coding Baryon in VS Code](vscode.md) sets up the editor, and `baryon watch` rebuilds a
  program each time you save it ([The `baryon` command](../reference/commands.md#baryon-watch)).
- [Differences from Python](../reference/differences-from-python.md) lists every place a
  Python programmer will be surprised.
- [What builds today](../reference/status.md) says, feature by feature, what the compiler
  builds, and the [language reference](../reference/README.md) defines the rest.
