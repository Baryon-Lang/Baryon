# Zstandard frames written by Python's `compression.zstd`

These files were written by Python 3.14's `compression.zstd` module (zstd 1.5) on Linux
x86-64, from the table in [`../zlib/text.txt`](../zlib/text.txt). The build tests in
[`../../build/zstd.rs`](../../build/zstd.rs) decompress them with Baryon's `compression.zstd`
module and compare the result with the table, so the tests show that Baryon reads the frames
Python writes (decision 0049) without Python running in the tests.

`level3.zst`, `level19.zst` and `fast.zst` are the table at levels 3, 19 and -5;
`checked.zst` and `unsized.zst` are forty copies of it, one frame with a content checksum and
one with no content size; `frames.zst` is two frames with a skippable frame between them. The
rest are the cases Python refuses: a wrong checksum, a frame cut short, two zero bytes after a
frame, a frame compressed with a dictionary, a reserved header bit set, and a window of 256 MiB.

To record them again, run this with Python 3.14 in this directory:

```text
from compression import zstd
from compression.zstd import CompressionParameter as P

text = open("../zlib/text.txt", "rb").read()
long = text * 40
frames = {
    "level3.zst": zstd.compress(text),
    "level19.zst": zstd.compress(text, 19),
    "fast.zst": zstd.compress(text, -5),
    "checked.zst": zstd.compress(long, options={P.checksum_flag: 1, P.compression_level: 9}),
    "unsized.zst": zstd.compress(long, options={P.content_size_flag: 0}),
}
first = zstd.compress(text[:5000])
skippable = b"\x50\x2a\x4d\x18\x03\x00\x00\x00abc"
frames["frames.zst"] = first + skippable + zstd.compress(text[5000:], 9)
checked = bytearray(frames["checked.zst"])
checked[-1] ^= 1
frames["checksum.zst"] = bytes(checked)
frames["truncated.zst"] = frames["level3.zst"][:-10]
frames["junk.zst"] = frames["level3.zst"] + b"\x00\x00"
dictionary = zstd.train_dict([text[i : i + 300] for i in range(0, 11000, 100)], 2000)
frames["dictionary.zst"] = zstd.compress(text, zstd_dict=dictionary)
reserved = bytearray(frames["level3.zst"])
reserved[4] |= 8
frames["reserved.zst"] = bytes(reserved)
window = bytearray(frames["unsized.zst"])
window[5] = 0x90
frames["window.zst"] = bytes(window)
for name, frame in frames.items():
    with open(name, "wb") as file:
        file.write(frame)
```
