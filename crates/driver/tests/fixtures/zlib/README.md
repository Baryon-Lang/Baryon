# Deflate streams written by Python's `zlib`

These files were written by Python 3.13's `zlib` and `gzip` modules (zlib 1.3) on Linux
x86-64. The build tests in [`../../build/zlib.rs`](../../build/zlib.rs) decompress them with
Baryon's `zlib` module and compare the result with `text.txt`, so the tests show that Baryon
reads the streams Python writes (decision 0049) without Python running in the tests.
`truncated.z`, `check.z`, `block.z` and `dictionary.z` are the corrupt and unsupported cases:
half a stream, a wrong Adler-32, a block of the reserved type 3, and a stream that needs a
preset dictionary.

To record them again, run this in this directory:

```text
import gzip, zlib

text = "".join(f"{i:4d} {i * i:8d} {1.0 / (i + 1):.12f}\n" for i in range(400)).encode()
with open("text.txt", "wb") as file:
    file.write(text)
for level in (0, 1, 6, 9):
    with open(f"level{level}.z", "wb") as file:
        file.write(zlib.compress(text, level))
fixed = zlib.compressobj(6, zlib.DEFLATED, 15, 9, zlib.Z_FIXED)
with open("fixed.z", "wb") as file:
    file.write(fixed.compress(text) + fixed.flush())
raw = zlib.compressobj(6, zlib.DEFLATED, -15)
with open("raw.deflate", "wb") as file:
    file.write(raw.compress(text) + raw.flush())
with open("member.gz", "wb") as file:
    file.write(gzip.compress(text, mtime=0))
stream = zlib.compress(text)
with open("truncated.z", "wb") as file:
    file.write(stream[: len(stream) // 2])
with open("check.z", "wb") as file:
    file.write(stream[:-1] + bytes([stream[-1] ^ 1]))
with open("block.z", "wb") as file:
    file.write(b"\x78\x9c\x07")
dictionary = zlib.compressobj(zdict=b"0123456789")
with open("dictionary.z", "wb") as file:
    file.write(dictionary.compress(text) + dictionary.flush())
```
