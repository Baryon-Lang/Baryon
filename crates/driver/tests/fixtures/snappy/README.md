# Snappy raw blocks written by Google's library

These blocks were written by the `python-snappy` package 0.5.3 on Python 3.12, which calls
Google's Snappy library 1.1.10, on Linux x86-64, from the table in
[`../zlib/text.txt`](../zlib/text.txt). The build tests in
[`../../build/snappy.rs`](../../build/snappy.rs) decompress them with the standard library's
private Snappy codec, `<std>/parquet_snappy.byn`, and compare the result with the table, so the
tests show that Baryon reads the blocks the reference library writes (decision 0049) without
that library running in the tests.

`text.snappy` is the table, and `long.snappy` forty copies of it, more than one 65,536-byte
piece; `noise.snappy` is 300 random bytes, one literal whose length takes two bytes;
`runs.snappy` is 100,000 times one byte, copies that reach into the bytes they write;
`empty.snappy` is no bytes. `far.snappy` is put together by hand, since the library never
writes a copy with a four-byte distance: 8 bytes, 1,093 copies of 64 bytes, and a copy of 40
bytes from 69,000 back, 70,000 bytes in all, which the library reads.

The rest are blocks the library refuses: `truncated.snappy` is cut short, `short.snappy` and
`long_length.snappy` say one byte more and one byte fewer than their elements give,
`zero_distance.snappy` and `before_start.snappy` hold a copy from 0 back and from before the
first byte, `varint.snappy` has a length of six bytes, `huge.snappy` says 4 GiB and holds one
byte, and `none.snappy` is empty.

To record them again, run this with Python 3.12 and `python-snappy` in this directory. It
prints each block's uncompressed length, or the error the library raises:

```text
import random
import snappy

text = open("../zlib/text.txt", "rb").read()
random.seed(1)
noise = bytes(random.randrange(256) for _ in range(300))
far = bytearray([0xF0, 0xA2, 0x04, 7 << 2]) + b"abcdefgh"
far += bytes([2 | (63 << 2), 8, 0]) * 1093
far += bytes([3 | (39 << 2), 0x88, 0x0D, 0x01, 0x00])
blocks = {
    "text.snappy": snappy.compress(text),
    "long.snappy": snappy.compress(text * 40),
    "noise.snappy": snappy.compress(noise),
    "runs.snappy": snappy.compress(b"x" * 100000),
    "empty.snappy": snappy.compress(b""),
    "far.snappy": bytes(far),
}
good = blocks["text.snappy"]
blocks["truncated.snappy"] = good[:-10]
blocks["short.snappy"] = bytes([0xD1, 0x5A]) + good[2:]
blocks["long_length.snappy"] = bytes([0xCF, 0x5A]) + good[2:]
blocks["zero_distance.snappy"] = bytes([8, 3 << 2]) + b"abcd" + bytes([1, 0])
blocks["before_start.snappy"] = bytes([8, 3 << 2]) + b"abcd" + bytes([1, 5])
blocks["varint.snappy"] = bytes([0x80] * 5 + [1])
blocks["huge.snappy"] = bytes([0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0])
blocks["none.snappy"] = b""
for name, block in blocks.items():
    try:
        print(name, len(snappy.uncompress(block)))
    except Exception as error:
        print(name, type(error).__name__, error)
    with open(name, "wb") as file:
        file.write(block)
```
