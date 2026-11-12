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
| [`zlib`](#zlib) | Deflate streams (RFC 1951), alone or in a zlib (RFC 1950) or gzip (RFC 1952) container | `compress`, `decompress`, `crc32`, `adler32` |

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

### `zlib.compress`

`compress(data, level=-1, wbits=15)` gives `data` compressed, as Python's `zlib.compress`
does. `level` trades time for size: 0 stores the data without compressing it, 1 is the fastest,
9 the smallest, and -1, the default, means 6. `wbits` names the container as for
[`decompress`](#zlibdecompress): 9 to 15 a zlib stream whose window is 2 ** `wbits` bytes (8 is
taken as 9, as zlib takes it), -9 to -15 a raw deflate stream, and 25 to 31 a gzip member with
no file name and a modification time of 0, the header zlib itself writes.

```python
from zlib import compress, decompress


def round_trip(data: Array[u8, 1]) -> Raises[bool, OSError]:
    """Whether `data` survives compression at the smallest level."""
    back = decompress(compress(data, 9))
    if len(back) != len(data):
        return False
    for i in range(len(data)):
        if back[i] != data[i]:
            return False
    return True
```

The compressor is zlib's algorithm written in Baryon: a hash chain of earlier positions for each
three bytes, searched up to each level's limit; from level 4, a match is put off by a byte when
the next position has a longer one; and each block of about 16,000 symbols written in whichever
of deflate's three block forms (stored, fixed codes or its own codes) is smallest. Its streams
come within a few bytes of zlib's, and on many inputs they are exactly as long, but they are not
always the same bytes, so compare what streams decompress to, not the streams themselves
([decision 0049](../decisions/0049-compression.md)).

A `level` outside -1 to 9, or a `wbits` outside the forms above, raises `OSError` with the kind
`INVALID_DATA` and Python's message, `Bad compression level`.

**Cost.** One pass over `data`, with a search at each position that grows with the level: at
most 4 earlier positions at level 1, 128 at level 6 and 4096 at level 9. Level 6 takes about 2.2
times as long as C zlib; making it faster is a later roadmap step. Memory is the output plus
256 KiB of tables.

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
