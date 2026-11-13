# HDF5 files written by h5py

These files were written by h5py 3.16 (HDF5 2.0) with NumPy 2.5.4 on Linux x86-64. The runtime's
tests read them, so the tests show that Baryon reads the files h5py writes (decision 0052)
without Python running in the tests. The two files hold the same objects:

- `earliest.h5` in h5py's default layout: a version 0 superblock, version 1 object headers,
  groups as symbol tables, and every chunked dataset indexed by a version 1 B-tree.
- `latest.h5` with `libver="latest"`: a version 3 superblock, version 2 object headers, links
  and attributes past eight kept densely in fractal heaps, and each chunk index the newer
  format has: a single chunk (`single`), a fixed array (`plain`, `deflated`, `shuffled`), an
  extensible array (`grow`, and `long`, whose 400 chunks reach its super blocks) and a version
  2 B-tree (`grow2`).

The objects:

- the root's attributes `title` (text), `version` (an `int32`) and `weights` (two `float64`s);
- `run1/temp`, `f4` of shape (2, 3, 4), the values 270.5 to 293.5, with `units = "K"`;
- `count`, `i4`, the values 7, -8 and 9; `big`, big-endian `i2`, the values 1, -2 and 300;
- `scalar`, one `f8`, 2.5; `compact`, four `i2` stored in the object header, 1 to 4;
- `plain`, `deflated`, `shuffled` and `single`: the same 6 by 5 `f8` grid, `0.5 * i`, chunked
  4 by 2 (6 by 5 for `single`), with no filter, with shuffle, deflate and Fletcher-32, with
  shuffle and Fletcher-32, and with deflate;
- `grow`, `i8` 0 to 9 in chunks of 3, deflated, along a dimension without limit; `long`, `i2`
  0 to 399 in chunks of 1, the same; `grow2`, `u1` 0 to 11 of shape (3, 4) in chunks of 2 by 2,
  both dimensions without limit;
- `many`, one `f4`, 1.5, with the ten attributes `a0` to `a9`, `int16`s from -5 in steps of 3.

To record them again, run this in this directory:

```text
import h5py
import numpy as np

def fill(f):
    f.attrs["title"] = "test run"
    f.attrs["version"] = np.int32(3)
    f.attrs["weights"] = np.array([0.5, 1.25])
    run = f.create_group("run1")
    temp = run.create_dataset("temp", data=np.arange(24, dtype="f4").reshape(2, 3, 4) + 270.5)
    temp.attrs["units"] = "K"
    f.create_dataset("count", data=np.array([7, -8, 9], dtype="i4"))
    f.create_dataset("big", data=np.array([1, -2, 300], dtype=">i2"))
    f.create_dataset("scalar", data=np.float64(2.5))
    dcpl = h5py.h5p.create(h5py.h5p.DATASET_CREATE)
    dcpl.set_layout(h5py.h5d.COMPACT)
    space = h5py.h5s.create_simple((4,))
    h5py.h5d.create(f.id, b"compact", h5py.h5t.NATIVE_INT16, space, dcpl=dcpl).write(
        h5py.h5s.ALL, h5py.h5s.ALL, np.array([1, 2, 3, 4], dtype="i2"))
    grid = np.arange(30, dtype="f8").reshape(6, 5) * 0.5
    f.create_dataset("plain", data=grid, chunks=(4, 2))
    f.create_dataset("deflated", data=grid, chunks=(4, 2), compression="gzip",
                     compression_opts=6, shuffle=True, fletcher32=True)
    f.create_dataset("single", data=grid, chunks=(6, 5), compression="gzip")
    grow = f.create_dataset("grow", data=np.arange(10, dtype="i8"), chunks=(3,),
                            maxshape=(None,), compression="gzip")
    f.create_dataset("shuffled", data=grid, chunks=(4, 2), shuffle=True, fletcher32=True)
    f.create_dataset("long", data=np.arange(400, dtype="i2"), chunks=(1,), maxshape=(None,))
    many = f.create_dataset("many", data=np.float32(1.5))
    for index in range(10):
        many.attrs[f"a{index}"] = np.int16(index * 3 - 5)
    f.create_dataset("grow2", data=np.arange(12, dtype="u1").reshape(3, 4), chunks=(2, 2),
                     maxshape=(None, None))

for libver, name in [("earliest", "earliest.h5"), ("latest", "latest.h5")]:
    with h5py.File(name, "w", libver=libver) as f:
        fill(f)
```
