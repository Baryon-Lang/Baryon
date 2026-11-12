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
