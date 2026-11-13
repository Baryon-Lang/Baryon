# bzip2 streams written by Python's `bz2`

These files were written by Python 3.14's `bz2` module (libbzip2 1.0.8) on Linux x86-64, from
the table in [`../zlib/text.txt`](../zlib/text.txt). The build tests in
[`../../build/bz2.rs`](../../build/bz2.rs) decompress them with Baryon's `bz2` module and
compare the result with the table, so the tests show that Baryon reads the streams Python
writes (decision 0049) without Python running in the tests.

`level9.bz2` is the table at level 9, and `level1.bz2` forty copies of it at level 1, in
blocks of 100,000 bytes; `streams.bz2` is two streams, the table's first 5,000 bytes and the
rest; `trailing.bz2` is a stream followed by bytes that start no other, which
Python ignores. The rest are the cases Python refuses: a stream cut short, a block whose CRC
has one bit flipped, and a file that is not bzip2.

To record them again, run this with Python 3.14 in this directory:

```text
import bz2

text = open("../zlib/text.txt", "rb").read()
streams = {
    "level9.bz2": bz2.compress(text),
    "level1.bz2": bz2.compress(text * 40, 1),
    "streams.bz2": bz2.compress(text[:5000]) + bz2.compress(text[5000:], 5),
    "trailing.bz2": bz2.compress(text) + b"\x00\x00junk",
}
streams["truncated.bz2"] = streams["level9.bz2"][:-10]
damaged = bytearray(streams["level9.bz2"])
damaged[10] ^= 1
streams["crc.bz2"] = bytes(damaged)
streams["junk.bz2"] = b"PK\x03\x04" + bytes(20)
for name, stream in streams.items():
    with open(name, "wb") as file:
        file.write(stream)
```
