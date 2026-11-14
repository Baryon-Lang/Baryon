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

## Writing

`writes.txt` holds the scenarios of the HDF5 writer's tests (`files::hdf5::writing` in the
runtime). Each is a log of changes, in the lines that module documents, after a line
`== name size digest`: the size and the 64-bit FNV-1a digest of the file h5py writes with the
same calls. A dataset's values are 0, 1, 2 and so on in its element type. A gzip dataset's
line ends with the sizes of the chunks h5py deflated, and the line `V` after it holds those
chunks in hexadecimal: the tests hand h5py's own compressed bytes to the writer, so the whole
file, layout and all, must match byte for byte even though the writer's own zlib compresses
differently. The digests stand in for the files, the largest of which is 17 MB: chunked
scenarios that fill the chunk cache.

To record the sizes and digests again, after adding a scenario with `== name 0 0`, run this in
this directory as `python record.py writes.txt`:

```python
import itertools
import os
import sys
import tempfile

import h5py
import numpy as np


def fnv(data):
    digest = 0xCBF29CE484222325
    for byte in data:
        digest = ((digest ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return digest


def lengths(text):
    return tuple(int(length) for length in text.split(",") if length)


def run(lines, path):
    with h5py.File(path, "w") as f:
        for line in lines:
            fields = line.split("\t", 4)
            if fields[0] == "G":
                f.create_group(fields[1])
            elif fields[0] == "D":
                shape = lengths(fields[3])
                values = np.arange(int(np.prod(shape))).astype(fields[2]).reshape(shape)
                if len(fields) == 4:
                    f.create_dataset(fields[1], data=values)
                else:
                    chunks, filters = fields[4].split("\t")[:2]
                    names = filters.split(",")
                    levels = [int(name[5:]) for name in names if name.startswith("gzip=")]
                    f.create_dataset(
                        fields[1],
                        data=values,
                        chunks=lengths(chunks) or True,
                        shuffle="shuffle" in names,
                        fletcher32="fletcher32" in names,
                        compression="gzip" if levels else None,
                        compression_opts=levels[0] if levels else None,
                    )
            elif fields[0] == "A":
                target = f[fields[1]] if fields[1] else f
                if fields[3] == "str":
                    target.attrs[fields[2]] = fields[4]
                else:
                    shape, digits = fields[4].split("\t")
                    values = np.frombuffer(bytes.fromhex(digits), dtype=fields[3])
                    target.attrs[fields[2]] = values.reshape(lengths(shape))[()]


def deflated(line, path):
    """A gzip dataset's line with the sizes of h5py's deflated chunks, and those chunks."""
    fields = line.split("\t")
    with h5py.File(path, "r") as f:
        dataset = f[fields[1]]
        counts = [-(-n // c) for n, c in zip(dataset.shape, dataset.chunks)]
        chunks = []
        for position in itertools.product(*map(range, counts)):
            offset = tuple(at * c for at, c in zip(position, dataset.chunks))
            chunk = dataset.id.read_direct_chunk(offset)[1]
            chunks.append(chunk[:-4] if dataset.fletcher32 else chunk)
    sizes = ",".join(str(len(chunk)) for chunk in chunks)
    return ["\t".join(fields[:6] + [sizes]), "V\t" + b"".join(chunks).hex()]


lines = [line for line in open(sys.argv[1]).read().split("\n") if not line.startswith("V\t")]
path = os.path.join(tempfile.gettempdir(), "record.h5")
out = []
heads = [index for index, line in enumerate(lines) if line.startswith("== ")]
out.extend(lines[: heads[0]] if heads else lines)
for start, end in zip(heads, heads[1:] + [len(lines)]):
    body = lines[start + 1 : end]
    run([line for line in body if line and not line.startswith("#")], path)
    data = open(path, "rb").read()
    out.append(f"== {lines[start].split()[1]} {len(data)} {fnv(data):016x}")
    for line in body:
        if line.startswith("D\t") and "gzip=" in line:
            out.extend(deflated(line, path))
        else:
            out.append(line)
open(sys.argv[1], "w").write("\n".join(out))
```
