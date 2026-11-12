# 0049. Compression keeps Python's module names, works on byte arrays, and raises `OSError`

Status: Accepted (Collins, 2026-11-12)
Date: 2026-11-12
Roadmap: 3.27

## Context

Scientific data is compressed almost everywhere it is stored. Survey catalogues and logs arrive
as `.gz` files, HDF5 files compress their chunks with deflate, Parquet files compress their pages
with Snappy, Zstandard, gzip or LZ4, PNG images hold deflate streams, and FITS tile compression
(roadmap 4.2.5) uses gzip among others. Roadmap 3.27 builds the codecs natively, so that the file
formats of roadmap 3.14 and the plots of 3.15 need no C library and no Python.

Python spreads compression over standard modules, each with the same shape: `zlib`,
`gzip`, `bz2`, `lzma`, and since Python 3.14 `compression.zstd`. Each has `compress(data)` and
`decompress(data)` on `bytes`, and the file formats add `open(path)`. Each raises its own error
for bad data: `zlib.error`, `gzip.BadGzipFile` (an `OSError`), `EOFError` for a truncated file,
`lzma.LZMAError` and so on.

Three questions had to be settled when compiling that Python settles while running:

- **What holds the bytes.** Baryon has no `bytes` type yet (b"..." literals are listed as not
  built in [What builds today](../reference/status.md)). It has `Array[u8, 1]`, which reads and
  writes files with `Array.fromfile` and `tofile` and costs one byte per element.
- **What bad data raises.** A compiled function declares what it raises, so five error classes
  for one kind of failure would make every caller name them all.
- **Where the codecs are written.** In Baryon, as `linalg` and `fft` are (decision 0040), or in
  the runtime's Rust, as the bytes of `.npy` and NetCDF files are read.

Collins chose among three options on 2026-11-12: Python's module names with `OSError` for bad
data (this record), Python's names with Python's error classes, or one Baryon-only `compression`
module with a `format=` argument. He took the first.

## Decision

1. **Python's module names and functions.** `zlib.compress(data, level=-1)`,
   `zlib.decompress(data, wbits=15)` with every `wbits` form Python accepts (zlib, raw deflate,
   gzip and automatic), `zlib.crc32` and `zlib.adler32`; then `gzip.compress`, `gzip.decompress`
   and `gzip.open`; then `bz2`, `lzma` and `compression.zstd` under Python 3.14's names. A
   program moves between Python and Baryon with its imports and calls unchanged.
2. **Bytes are `Array[u8, 1]`** wherever Python has `bytes`, until Baryon has a `bytes` type.
   `compress` returns a new array of exactly the stream's length.
3. **Bad data raises `OSError` with the kind `INVALID_DATA`**, as a bad FITS, NetCDF or `.npy`
   file does (decisions 0043, 0044 and 0047). Its `strerror` is Python's message, such as
   `Error -3 while decompressing data: invalid block type`, so the words match what Python
   prints. A program that catches `zlib.error`, `gzip.BadGzipFile` or `EOFError` catches
   `OSError` instead; the reference flags this one difference.
4. **The codecs are written in Baryon** in the standard library (decision 0038). The prototype
   that settled this decodes deflate in about twice the time C zlib takes, and compresses at
   level 6 in about 2.2 times; Zstandard decoding takes about four times as long as the C
   library's. Optimization starts from those benchmarks, as the motto asks.
5. **Streams are standard, not byte-identical to C zlib.** `zlib.compress` follows zlib's
   algorithm (hash chains, lazy matching, the levels' search limits, and the smallest of stored,
   fixed and dynamic blocks) and its streams come within a few bytes of zlib's, but they are not
   the same bytes. Tests check that Python's `zlib` decodes every stream Baryon writes, and that
   Baryon decodes every stream Python writes, with the same errors for corrupt input.
6. **Codecs with no Python module stay inside the formats that use them.** Snappy and LZ4 serve
   Parquet pages only, so they are private to the Parquet reader and writer, not new
   language-visible modules.

## Consequences

- Built in sub-rows 3.27.1 to 3.27.10: deflate decoding with its zlib and gzip containers and
  checksums, deflate encoding, the `zlib` and `gzip` functions on arrays, `gzip.open` streaming
  files larger than memory, Snappy, LZ4, Zstandard decoding and encoding, `bz2` and `lzma`.
- Parquet and HDF5 (roadmap 3.14.6) and PNG output (3.15.6) call these codecs instead of storing
  data uncompressed.
- When Baryon gains a `bytes` type, the functions take and give it, and `Array[u8, 1]` keeps
  working through a conversion; that change is its own decision.
- The standard-library files grow past the 500-line guideline, since a module is one file and
  its private helpers cannot be imported from another; idea I.50 (one module across several
  files) splits them.
