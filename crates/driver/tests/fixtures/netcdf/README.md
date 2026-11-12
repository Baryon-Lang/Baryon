# NetCDF classic files written by netCDF4

These files were written by `netCDF4` 1.7.2 with NumPy 2.5.4 on Linux x86-64. The runtime's
tests and the build tests read them, so the tests show that Baryon reads the files `netCDF4`
writes (decision 0047) without Python running in the tests.

- `classic.nc`: a classic (CDF-1) file. It has the global attributes `title` (text),
  `version` (an `int32`) and `weights` (two `float64`s); the dimensions `time` (unlimited, two
  records), `lat` (2) and `lon` (3); and the variables:
  - `temp`, `f4` over `(time, lat, lon)`, `units = "K"`, the values 270.5 to 281.5;
  - `packed`, `i2` over `(time, lat, lon)` with the `float32` attributes `scale_factor = 0.1`
    and `add_offset = 250`, stored as -3, 4, 11 and so on up to 74;
  - `count`, `i4` over `(time,)`, the values 7 and -8;
  - `lat`, `f8` over `(lat,)`, `units = "degrees_north"`, the values 10 and 20;
  - `flags`, `i1` over `(lon,)`, the values -1, 0 and 1.
- `offset.nc`: the same in the 64-bit offset format (CDF-2).

To record them again, run this in this directory:

```text
import numpy as np
from netCDF4 import Dataset

for fmt, name in [("NETCDF3_CLASSIC", "classic.nc"), ("NETCDF3_64BIT_OFFSET", "offset.nc")]:
    with Dataset(name, "w", format=fmt) as ds:
        ds.setncattr("title", "test grid")
        ds.setncattr("version", np.int32(3))
        ds.setncattr("weights", np.array([0.5, 1.25], dtype="f8"))
        ds.createDimension("time", None)
        ds.createDimension("lat", 2)
        ds.createDimension("lon", 3)
        temp = ds.createVariable("temp", "f4", ("time", "lat", "lon"))
        temp.setncattr("units", "K")
        temp[:] = np.arange(12, dtype="f4").reshape(2, 2, 3) + 270.5
        packed = ds.createVariable("packed", "i2", ("time", "lat", "lon"))
        packed.set_auto_scale(False)
        packed.setncattr("scale_factor", np.float32(0.1))
        packed.setncattr("add_offset", np.float32(250.0))
        packed[:] = np.arange(12, dtype="i2").reshape(2, 2, 3) * 7 - 3
        count = ds.createVariable("count", "i4", ("time",))
        count[:] = [7, -8]
        lat = ds.createVariable("lat", "f8", ("lat",))
        lat.setncattr("units", "degrees_north")
        lat[:] = [10.0, 20.0]
        flags = ds.createVariable("flags", "i1", ("lon",))
        flags[:] = [-1, 0, 1]
```

## Files the writing tests compare against

The runtime's writing tests make each of these, change by change, and compare the bytes, so
they show that Baryon writes the files `netCDF4` writes, fill values and padding included.

- `written.nc`: a classic file with the global attributes `n = 3`, `x = 2.5` and `s = "hi"`,
  the dimensions `t` (unlimited) and `x` (3), the record variable `v` (`i2` over `(t, x)`,
  one record 1, 2, 3), and the variables `w` (`f4` over `(x,)`) and `z` (`f8` over `(t,)`),
  never written, so filled.
- `filled.nc`: a 64-bit offset file whose record variables `a` (`i2` over `(t, x)`) and `b`
  (`i1` over `(t,)`) are filled, padding and all, because `d` (`f8` over `(t,)`) wrote two
  records; the fixed `c` (`i2` over `(x,)`) is filled; `a` gains `units = "m"` last.
- `single.nc`: a classic file with one record variable, `e` (`i2` over `(t,)`), three records
  5, 6 and 7, unpadded since it is the only one.
