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
| [`bz2`](#bz2) | bzip2 streams of one block or several | `compress`, `decompress` |
| [`lzma`](#lzma) | xz files and `.lzma` files | `decompress`, `open` to read |

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
of what it decompresses to. `gzip` writes and reads them on arrays, as Python's does, and
[`gzip.open`](#gzipopen) reads a file piece by piece, so a file larger than memory can be read.

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

### `gzip.open`

`open(filename, mode="rb", compresslevel=9)` opens a gzip file and gives a `GzipFile`, as
Python's does. Use it in `with`, which closes the file at the end of the block, however the
block ends ([decision 0053](../decisions/0053-with-and-for-on-user-classes.md)), or call
`f.close()` yourself.

```python
import gzip


def main() -> Raises[None, OSError | ValueError]:
    with gzip.open("table.txt.gz", "wb") as f:
        f.write(Array.fromfile("text.txt", u8))
    total = 0
    with gzip.open("table.txt.gz") as f:
        piece = f.read(1000)
        while len(piece) > 0:
            total += len(piece)
            piece = f.read(1000)
    print(total, "bytes")
```

This writes the 11,600-byte table as a gzip file, reads it back 1,000 bytes at a time, and
prints `11600 bytes`.

| Mode | What it does | When the file is there | When it is not |
| --- | --- | --- | --- |
| `"rb"` or `"r"`, the default | read | reads it from the start | `OSError` `FILE_NOT_FOUND` |
| `"wb"` or `"w"` | write | empties it first | creates it |
| `"ab"` or `"a"` | write a new member after the file's end | keeps what it holds | creates it |
| `"xb"` or `"x"` | write | `OSError` `FILE_EXISTS` | creates it |

| Method | Gives | What it does |
| --- | --- | --- |
| `f.read()` | `Array[u8, 1]` | the rest of the data |
| `f.read(size)` | `Array[u8, 1]` | the next `size` bytes, fewer at the end, none after it |
| `f.write(data)` | `int` | adds the bytes of `data`, an `Array[u8, 1]`, and gives how many |
| `f.close()` | `None` | writes the file out, for a file opened to write; a second call does nothing |
| `f.name` | `str` | the path the file was opened with |

- **Reading streams.** A `GzipFile` reads its file 64 KiB at a time and decodes it a deflate
  block at a time, so `read(size)` holds about one block of data, whatever the file's size.
  Members follow one another as in [`gzip.decompress`](#gzipdecompress), with the same checks
  and the same messages, and zero bytes between and after them are skipped.
- **Writing holds the compressed file.** Data is gathered 4 MiB at a time and compressed as one
  member; any gzip reader, Python and `gunzip` included, reads the members back as one stream
  of data. Baryon cannot yet add bytes to the end of a file, so the compressed members stay in
  memory until `close` writes them, and `"ab"` reads the file's bytes in first. Writing needs
  memory for the compressed file, not for the data.
- **A dropped file is closed.** A file opened to write and dropped without `close()`, at the
  end of a function or when its variable is given a new value, is closed then and written out,
  as in CPython ([decision 0063](../decisions/0063-drop-hooks.md)). An error writing it there
  cannot be raised, so it ends the program with a panic; `with` or `close()` raises it instead.
- **Text modes.** `"rt"`, `"wt"`, `"at"` and `"xt"` give a `GzipTextFile`, which reads and
  writes `str` as UTF-8; see [Text modes](#text-modes) below. Any other mode raises
  `ValueError`, as in Python.
- **Errors.** `read` on a file opened to write, or `write` on one opened to read, raises
  `OSError`, and either on a closed file raises `ValueError` `I/O operation on closed file`, as
  in Python. A `GzipFile`'s `readline`, iteration over lines, `seek`, `peek` and `mtime` are
  not built; open the file in a text mode to read it line by line.

### Text modes

`gzip.open(path, "rt")` reads a gzip file of text, and `"wt"`, `"at"` and `"xt"` write one, as in
Python ([decision 0064](../decisions/0064-utf-8-text-from-bytes.md)). The file gives and takes
`str`, stored as UTF-8, with the methods of a text file from `open`:

```python
import gzip


def main() -> Raises[None, OSError | ValueError]:
    with gzip.open("notes.txt.gz", "wt") as f:
        f.write("T = 293.15 K\n")
        f.write("Å = 1e-10 m\n")
    with gzip.open("notes.txt.gz", "rt") as f:
        for line in f:
            print(line.strip())
```

This prints `T = 293.15 K` and `Å = 1e-10 m`, and Python's `gzip.open("notes.txt.gz", "rt")`
reads the same two lines.

| Method | Gives | What it does |
| --- | --- | --- |
| `f.readline()` | `str` | the next line with its `"\n"`, or `""` at the end |
| `f.readlines()` | `list[str]` | the remaining lines |
| `f.read()` | `str` | the rest of the text |
| `for line in f:` | `str` | each line in turn |
| `f.write(text)` | `int` | adds the UTF-8 bytes of `text`, and gives its length in characters |
| `f.close()` | `None` | as for a `GzipFile` |

- **The mode is written out.** The compiler picks the class from the mode, so a text mode must
  be a string literal: `gzip.open(path, mode)` with `mode` holding `"rt"` raises `ValueError`.
  The class is `gzip.GzipTextFile`; Python's is `io.TextIOWrapper`.
- **UTF-8 only.** `encoding="utf-8"` may be written; any other encoding raises `ValueError`.
  Bytes that are not UTF-8 raise `ValueError` with Python's message, such as
  `'utf-8' codec can't decode byte 0xff in position 3: invalid start byte`, from the read that
  reaches them. `errors=` and `newline=` are not built.
- **Line ends.** Reading turns `"\r\n"` and `"\r"` into `"\n"`, as Python does by default.
  Writing writes the text as it is.
- **Cost.** Reading decodes the file 64 KiB at a time, holding back a character or a `"\r"` cut
  at the end of a block, so a file larger than memory can be read line by line; each byte is
  decoded and split once. 200,000 lines of 15 characters read in about the time Python takes.
  Writing holds the compressed file in memory until `close`, as for a `GzipFile`.

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

## `bz2`

bzip2 compresses text and tables tighter than gzip, at several times the cost; many catalogues,
logs and archives are kept as `.bz2` files. Baryon's `bz2` module keeps Python's name and
functions.

```python
import bz2


def main() -> Raises[None, OSError | ValueError]:
    table = Array.fromfile("text.txt", u8)
    packed = bz2.compress(table)
    print(len(packed), "bytes, back to", len(bz2.decompress(packed)))
```

This compresses the table at the top of the chapter to 3,918 bytes, reads it back and prints
`3918 bytes, back to 11600`.

### `bz2.decompress`

`decompress(data)` gives the bytes the bzip2 streams in `data` hold, joined, as Python's does;
empty `data` gives an empty array. As in Python, bytes after the first stream that start no
other stream are ignored. Each block's CRC and each stream's combined CRC are checked.

Data Python refuses raises `OSError` with the kind `INVALID_DATA` and, in `strerror`, the
message Python's error carries:

| Data | Message |
| --- | --- |
| A stream cut short, the first or a later one | `Compressed data ended before the end-of-stream marker was reached` |
| Bytes that are not bzip2, a damaged block or a wrong CRC | `Invalid data stream` |

Python raises `ValueError` for the first and `OSError` for the second; Baryon raises `OSError`
for both ([decision 0049](../decisions/0049-compression.md)). The decoder checks each part of a
stream in the order libbzip2 checks it, so a stream damaged and cut short gets Python's message:
all of 2,316 damaged and cut streams in tests did. Blocks in the randomized form bzip2 stopped
writing in 1999 raise `Invalid data stream`, where Python reads them.

**Cost.** One pass over `data`, and for each block of n bytes about 5n bytes of tables and one
pass over them; the output is copied each time its buffer doubles. About as fast as Python's
`bz2.decompress`.

### `bz2.compress`

`compress(data, compresslevel=9)` gives `data` as one bzip2 stream, which any bzip2 decoder
reads, Python's included. `compresslevel` runs from 1 to 9 and sets the block size, 100,000
bytes times the level; 9 compresses best, and a level outside 1 to 9 raises `ValueError` with
Python's message, `compresslevel must be between 1 and 9`.

The blocks are bzip2's: the same run-length coding, the same Burrows-Wheeler sort and the same
move-to-front stage. The Huffman tables are chosen as bzip2 chooses them, so streams come
within a fraction of a percent of the C library's size, larger or smaller. The bytes are
Baryon's own, as for `zlib.compress` ([decision 0049](../decisions/0049-compression.md)).

**Cost.** O(n log n) for n bytes: each block is sorted by prefix doubling, which takes about
13 bytes of tables for each byte of the block. Compressing takes about 1.6 times the time
Python's `bz2.compress` takes.

## `lzma`

xz files compress tighter than bzip2 and decompress faster; source archives, Linux packages and
many data releases ship as `.xz`. Baryon's `lzma` module keeps Python's name and reads xz files
and the older `.lzma` files, whole or through [`lzma.open`](#lzmaopen). Writing them is not
built yet.

```python
import lzma
from zlib import crc32


def main() -> Raises[None, OSError]:
    table = lzma.decompress(Array.fromfile("level6.xz", u8))
    print(len(table), "bytes, CRC-32", crc32(table))
```

This reads `level6.xz`, a file Python's `lzma.compress` wrote from the table at the top of the
chapter, and prints `11600 bytes, CRC-32 1019181682`.

### `lzma.decompress`

`decompress(data)` gives the bytes the streams in `data` hold, joined, as Python's does. Like
Python's default, `FORMAT_AUTO`, it tells an xz file from a `.lzma` file by its first bytes. It
checks each block's CRC-32, CRC-64 or SHA-256, and the index and footer of each stream. As in
Python, bytes after the first stream that start no other stream are ignored. The `format`,
`memlimit` and `filters` arguments are not built yet, and neither are blocks with a filter
before LZMA2, such as the x86 branch filter some packages use.

Data Python refuses raises `OSError` with the kind `INVALID_DATA` and, in `strerror`, the
message Python's `LZMAError` carries:

| Data | Message |
| --- | --- |
| Empty, or a stream cut short | `Compressed data ended before the end-of-stream marker was reached` |
| Bytes that are neither format | `Input format not supported by decoder` |
| A header naming options liblzma cannot decode, or a filter Baryon does not decode yet | `Invalid or unsupported options` |
| Any damage, a wrong check included | `Corrupt input data` |

The decoder checks each part in the order liblzma checks it, so a damaged or cut file gets
Python's message: all of 4,563 damaged and cut files in tests did.

**Cost.** One pass over `data` and one over the output, which is copied each time its buffer
doubles. The probabilities take 7 to 50 KiB, and up to 12 MiB for a `.lzma` file with the
largest literal context. Decoding takes about three times the time Python's takes.

### `lzma.open`

`open(filename, mode="rb")` opens an xz or `.lzma` file to read and gives an `LZMAFile`, as
Python's does: `with lzma.open("level6.xz") as f:` reads the table above with `f.read()`. Use it
in `with`, which closes the file at the end of the block, or call `f.close()`.

| Method | Gives | What it does |
| --- | --- | --- |
| `f.read()` | `Array[u8, 1]` | the rest of the data |
| `f.read(size)` | `Array[u8, 1]` | the next `size` bytes, fewer at the end, none after it |
| `f.close()` | `None` | lets go of the data; a second call does nothing |
| `f.name` | `str` | the path the file was opened with |

- **Read whole.** `open` reads the file's bytes, and the first `read` decompresses all of them
  with [`decompress`](#lzmadecompress), so the file and its data must both fit in memory. A
  file that is not xz or `.lzma` raises at that first `read`, as in Python, with the same
  messages; a missing file raises `OSError` `FILE_NOT_FOUND` at `open`.
- **Reading only.** `"rb"`, `"r"` and `"rt"` are the modes built. `"rt"`, written as a literal,
  gives an `LZMATextFile` that reads `str` as the [gzip text modes](#text-modes) do, with
  `read()`, `readline()`, `readlines()` and `for line in f:`; it decodes the whole file at its
  first read. The modes that write, such as `"wb"` and `"wt"`, wait on an `lzma.compress` and
  raise `ValueError` until then, and any other mode raises `ValueError` `Invalid mode`, as in
  Python. `read` on a closed file raises `ValueError`
  `I/O operation on closed file`.
