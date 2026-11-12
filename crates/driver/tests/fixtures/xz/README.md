# xz and `.lzma` files written by Python's `lzma`

These files were written by Python 3.14's `lzma` module on Linux x86-64, from the
table in [`../zlib/text.txt`](../zlib/text.txt). The build tests in
[`../../build/lzma.rs`](../../build/lzma.rs) decompress them with Baryon's `lzma` module and
compare the result with the table, so the tests show that Baryon reads the files Python
writes (decision 0049) without Python running in the tests.

`level6.xz` is the table at the default preset with a CRC-64 check; `crc32.xz` is forty copies
of it with a CRC-32 check, `sha256.xz` the table with a SHA-256 check and `unchecked.xz` the
table at preset 9 extreme with no check. `alone.lzma` is the table in the `.lzma` format;
`streams.xz` is two streams, the table's first 5,000 bytes and the rest; `trailing.xz` is a
stream followed by bytes that start no other, which Python ignores. The rest are the cases
Python refuses or Baryon does not decode yet: a stream cut short, a stream with one bit of its
check flipped, a file that is neither format, and `x86.xz`, whose block has the x86 branch
filter before LZMA2.

To record them again, run this with Python 3.14 in this directory:

```text
import lzma

text = open("../zlib/text.txt", "rb").read()
files = {
    "level6.xz": lzma.compress(text),
    "crc32.xz": lzma.compress(text * 40, check=lzma.CHECK_CRC32),
    "sha256.xz": lzma.compress(text, check=lzma.CHECK_SHA256),
    "unchecked.xz": lzma.compress(text, preset=9 | lzma.PRESET_EXTREME, check=lzma.CHECK_NONE),
    "alone.lzma": lzma.compress(text, format=lzma.FORMAT_ALONE),
    "streams.xz": lzma.compress(text[:5000]) + lzma.compress(text[5000:], preset=1),
    "trailing.xz": lzma.compress(text) + b"junk",
}
files["truncated.xz"] = files["level6.xz"][:-10]
damaged = bytearray(files["level6.xz"])
damaged[-20] ^= 1
files["check.xz"] = bytes(damaged)
files["junk.xz"] = b"PK\x03\x04" + bytes(20)
filters = [{"id": lzma.FILTER_X86}, {"id": lzma.FILTER_LZMA2}]
files["x86.xz"] = lzma.compress(text, filters=filters)
for name, stream in files.items():
    with open(name, "wb") as file:
        file.write(stream)
```
