# LZ4 raw blocks written by the LZ4 library

These blocks were written by the `lz4` package 4.0.2 for Python 3.12, which calls the LZ4
library 1.9.4, on Linux x86-64, from the table in [`../zlib/text.txt`](../zlib/text.txt). The
build tests in [`../../build/lz4.rs`](../../build/lz4.rs) decompress them with the standard
library's private LZ4 codec, `<std>/parquet_lz4.byn`, and compare the result with the table,
so the tests show that Baryon reads the blocks the reference library writes (decision 0049)
without that library running in the tests.

`text.lz4` is the table, and `long.lz4` forty copies of it; `noise.lz4` is 300 random bytes,
literals whose count goes on past the token; `runs.lz4` is 100,000 times one byte, a match
that reaches into the bytes it writes and whose count takes hundreds of bytes; `empty.lz4` is
no bytes, one token of 0.

The rest are blocks that are not valid: `truncated.lz4` is cut short, `before_start.lz4`
holds a match from before the first byte, `none.lz4` is empty, and `zero_distance.lz4` holds
a match from 0 back. The format calls a distance of 0 a corrupted block, and Baryon refuses
it; the library 1.9.4 does not, and gives 13 bytes for it.

To record them again, run this with Python 3.12 and the `lz4` package in this directory. It
prints each block's uncompressed length, or the error the library raises:

```text
import random
import lz4.block

text = open("../zlib/text.txt", "rb").read()
random.seed(1)
noise = bytes(random.randrange(256) for _ in range(300))
data = {"text": text, "long": text * 40, "noise": noise, "runs": b"x" * 100000, "empty": b""}
blocks = {
    name + ".lz4": lz4.block.compress(plain, store_size=False) for name, plain in data.items()
}
sizes = {name + ".lz4": len(plain) for name, plain in data.items()}
blocks["truncated.lz4"] = blocks["text.lz4"][:-10]
blocks["zero_distance.lz4"] = bytes([0x40]) + b"abcd" + bytes([0, 0, 0x50]) + b"vwxyz"
blocks["before_start.lz4"] = bytes([0x40]) + b"abcd" + bytes([5, 0, 0x50]) + b"vwxyz"
blocks["none.lz4"] = b""
for name, block in blocks.items():
    try:
        print(name, len(lz4.block.decompress(block, uncompressed_size=sizes.get(name, 11600))))
    except Exception as error:
        print(name, type(error).__name__, error)
    with open(name, "wb") as file:
        file.write(block)
```
