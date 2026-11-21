# `lzma`

[What builds today](status.md#compression) says which parts of this page the compiler builds
into programs yet. The other formats, and the byte arrays every one of them takes, are in
[Compression](compression.md).

xz files compress tighter than bzip2 and decompress faster; source archives, Linux packages and
many data releases ship as `.xz`. Baryon's `lzma` module keeps Python's name and reads xz files
and the older `.lzma` files, whole or through [`lzma.open`](#lzmaopen), and writes xz data with
[`lzma.compress`](#lzmacompress).

```python
import lzma
from zlib import crc32


def main() -> Raises[None, OSError]:
    table = lzma.decompress(Array.fromfile("level6.xz", u8))
    print(len(table), "bytes, CRC-32", crc32(table))
```

This reads `level6.xz`, a file Python's `lzma.compress` wrote from the table at the top of the
[Compression](compression.md) chapter, and prints `11600 bytes, CRC-32 1019181682`.

## `lzma.decompress`

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

## `lzma.compress`

`compress(data, preset=6)` gives `data` as one xz stream with a CRC-64 check, which any xz
decoder reads, Python's `lzma` and the `xz` tool included. `preset` runs from 0, fastest, to 9,
and sets the dictionary, from 256 KiB at 0 to 8 MiB at 6 and 64 MiB at 9, as XZ Utils sets it.
A preset outside 0 to 9 raises `OSError` with the kind `INVALID_DATA` and Python's message,
`Invalid or unsupported options`. The `format`, `check` and `filters` arguments, and
`PRESET_EXTREME`, are not built.

The encoder works as XZ Utils' fast mode, which liblzma uses for presets 0 to 3: hash chains
find the longest match, and one step of lookahead decides between it, a match at one of the
last four distances, and a literal. Data that does not shrink is stored. Presets 0 to 3 come
within 3% of Python's size. For presets 4 to 9, liblzma searches for the cheapest way to code
the whole data, which Baryon does not do yet, so Baryon searches as at preset 3 with the
larger dictionary and its streams come out larger than Python's: about 28% larger on a 3.9 MB
table of numbers, about 4% larger on 2.4 MB of `float64` samples. The bytes are Baryon's own
([decision 0049](../decisions/0049-compression.md)); empty data and one byte give exactly
Python's stream.

**Cost.** Two passes over the data, plus the compressed copy, a 4 MiB hash table and 4 bytes
for each byte of the dictionary, up to the data's length. On the table above, presets 0 to 3
take about the time Python's take, and preset 6 takes 1.6 s where Python's takes 3.9 s.

## `lzma.open`

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
  gives an `LZMATextFile` that reads `str` as the [gzip text modes](compression.md#text-modes) do,
  with `read()`, `readline()`, `readlines()` and `for line in f:`; it decodes the whole file at its
  first read. The modes that write, such as `"wb"` and `"wt"`, are not built yet and raise
  `ValueError`; [`compress`](#lzmacompress) writes xz data whole. Any other mode raises `ValueError`
  `Invalid mode`, as in Python. `read` on a closed file raises `ValueError` `I/O operation on closed
  file`.
