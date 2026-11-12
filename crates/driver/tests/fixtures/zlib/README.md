# Deflate streams written by Python's `zlib`

These files were written by Python 3.13's `zlib` and `gzip` modules (zlib 1.3) on Linux
x86-64. The build tests in [`../../build/zlib.rs`](../../build/zlib.rs) decompress them with
Baryon's `zlib` and `gzip` modules and compare the result with `text.txt`, so the tests show that Baryon
reads the streams Python writes (decision 0049) without Python running in the tests.
`truncated.z`, `check.z`, `block.z` and `dictionary.z` are the corrupt and unsupported cases:
half a stream, a wrong Adler-32, a block of the reserved type 3, and a stream that needs a
preset dictionary. `members.gz` is two gzip members with zero bytes between and after them, and
`fields.gz` a member whose header holds every optional field: an extra field, a file name, a
comment and a header CRC.

To record them again, run this in this directory:

```text
import gzip, struct, zlib

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
raw_body = raw.compress(text) + raw.flush()
with open("raw.deflate", "wb") as file:
    file.write(raw_body)
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
with open("members.gz", "wb") as file:
    first = gzip.compress(text[:5000], mtime=0)
    file.write(first + b"\0\0\0" + gzip.compress(text[5000:], 6, mtime=0) + b"\0\0")
header = b"\x1f\x8b\x08\x1e" + struct.pack("<I", 7) + b"\x00\xff" + struct.pack("<H", 4)
header += b"AB\x00\x01" + b"table.txt\x00" + b"a comment\x00"
header += struct.pack("<H", zlib.crc32(header) & 0xFFFF)
with open("fields.gz", "wb") as file:
    file.write(header + raw_body + struct.pack("<II", zlib.crc32(text), len(text)))
```
