# 0047. NetCDF classic files are read and written with netCDF4's names

Status: Accepted
Date: 2026-11-12
Roadmap: 3.14.5

## Context

NetCDF is the file format of climate, weather, ocean and much of earth science: model output,
reanalyses and satellite products arrive as `.nc` files. A NetCDF file holds named dimensions,
named variables, each an array over some of the dimensions, and attributes, small named values
on the file or on a variable such as `units = "K"`. Roadmap 3.14.5 reads and writes the classic
formats, CDF-1 and CDF-2 (64-bit offsets), which Unidata specifies in a few pages and which need
no outside library. NetCDF-4 files are HDF5 files and wait on roadmap 3.14.6.

Python has three common ways in:

- **`netCDF4`**, Unidata's own package: `Dataset(path)`, `ds.variables["temp"][:]`,
  `ds.dimensions["time"].size`, `var.getncattr("units")`, `ds.createVariable(...)`. It reads and
  writes every format, and xarray uses it underneath.
- **`scipy.io.netcdf_file`**: classic formats only, a similar `variables` mapping, and no way to
  grow into NetCDF-4.
- **xarray**, `open_dataset(path)`: labelled arrays with coordinates. It needs labelled arrays
  and the DataFrame work of roadmap 3.13 first.

As for FITS ([decision 0044](0044-fits-images-and-headers.md)), three things Python settles when
the program runs must be settled when it compiles: a variable's element type and rank, the type
of an attribute, and what a missing name raises. One more is particular to NetCDF: `netCDF4`
returns a masked array by default, with `_FillValue` points masked and `scale_factor` and
`add_offset` applied, and Baryon has no masked arrays yet (roadmap D.21).

## Decision

1. **A `netCDF4` module with Unidata's names.** `from netCDF4 import Dataset`, the names and
   arguments of the `netCDF4` package, so a program moves between Python and Baryon unchanged.
   The module is written in Baryon in the standard library
   ([decision 0038](0038-standard-library-in-baryon.md)), header parsing included, with the
   runtime reading and writing the bytes of variables. When 3.14.6 brings HDF5, NetCDF-4 files
   open through the same `Dataset`.
2. **A variable's type and rank come from the context**, as `fits.getdata`'s do:
   `temp: Array[f32, 3] = ds.variables["temp"][:]`. The axes are in the file's order, as
   `netCDF4` gives them. A rank other than the variable's, or an element type that cannot hold
   its values, raises `OSError` with the kind `INVALID_DATA`. The first version reads a whole
   variable, `[:]`; slices of a variable come later.
3. **Scaled, never masked.** A float destination gets `scale_factor` and `add_offset` applied,
   as `netCDF4` applies them. Fill values are read as stored, as `netCDF4` gives them after
   `ds.set_auto_mask(False)`, until Baryon has missing values (D.21). The data-files reference
   and the transition notes flag this one difference.
4. **Attributes and dimensions are typed by the context.** `units: str =
   ds.variables["temp"].getncattr("units")` and `ds.getncattr("title")` read attributes, typed
   by where they go; `ds.dimensions["time"].size` is an `int`. A missing variable or dimension
   raises `KeyError`, and a missing attribute `AttributeError`, as in `netCDF4`. Attribute access
   by name, `ds.title`, is not taken: the name is not known when compiling.
5. **Writing names its format.** `Dataset(path, "w", format="NETCDF3_64BIT_OFFSET")` or
   `format="NETCDF3_CLASSIC"`, then `createDimension`, `createVariable(name, "f4", dims)`,
   `var[:] = array`, `setncattr` and `close()`, or a `with` block. `netCDF4` writes NetCDF-4 when
   no format is given; until 3.14.6 a missing `format=` is a compile error that says so, so no
   program silently writes a different format from Python's. Files written are read by
   `netCDF4` and `ncdump`, and their bytes are compared with what `netCDF4` 1.7 writes for the
   same calls.

## Consequences

- A `netCDF4` user reads `Dataset`, `variables` and `[:]` as they do today; the transition notes
  list the differences: types come from the program, fill values are not masked, attributes are
  read with `getncattr`, and writing names its format until 3.14.6.
- `AttributeError` joins `OSError`, `ValueError`, `EOFError` and `KeyError` as an error type the
  standard library provides.
- Built in slices, smallest first: reading (header, dimensions, attributes, whole variables),
  then writing, then slices of a variable. Each slice ships with its docs and tests against files
  `netCDF4` wrote.
- xarray's names are not taken here; a labelled-array layer can be built on this module once
  3.13 lands.
