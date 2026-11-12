# Compression

[What builds today](status.md#compression) says which parts of this chapter the compiler builds
into programs yet.

Compressed data is everywhere scientific data is kept: `.gz` catalogues and logs, the deflate
chunks of HDF5 files, the pages of Parquet files and the image data of PNG files. Baryon reads and
writes the common compression formats with Python's module names, written in Baryon itself
([decision 0049](../decisions/0049-compression.md)), so a program moved from Python keeps its
imports and calls.

| Module | Format | Functions |
| --- | --- | --- |
| [`zlib`](#zlib) | Deflate streams (RFC 1951), alone or in a zlib (RFC 1950) or gzip (RFC 1952) container | `decompress`, `crc32`, `adler32` |

Python's functions take and give `bytes`. Baryon has no `bytes` type yet, so they take and give
`Array[u8, 1]`, an array of bytes, which [`Array.fromfile`](files-and-processes.md#array-files)
reads from a file and `a.tofile` writes to one.

```python
from zlib import crc32, decompress


def main() -> Raises[None, OSError]:
    table = decompress(Array.fromfile("level6.z", u8))
    print(len(table), "bytes, CRC-32", crc32(table))
    table.tofile("table.txt")
```

This reads `level6.z`, a file Python's `zlib.compress` wrote, decompresses it, prints
`11600 bytes, CRC-32 1019181682` and writes the 11,600 bytes back out as `table.txt`.

## `zlib`

### `zlib.decompress`

`decompress(data, wbits=15, bufsize=16384)` gives the bytes `data` holds compressed, as Python's
`zlib.decompress` does. `wbits` names the container, with Python's numbers:

| `wbits` | Container |
| --- | --- |
| 8 to 15 | A zlib stream whose window is at most 2 ** `wbits` bytes; 15, the default, takes any |
| 0 | A zlib stream of any window |
| -8 to -15 | A raw deflate stream, with no header and no checksum |
| 24 to 31, or 16 | A gzip member, its CRC-32 and length checked |
| 40 to 47, or 32 | A zlib stream or a gzip member, told apart by the first two bytes |

Bytes after the stream are ignored, as in Python. A gzip file may hold several members one
after another; `decompress` reads the first only, as Python's does, and roadmap 3.27.3's
`gzip.decompress` reads them all. `bufsize` is the size the output buffer starts at; it doubles as
the output grows, so the argument only saves copies when the size is known.

**Cost.** One pass over `data` and one over the output, plus the copies of the output each
time its buffer doubles, about as many bytes again in all. The decoder takes about twice the
time C zlib takes for the same stream; making it faster is a later roadmap step.

### Errors

A stream that is corrupt, cut short or in another container raises `OSError` with the kind
`ErrorKind.INVALID_DATA` and, in `strerror`, the message Python's `zlib.error` carries for the
same call:

```text
Error -3 while decompressing data: invalid block type
Error -3 while decompressing data: incorrect data check
Error -5 while decompressing data: incomplete or truncated stream
Error 2 while decompressing data
```

The last is a stream compressed with a preset dictionary, which Python's `decompress` cannot
read either. A `wbits` outside the table above raises
`Error -2 while preparing to decompress data: inconsistent stream state`.

Python raises `zlib.error` here, a class of its own. Baryon raises `OSError`, as it does for any
data file it cannot read, so one `except OSError:` covers reading a file and decompressing it
([decision 0049](../decisions/0049-compression.md)).

### `zlib.crc32` and `zlib.adler32`

`crc32(data, value=0)` is the CRC-32 that gzip and PNG files carry, and `adler32(data,
value=1)` the Adler-32 that ends a zlib stream. Each gives a number from 0 to 2 ** 32 - 1, as
Python's do, and each continues from `value`, the checksum of the bytes before `data`, so a
checksum of data that arrives in pieces is `crc32(second, crc32(first))`. Each costs one pass
over `data`.