- `moved.nc`: `single.nc` with a second record variable `f` added after the records were
  written, so the C library moves `e`'s records to the new, padded record size.
- `padded.nc`: a classic file with the record variables `a` (`i2` over `(t, x)`) and `b` (`i1`
  over `(t,)`), one record of `a` written: the C library fills the new record for both,
  padding and all, before it writes `a`'s values.

To record them again, run this in this directory:

```text
import numpy as np
from netCDF4 import Dataset

with Dataset("written.nc", "w", format="NETCDF3_CLASSIC") as ds:
    ds.setncattr("n", 3)
    ds.setncattr("x", 2.5)
    ds.setncattr("s", "hi")
    ds.createDimension("t", None)
    ds.createDimension("x", 3)
    v = ds.createVariable("v", "i2", ("t", "x"))
    v[:] = np.array([[1, 2, 3]])
    ds.createVariable("w", "f4", ("x",))
    ds.createVariable("z", "f8", ("t",))

with Dataset("filled.nc", "w", format="NETCDF3_64BIT_OFFSET") as ds:
    ds.createDimension("t", None)
    ds.createDimension("x", 3)
    a = ds.createVariable("a", "i2", ("t", "x"))
    ds.createVariable("b", "i1", ("t",))
    ds.createVariable("c", "i2", ("x",))
    d = ds.createVariable("d", "f8", ("t",))
    d[:] = np.array([1.0, 2.0])
    a.setncattr("units", "m")

for name, second in [("single.nc", False), ("moved.nc", True)]:
    with Dataset(name, "w", format="NETCDF3_CLASSIC") as ds:
        ds.createDimension("t", None)
        e = ds.createVariable("e", "i2", ("t",))
        e[:] = np.array([5, 6, 7], dtype="i2")
        if second:
            ds.createVariable("f", "i2", ("t",))

with Dataset("padded.nc", "w", format="NETCDF3_CLASSIC") as ds:
    ds.createDimension("t", None)
    ds.createDimension("x", 3)
    a = ds.createVariable("a", "i2", ("t", "x"))
    ds.createVariable("b", "i1", ("t",))
    a[:] = np.array([[4, 5, 6]], dtype="i2")
```

## Files the slice tests compare against

The runtime's tests write slices of a variable, `var[0, ::2] = values`, change by change, and
compare the bytes with these.

- `rows.nc`: a classic file with the record variables `a` (`i2` over `(t, x)`) and `b` (`i1` over
  `(t,)`). `a[2] = [4, 5, 6]` adds three records, each filled for both variables, then
  `a[0, ::2] = [7, 8]` writes every other value of the first.
- `point.nc`: `a[1, 1] = 9` of the only record variable, which adds two filled records.
- `grid.nc`: `c[1:3, -1] = [1.5, 2.5]` of a fixed `f4` variable over `(x, y)` of 4 by 3, the rest
  filled.

To record them again, run this in this directory:

```text
import numpy as np
from netCDF4 import Dataset

with Dataset("rows.nc", "w", format="NETCDF3_CLASSIC") as ds:
    ds.createDimension("t", None)
    ds.createDimension("x", 3)
    a = ds.createVariable("a", "i2", ("t", "x"))
    ds.createVariable("b", "i1", ("t",))
    a[2] = np.array([4, 5, 6], dtype="i2")
    a[0, ::2] = np.array([7, 8], dtype="i2")

with Dataset("point.nc", "w", format="NETCDF3_CLASSIC") as ds:
    ds.createDimension("t", None)
    ds.createDimension("x", 3)
    a = ds.createVariable("a", "i2", ("t", "x"))
    a[1, 1] = 9

with Dataset("grid.nc", "w", format="NETCDF3_CLASSIC") as ds:
    ds.createDimension("x", 4)
    ds.createDimension("y", 3)
    c = ds.createVariable("c", "f4", ("x", "y"))
    c[1:3, -1] = np.array([1.5, 2.5], dtype="f4")
```
