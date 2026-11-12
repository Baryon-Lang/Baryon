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
| [`zlib`](#zlib) | Deflate streams (RFC 1951), alone or in a zlib (RFC 1950) or gzip (RFC 1952) container | `compress`, `decompress`, `decompressobj`, `crc32`, `adler32` |
| [`gzip`](#gzip) | gzip files (RFC 1952) of one member or several | `compress`, `decompress` |
| [`compression.zstd`](#compressionzstd) | Zstandard frames (RFC 8878) | `compress`, `decompress` |

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
after another; `decompress` reads the first only, as Python's does, and
[`gzip.decompress`](#gzipdecompress) reads them all. `bufsize` is the size the output buffer starts at; it doubles as
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

### `zlib.decompressobj`

`decompressobj(wbits=15)` gives a `Decompress`, which takes a stream in pieces, as Python's
does. Each `d.decompress(piece)` gives the bytes the pieces so far hold that no earlier call
gave, and `d.flush()`, called once after the last piece, gives the rest. `wbits` names the
container as for `decompress`. Once the stream ends, `d.eof` is `True` and any bytes after it
collect in `d.unused_data`. A corrupt stream raises the same `OSError` as `decompress`, from
the call that reaches it.

```python
from zlib import decompressobj


def main() -> Raises[None, OSError]:
    data = Array.fromfile("level6.z", u8)
    d = decompressobj()
    total = 0
    for start in range(0, len(data), 1000):
        total += len(d.decompress(data[start : min(start + 1000, len(data))].copy()))
    total += len(d.flush())
    print(total, d.eof)
```

This prints `11600 True`: the same 11,600 bytes as the example at the top of the chapter, a
thousand compressed bytes at a time.

**Cost.** A `Decompress` gives out whole deflate blocks. It keeps the part of a block that a
piece cuts off and decodes that block again, from its start, when the next piece arrives, so
pieces of 64 KiB or more cost one pass over the data, and pieces much smaller than a block,
which is often 16 to 64 KiB of output, decode its start several times. It also keeps the last
32 KiB of output, which a later block may copy from.

## `gzip`

A gzip file is one or more members, each a header, a deflate stream, and the CRC-32 and length
of what it decompresses to. `gzip` writes and reads them on arrays, as Python's does; reading
and writing a gzip file piece by piece, larger than memory, is `gzip.open`, roadmap 3.27.4.

```python
import gzip


def main() -> Raises[None, OSError]:
    gzip.compress(Array.fromfile("text.txt", u8)).tofile("table.txt.gz")
    packed = Array.fromfile("table.txt.gz", u8)
    print(len(gzip.decompress(packed)), "bytes from", len(packed))
```

This compresses the 11,600-byte table at level 9 into a 4,390-byte file and prints
`11600 bytes from 4390`. `gunzip table.txt.gz` and Python's `gzip.decompress` read it back.

### `gzip.compress`

`compress(data, compresslevel=9, mtime=None)` gives `data` as one gzip member, as Python's does:
`compresslevel` runs from 0 to 9 as `zlib.compress`'s `level`, and the header records no file
name, the operating system 255, "unknown", and `mtime`, a time in seconds since 1970. Python
writes the current time when `mtime` is `None`; Baryon has no clock yet and writes 0, which RFC
1952 reads as "no time recorded", so the same data always gives the same file. Its cost is
`zlib.compress`'s.

### `gzip.decompress`

`decompress(data)` gives the bytes every member of `data` holds, joined, as Python's does. It
skips zero bytes between and after members, and the extra field, file name and comment a header
may hold. Empty `data` gives an empty array. Data that is not a gzip file raises `OSError` with
the kind `INVALID_DATA` and Python's message:

| Data | Message |
| --- | --- |
| Not gzip | `Not a gzipped file (b'PK')`, with the first two bytes |
| A method other than deflate | `Unknown compression method` |
| A member cut short | `Compressed file ended before the end-of-stream marker was reached` |
| A wrong CRC-32 | `CRC check failed` |
| A wrong length | `Incorrect length of data produced` |
| A corrupt deflate stream | `zlib.decompress`'s messages, such as `Error -3 while decompressing data: invalid block type` |

Python raises `gzip.BadGzipFile`, `EOFError` or `zlib.error` for these. Baryon raises
`OSError` for all of them ([decision 0049](../decisions/0049-compression.md)).

**Cost.** One pass over `data` and one over the output, plus a copy of the bytes after each
member, as Python's makes, and one of the output to join the members. A file of thousands of
small members, such as a BGZF file, copies its tail once per member and is better read with
`gzip.open`.

## `compression.zstd`

Zstandard compresses about as well as gzip's highest level and decompresses several times
faster; Parquet files, HDF5 filters and many archives use it. Python 3.14 added it as
`compression.zstd`, and Baryon keeps that name. Import it with `from compression import zstd`
or `from compression.zstd import decompress`; `import compression.zstd` does not build yet.

```python
from compression import zstd
from zlib import crc32


def main() -> Raises[None, OSError]:
    table = zstd.decompress(Array.fromfile("level3.zst", u8))
    print(len(table), "bytes, CRC-32", crc32(table))
```

This reads `level3.zst`, a file Python's `zstd.compress` wrote from the table at the top of the
chapter, and prints `11600 bytes, CRC-32 1019181682`.

### `compression.zstd.decompress`

`decompress(data)` gives the bytes the frames in `data` hold, joined, as Python's does. It skips
skippable frames, and checks each frame's content size and content checksum where the frame
records them. Frames that need a dictionary, and the `zstd_dict` and `options` arguments, are
not built yet.

Data Python refuses raises `OSError` with the kind `INVALID_DATA` and, in `strerror`, the
message Python's `ZstdError` carries:

| Data | Message |
| --- | --- |
| Empty, or a frame cut short | `Compressed data ended before the end-of-stream marker was reached` |
| Bytes that start no frame | `Unable to decompress Zstandard data: Unknown frame descriptor` |
| A wrong checksum | `Unable to decompress Zstandard data: Restored data doesn't match checksum` |
| A frame that needs a dictionary | `Unable to decompress Zstandard data: Dictionary mismatch` |
| A reserved header bit set | `Unable to decompress Zstandard data: Unsupported frame parameter` |
| A window over 128 MiB | `Unable to decompress Zstandard data: Frame requires too much memory for decoding` |
| More output than the frame's content size | `Unable to decompress Zstandard data: Destination buffer is too small` |
| Any other damage | `Unable to decompress Zstandard data: Data corruption detected` |

The C library checks a damaged frame in its own order, so a frame damaged in more than one way
can name a different check than Python's, about one frame in fifty in tests of single flipped
bits; the error is raised either way. A damaged frame with no checksum can decode to different
wrong bytes than Python's, as no check catches it.

**Cost.** One pass over `data` and one over the output. A frame that records its size, as
`zstd.compress` writes one, is decoded into a buffer of that size; the output of a frame
without one is copied each time its buffer doubles. The decoder takes about four times the time
the C library takes; making it faster is a later roadmap step.

### `compression.zstd.compress`

`compress(data, level=3)` gives `data` as one Zstandard frame, which any Zstandard decoder reads,
Python's included. The frame records the content size and ends with an XXH64 checksum of the
content, which Python writes only when its `options` ask for one. `level` runs from -131072 to
22, as Python's does, and 0 means 3; a level outside that range raises `ValueError` with
Python's message, `illegal compression level 23; the valid range is [-131072, 22]`.

Baryon's encoder has one strategy, the C library's fast one: hash chains of earlier positions,
matches of four bytes or more up to 1 MiB back, the repeated offset when it is as long, and
Huffman-coded literals where they pay. The level sets how far each chain is searched, from one
position at negative levels to 256 at level 19 and above. At levels 1 to 3 its frames come within
about 15% of the C library's size; above that the C library's slower strategies win, by up to a
third at level 19. The bytes are Baryon's own, as for `zlib.compress`
([decision 0049](../decisions/0049-compression.md)).

**Cost.** One pass over `data`, each position following up to the level's number of chain
links, plus about 4.5 MiB of tables for data of 1 MiB or more, less for smaller data.
