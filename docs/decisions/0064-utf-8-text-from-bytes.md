# 0064. Bytes become text by UTF-8: `str.encode`, `Array.decode` and compressed text files

Status: Accepted
Date: 2026-11-14
Roadmap: 1.13.18

## Context

A `str` is UTF-8 text ([decision 0022](0022-str-values.md)), and binary data is an
`Array[u8, 1]` ([decision 0036](0036-files-and-processes.md)). Nothing turns one into the
other. That blocks the text modes of `gzip.open` and `lzma.open`
([decision 0049](0049-compression.md)): `gzip.open("log.txt.gz", "rt")`, the usual way to read a
compressed text file in Python, gives lines of `str`, but the decompressor gives bytes.

Python has `text.encode()` and `data.decode()`, each taking an encoding, UTF-8 by default, and an
`errors=` policy. `gzip.open` and `lzma.open` in a mode with `t` wrap the binary file in an
`io.TextIOWrapper`, which decodes as it reads, encodes as it writes, and turns `"\r\n"` and
`"\r"` into `"\n"` on reading.

Collins chose on 2026-11-14 at 05:35 UTC (card 33, "Add UTF-8 text now"): add `str.encode()` and
`Array[u8, 1].decode()` for UTF-8, with `ValueError` for bytes that are not UTF-8, and the text
modes of `gzip.open` and `lzma.open`, the class picked from the mode literal.

## Decision

1. **`text.encode()`** gives an `Array[u8, 1]` of the text's UTF-8 bytes, a new array. It cannot
   fail. `encoding="utf-8"` may be written, as for `open`; any other encoding is a compile error.
2. **`data.decode()`**, on an `Array[u8, 1]`, gives a `str` of the same bytes. Bytes that are not
   UTF-8 raise `ValueError` with Python's message, such as
   `'utf-8' codec can't decode byte 0xff in position 0: invalid start byte`. Python raises
   `UnicodeDecodeError`, a subclass of `ValueError`; Baryon has no subclasses, so
   `except ValueError` catches both. `encoding="utf-8"` may be written; any other is a compile
   error. There is no `errors=` argument.
3. **The text modes.** `gzip.open(path, mode)` and `lzma.open(path, mode)` with `"rt"`, and
   `gzip.open` with `"wt"`, `"at"` and `"xt"`, give a text file: `gzip.GzipTextFile` or
   `lzma.LZMATextFile`. The checker picks the class from the mode, which must then be a string
   literal; a binary mode, or a mode that is not a literal, gives `GzipFile` or `LZMAFile` as
   today. The text file has the methods of a `File` opened the same way: `read()`, `readline()`,
   `readlines()`, `for line in f`, `write(text)`, `close()` and `with`.
4. **Reading text** decodes as it reads, holding back the bytes of a character cut at the end of
   a block, and turns `"\r\n"` and `"\r"` into `"\n"`, as Python's default `newline=None` does.
   Bytes that are not UTF-8 raise `ValueError` from the read that reaches them.
5. **Writing text** writes the text's bytes as they are, as a `File` does.

## Alternatives

- **Binary only until a `bytes` type exists.** A compressed text file would stay out of reach,
  and a `bytes` type is a core type of its own with literals, methods and printing, which
  decision 0036 put off.
- **Other encodings and `errors=`.** Latin-1 and `errors="replace"` are the next most common.
  They are left out until a program needs them; each is a separate choice.
- **One class for both modes, with `read` giving bytes or text by a flag.** A method's return
  type would depend on a value, which the checker cannot see; two classes keep each method's type
  fixed.

## Consequences

- `encode` and `decode` are the bridge between text and every binary format: a program can now
  write a header line into a binary file, or read one out of it.
- `gzip.open(path, "rt")` reads a compressed text file line by line, as in Python.
- `gzip.open` and `lzma.open` are typed by their mode literal, as `open` is.
