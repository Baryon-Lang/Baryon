# 0052. HDF5 files are read with h5py's names

Status: Accepted (Collins, 2026-11-13)
Date: 2026-11-12
Roadmap: 3.14.6.1

## Context

HDF5 is how much of physics, astronomy, materials science and machine learning stores large
arrays. Simulation codes write snapshots with it, instruments and beamlines record into it, and
every NetCDF-4 file is an HDF5 file underneath. An HDF5 file is a tree. Its groups are like
directories. Its datasets are n-dimensional arrays of numbers. Groups and datasets both carry
attributes, which are small named values such as `units = "K"`. A dataset's values are stored
in one of three ways: in one contiguous block, in the dataset's header for tiny datasets, or in
chunks. Chunks are fixed-size tiles found through a B-tree, and each chunk is passed through
filters. The filters in nearly every file are deflate (zlib), shuffle (bytes regrouped so that
deflate compresses better) and the fletcher32 checksum.

HDF5 is specified in the HDF Group's *File Format Specification* (version 3.0). The reference
library is about 400,000 lines of C. Roadmap 3.14.6 reads and writes HDF5 natively, with no C
library. Deflate is already native (3.27.1, [decision 0049](0049-compression.md)).

Python has two common ways in:

- **`h5py`**, the de facto standard: `h5py.File(path, "r")`, `f["run1/temp"][...]` or
  `f["run1"]["temp"][:]`, `dset.shape`, `dset.dtype`, `dset.attrs["units"]`, `f.keys()` and
  `with h5py.File(...) as f:`. Most HDF5 code in Python uses it.
- **PyTables** (`tables.open_file`), whose own object model is aimed at tables. It is used far
  less for plain arrays.

As for FITS and NetCDF ([decisions 0044](0044-fits-images-and-headers.md) and
[0047](0047-netcdf-classic-files.md)), a few things that Python settles while it runs must be
settled when Baryon compiles. They are a dataset's element type and rank, an attribute's type,
and what a missing name, a bad subscript or a filter Baryon does not have raises. One question
is new: the codecs are written in Baryon (decision 0049), while the file's structure is
binary, and the runtime reads binary structures fastest.

Collins chose among three options on 2026-11-13: h5py's names (this record), a Baryon-only
`hdf5` module shaped like `netCDF4`, or HDF5 only through `netCDF4.Dataset`. He took the
first.

## Decision

1. **An `h5py` module with h5py's names.** `import h5py`, then `h5py.File(path)` or
   `h5py.File(path, "r")`, `f[name]` with a path such as `"run1/temp"`, `group[name]`,
   `dset[...]`, `dset[:]` and subscripts of integers and slices, `dset.shape` (a `list[int]`),
   `dset.ndim`, `dset.dtype` (NumPy's type string, such as `"<f4"`), `obj.attrs[name]`,
   `f.keys()` and `group.keys()`, `name in group`, `f.close()` and the `with` block. A
   program moves between Python and Baryon with its imports and calls unchanged.
2. **A dataset's type and rank come from the context**, as a NetCDF variable's do:
   `temp: Array[f32, 3] = f["temp"][:]`. Every integer and float type HDF5 stores is read, in
   either byte order. A rank the subscript does not give, or an element type that cannot hold
   the values, raises `OSError` with the kind `INVALID_DATA`. Reading compound, string,
   enum, reference and variable-length datasets waits on later rows.
3. **Attributes are typed by the context**: `units: str = dset.attrs["units"]`. A number
   attribute read as a float array is `Array[f64, 1]`.
4. **Errors as h5py raises them**, with h5py's messages where they can be matched:
   - `KeyError` for a missing group, dataset or attribute.
   - `IndexError` for an index out of bounds.
   - `ValueError` for a step that is not positive (h5py takes no negative steps).
   - `OSError` for a file that is not HDF5, is damaged, or uses a feature Baryon does not read
     yet, such as an unknown filter (szip, LZF, Blosc and other plugins). The message names
     the feature.
5. **The runtime reads the structure; Baryon decodes the chunks.** The runtime, in Rust, parses
   the superblock (versions 0 to 3), object headers (versions 1 and 2), groups (symbol tables
   and the newer link messages), datatypes, dataspaces, layouts and chunk indexes (the version
   1 B-tree, and the fixed, extensible-array and version 2 B-tree indexes of HDF5 1.10 and
   later). It hands the standard library a list of the chunks a subscript touches. The library
   inflates each chunk with the native `zlib`, so there is one deflate in Baryon, not two. The
   runtime then undoes shuffle, checks fletcher32, converts the byte order and the element
   type, and copies each chunk's part into the result. Contiguous and compact data skip the
   library and are read straight into the array.
6. **NetCDF-4 files open through `netCDF4.Dataset`** on the same reader, as decision 0047
   promised. That work is its own sub-row after 3.14.6.1.
7. **Writing** (3.14.6.2) is decided with its own row: what h5py writes by default, and
   which layouts and filters `create_dataset` takes.

## Consequences

- Programs read the HDF5 files that simulation codes, instruments and h5py write, with no C
  library. Files are checked against data h5py 3.16 (HDF5 2.0) wrote with each layout, each
  chunk index and each filter, compressed and not.
- A subscript of a chunked dataset reads only the chunks it touches, so one time step of a
  large file costs one step's chunks.
- Decoding speed is bounded by the native deflate, about twice C zlib's time (decision 0049),
  until 3.27 is optimized.
- Datasets of strings, compound types and variable-length data, external links, virtual
  datasets, and the szip, LZF and Blosc filters are not read in the first version. Each raises
  `OSError` naming what is missing, and the reference lists them.

## Alternatives considered

- **A Baryon-only `hdf5` module shaped like `netCDF4`.** One API for both formats, but every
  h5py program would need rewriting, against the rule that names follow the Python package
  that scientists use.
- **HDF5 only through `netCDF4.Dataset`.** This would read NetCDF-4 files, which follow
  NetCDF's conventions, but not general HDF5 files: groups without dimensions, datasets with no
  dimension scales.
- **Deflate in the runtime too.** This would be simpler to wire, but it would give two deflate
  decoders to maintain and tune, which decision 0049 set out to avoid.
