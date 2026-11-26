# Parquet files for the reader's tests

The build tests in [`../../build/frame_parquet.rs`](../../build/frame_parquet.rs) read these
files with `DataFrame.read_parquet` (roadmap 3.14.6.3) and compare what `to_parquet` writes
(roadmap 3.14.6.4) with one of them, so the tests show that Baryon reads
the files another library writes without that library running in the tests. The `types_*`
files and `other.parquet` were written by polars 2.1.0 on Python 3.12, Linux x86-64. The
`pages_v2*` files were put together by hand, since polars writes data pages of version 1
only; polars reads from them the values the tests expect.

- `types_uncompressed.parquet`, `types_snappy.parquet`, `types_gzip.parquet`,
  `types_zstd.parquet` and `types_lz4.parquet` hold the same 120 rows, one file for each
  codec, in three row groups whose pages hold a few values each. The columns: `f` (`DOUBLE`,
  with missing values, infinities, signed zero, the smallest and the largest values), `g`
  (`FLOAT`), `a` to `d` (`INT32` and `INT64` marked as 8, 16, 32 and 64 bits, signed), `p` to
  `s` (the same, unsigned), `t` and `u` (`BOOLEAN`), `w` and `x` (text with commas, quotes,
  line ends and letters of several bytes), `z` (nothing but missing values) and `extra`,
  which the tests' schema leaves out. The whole numbers and `x` go through dictionaries and
  the rest are stored plainly.
- `types.csv` is those rows as CSV, written by the same script from the values it gave
  polars, with `<none>` for a missing value. The tests compare what Baryon reads, written
  with `to_csv`, with it byte for byte.
- `types_written.parquet` is what `to_parquet` wrote for the frame read from
  `types_snappy.parquet`, less `extra`. polars 2.1.0 reads it as a table equal to that one,
  with the same type for every column and every float the same bit for bit. Its two columns
  of text, `w` and `x`, and four of its columns of numbers each have a dictionary page, the
  others being too varied to gain from one, and the statistics of every column
  are those polars writes for the same table. The test that
  writes the frame again compares its bytes with this file, so a change to the writer shows.
  After a change that is meant, write the file again with the test's program and check it
  with polars: `a.equals(b, null_equal=True)` and `a.schema == b.schema`, where `a` is
  `types_snappy.parquet` without `extra` and `b` the new file.
- `statistics.parquet` is what `to_parquet` wrote for the nine columns of three rows in
  `tests/build/frame_parquet_statistics.rs`, kept for its statistics: zeros of each sign,
  NaN, missing values, a negative whole number, booleans and text. polars 2.1.0 read it and
  wrote the table again with `statistics=True`, and the missing count, the smallest value
  and the largest value of every column were the same bytes in both files, as was the
  `column_orders` entry of the metadata. After a change that is meant, write the file again
  with the test's program and make the same comparison; a short script that walks the
  Thrift metadata is enough.
- `pages_v2.parquet` and `pages_v2_gzip.parquet` hold ten rows in data pages of version 2:
  `n`, an `INT64` through a dictionary, in one page of bit-packed runs and one of repeated
  runs; `s`, text in two pages; `b`, booleans stored as runs; and `x`, a `DOUBLE` whose
  pages say they are not compressed, though the gzip file's other pages are.
- `delta.parquet` holds 300 rows in the encodings writers choose for the format's version 2,
  each column in two pages of version 2: `n`, `m` and `k` as `DELTA_BINARY_PACKED`, with the
  ends of the `INT64` and `INT32` ranges, whose differences wrap around, and one column of
  a single value; `s` as `DELTA_BYTE_ARRAY`; `t` as `DELTA_LENGTH_BYTE_ARRAY`, with missing
  values; and `x` and `y`, a `DOUBLE` and a `FLOAT`, as `BYTE_STREAM_SPLIT`. polars writes
  none of these, so the file is put together by hand; polars reads from it the values in
  `delta.csv`, which the same script writes.
- `other.parquet` holds three rows of what a schema can get wrong: a date, a timestamp, a
  decimal, a list, a column of whole numbers with a missing value, and one with a value
  past a `u8`.

To record the `types_*` files and `types.csv` again, run this with polars in this directory:

```text
import csv, io, math, random, struct
import polars as pl

random.seed(7)
n = 120
words = ["", "a", "héllo", "日本語", "x,y", 'q"r', "line\nbreak", "long" * 20]


def maybe(value):
    return None if random.random() < 0.2 else value


def single(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


doubles = [0.0, -0.0, math.inf, -math.inf, 5e-324, 1.7976931348623157e308,
           2.2250738585072014e-308, 1.5e-310]
singles = [1.5, -0.0, 1e-45, 3.4028234663852886e38]
columns = {
    "f": [maybe(random.choice(doubles + [random.uniform(-1e6, 1e6)])) for _ in range(n)],
    "g": [single(random.choice(singles + [random.uniform(-1e3, 1e3)])) for _ in range(n)],
    "a": [maybe(random.randint(-128, 127)) for _ in range(n)],
    "b": [random.randint(-32768, 32767) for _ in range(n)],
    "c": [maybe(random.randint(-2**31, 2**31 - 1)) for _ in range(n)],
    "d": [maybe(random.choice([random.randint(-2**63, 2**63 - 1), 3])) for _ in range(n)],
    "p": [random.randint(0, 255) for _ in range(n)],
    "q": [maybe(random.randint(0, 65535)) for _ in range(n)],
    "r": [random.randint(0, 2**32 - 1) for _ in range(n)],
    "s": [maybe(random.choice([random.randint(0, 2**64 - 1), 7])) for _ in range(n)],
    "t": [maybe(random.random() < 0.5) for _ in range(n)],
    "u": [random.random() < 0.5 for _ in range(n)],
    "w": [maybe(random.choice(words)) for _ in range(n)],
    "x": [random.choice(words) + str(i % 7) for i in range(n)],
    "z": [None for _ in range(n)],
    "extra": [i for i in range(n)],
}
types = {
    "f": pl.Float64, "g": pl.Float32, "a": pl.Int8, "b": pl.Int16, "c": pl.Int32,
    "d": pl.Int64, "p": pl.UInt8, "q": pl.UInt16, "r": pl.UInt32, "s": pl.UInt64,
    "t": pl.Boolean, "u": pl.Boolean, "w": pl.String, "x": pl.String, "z": pl.Float64,
    "extra": pl.Int64,
}
frame = pl.DataFrame(columns, schema=types)
for codec in ("uncompressed", "snappy", "gzip", "zstd", "lz4"):
    frame.write_parquet(
        f"types_{codec}.parquet", compression=codec, row_group_size=50, data_page_size=64,
        statistics=False,
    )


def shown(name, value):
    if value is None:
        return "<none>"
    if name == "g":
        return str(pl.Series([value], dtype=pl.Float32).to_numpy()[0])
    return str(value)


out = io.StringIO()
writer = csv.writer(out, lineterminator="\n")
names = [name for name in columns if name != "extra"]
writer.writerow(names)
for row in range(n):
    writer.writerow([shown(name, columns[name][row]) for name in names])
open("types.csv", "w", newline="").write(out.getvalue())
```

The files of version 2 pages come from this script, saved as `pages.py` and run as
`python pages.py pages_v2.parquet 0` and `python pages.py pages_v2_gzip.parquet 2`, the
number being the codec:

```text
"""Writes a Parquet file with version 2 data pages by hand, for reader tests."""
import struct, sys, zlib

def varint(n):
    out = bytearray()
    while True:
        if n < 0x80:
            out.append(n); return bytes(out)
        out.append(n & 0x7F | 0x80); n >>= 7

def zigzag(n): return varint((n << 1) ^ (n >> 63))

class Struct:
    def __init__(self): self.out = bytearray(); self.last = 0
    def head(self, field, kind):
        delta = field - self.last
        self.out += bytes([delta << 4 | kind]) if 0 < delta < 16 else bytes([kind]) + zigzag(field)
        self.last = field
    def i32(self, field, value): self.head(field, 5); self.out += zigzag(value); return self
    def i64(self, field, value): self.head(field, 6); self.out += zigzag(value); return self
    def flag(self, field, value): self.head(field, 1 if value else 2); return self
    def text(self, field, value):
        raw = value.encode(); self.head(field, 8); self.out += varint(len(raw)) + raw; return self
    def struct(self, field, value): self.head(field, 12); self.out += value.done(); return self
    def items(self, field, kind, values):
        self.head(field, 9)
        if len(values) < 15: self.out += bytes([len(values) << 4 | kind])
        else: self.out += bytes([0xF0 | kind]) + varint(len(values))
        for value in values: self.out += value
        return self
    def done(self): return bytes(self.out) + b"\0"

def levels(valid):
    """Definition levels as bit-packed runs of the RLE hybrid, width 1."""
    groups = (len(valid) + 7) // 8
    bits = bytearray(groups)
    for i, there in enumerate(valid):
        if there: bits[i // 8] |= 1 << (i % 8)
    return varint(groups << 1 | 1) + bytes(bits)

def packed(numbers, width):
    """Bit-packed run of the RLE hybrid."""
    groups = (len(numbers) + 7) // 8
    total = 0
    for i, n in enumerate(numbers): total |= n << (i * width)
    return varint(groups << 1 | 1) + total.to_bytes(groups * width, "little")

def runs(numbers, width):
    """The numbers as repeated runs of the RLE hybrid, one for each stretch of equal ones."""
    out = bytearray(); i = 0
    while i < len(numbers):
        j = i
        while j < len(numbers) and numbers[j] == numbers[i]: j += 1
        out += varint((j - i) << 1) + numbers[i].to_bytes((width + 7) // 8, "little")
        i = j
    return bytes(out)

def page_v2(values, nulls, rows, encoding, level_bytes, body, codec, squeeze=True):
    stored = zlib.compress(body, wbits=31) if codec == 2 and squeeze else body
    inner = Struct().i32(1, values).i32(2, nulls).i32(3, rows).i32(4, encoding)
    inner.i32(5, len(level_bytes)).i32(6, 0)
    if not squeeze: inner.flag(7, False)
    head = Struct().i32(1, 3).i32(2, len(level_bytes) + len(body))
    head.i32(3, len(level_bytes) + len(stored)).struct(8, inner)
    return head.done() + level_bytes + stored, len(level_bytes) + len(body)

def dictionary_page(body, count, codec):
    stored = zlib.compress(body, wbits=31) if codec == 2 else body
    head = Struct().i32(1, 2).i32(2, len(body)).i32(3, len(stored))
    head.struct(7, Struct().i32(1, count).i32(2, 0))
    return head.done() + stored, len(body)

def write(path, codec):
    out = bytearray(b"PAR1"); chunks = []
    def chunk(name, kind, encodings, pages, count, dictionary=None):
        start = len(out); raw = 0; first = None
        if dictionary is not None:
            blob, size = dictionary; out.extend(blob); raw += size
            first = start
        data = len(out)
        for blob, size in pages: out.extend(blob); raw += size
        meta = Struct().i32(1, kind).items(2, 5, [zigzag(e) for e in encodings])
        meta.items(3, 8, [varint(len(name)) + name.encode()]).i32(4, codec).i64(5, count)
        meta.i64(6, raw).i64(7, len(out) - start).i64(9, data)
        if first is not None: meta.i64(11, first)
        chunks.append(Struct().i64(2, start).struct(3, meta).done())
    # n: optional int64 through a dictionary, two pages: bit-packed runs, then repeated ones
    table = [10, -20, 9007199254740993]
    first = [0, None, 1, 2, None, 0, 1]; second = [2, 2, None]
    pages = []
    for part, pack in ((first, True), (second, False)):
        valid = [v is not None for v in part]; held = [v for v in part if v is not None]
        body = bytes([2]) + (packed(held, 2) if pack else runs(held, 2))
        marks = levels(valid) if pack else runs([int(v) for v in valid], 1)
        pages.append(page_v2(len(part), valid.count(False), len(part), 8, marks, body, codec))
    plain = b"".join(struct.pack("<q", v) for v in table)
    chunk("n", 2, [0, 3, 8], pages, 10, dictionary_page(plain, 3, codec))
    # s: required text, PLAIN, two pages
    words = ["alpha", "", "héllo", "d,e", "x", "y", "z", "tail", "日本", "end"]
    pages = []
    for part in (words[:7], words[7:]):
        body = b"".join(struct.pack("<i", len(w.encode())) + w.encode() for w in part)
        pages.append(page_v2(len(part), 0, len(part), 0, b"", body, codec))
    chunk("s", 6, [0], pages, 10)
    # b: optional bool as RLE, one page
    flags = [True, None, False, True, True, None, False, False, True, None]
    valid = [v is not None for v in flags]; held = [int(v) for v in flags if v is not None]
    run = packed(held, 1)
    body = struct.pack("<i", len(run)) + run
    chunk("b", 0, [3], [page_v2(10, 3, 10, 3, levels(valid), body, codec)], 10)
    # x: optional float64, PLAIN, pages marked as not compressed
    numbers = [1.5, None, -0.0, 2.5e-300, None, 1e300, 3.0, None, 4.25, 5.0]
    pages = []
    for part in (numbers[:4], numbers[4:]):
        valid = [v is not None for v in part]
        body = b"".join(struct.pack("<d", v) for v in part if v is not None)
        nulls = valid.count(False)
        pages.append(page_v2(len(part), nulls, len(part), 0, levels(valid), body, codec, False))
    chunk("x", 5, [0, 3], pages, 10)
    def leaf(name, kind, repetition, text=False):
        element = Struct().i32(1, kind).i32(3, repetition).text(4, name)
        if text: element.i32(6, 0).struct(10, Struct().struct(1, Struct()))
        return element.done()
    schema = [Struct().text(4, "schema").i32(5, 4).done(), leaf("n", 2, 1), leaf("s", 6, 0, True),
              leaf("b", 0, 1), leaf("x", 5, 1)]
    group = Struct().items(1, 12, chunks).i64(2, len(out) - 4).i64(3, 10).done()
    footer = Struct().i32(1, 2).items(2, 12, schema).i64(3, 10).items(4, 12, [group])
    footer = footer.text(6, "baryon test fixture").done()
    out.extend(footer); out.extend(struct.pack("<i", len(footer))); out.extend(b"PAR1")
    open(path, "wb").write(out)

if __name__ == "__main__":
    write(sys.argv[1], int(sys.argv[2]))
```

`delta.parquet` and `delta.csv` come from this script, saved as `delta.py` beside the one
above saved as `pages.py`, and run as `python delta.py`:

```text
"""Writes `delta.parquet`, a file in the delta encodings and split byte streams, by hand,
and `delta.csv`, its values as CSV. Needs `pages.py` beside it and NumPy."""
import csv, io, random, struct
import numpy
from pages import Struct, levels, page_v2, varint, zigzag


def delta_packed(values, bits):
    """`DELTA_BINARY_PACKED`: blocks of 128 differences in 4 miniblocks of 32."""
    mask = (1 << bits) - 1
    half = 1 << (bits - 1)
    out = bytearray(varint(128) + varint(4) + varint(len(values)))
    out += zigzag(values[0] if values else 0)
    deltas = [((b - a + half) & mask) - half for a, b in zip(values, values[1:])]
    for start in range(0, len(deltas), 128):
        block = deltas[start:start + 128]
        least = min(block)
        out += zigzag(least)
        over = [(d - least) & mask for d in block]
        minis = [over[k:k + 32] for k in range(0, 128, 32)]
        out += bytes(max(v.bit_length() for v in mini) if mini else 0 for mini in minis)
        for mini in minis:
            if mini:
                width = max(v.bit_length() for v in mini)
                total = 0
                for k, value in enumerate(mini):
                    total |= value << (k * width)
                out += total.to_bytes(4 * width, "little")
    return bytes(out)


def split(raw, width):
    """`BYTE_STREAM_SPLIT`: byte 0 of every value, then byte 1 of every value, and on."""
    return b"".join(raw[part::width] for part in range(width))


random.seed(11)
rows = 300
low, high = -(2 ** 63), 2 ** 63 - 1
n = [random.choice([low, high, 0, random.randint(-1000, 1000), random.randint(low, high)])
     if random.random() > 0.15 else None for _ in range(rows)]
m = [random.choice([-(2 ** 31), 2 ** 31 - 1, 5, random.randint(-9, 9)]) for _ in range(rows)]
k = [7] * rows
stems = ["", "inter", "interval", "international", "日本", "日本語", "x"]
s = [random.choice(stems) + random.choice(["", "s", "ly", "-9"]) for _ in range(rows)]
t = [random.choice(stems) * random.randint(0, 3) if random.random() > 0.2 else None
     for _ in range(rows)]
x = [random.choice([0.0, -0.0, 1.5, 5e-324, 1.7976931348623157e308, random.uniform(-9, 9)])
     for _ in range(rows)]
y = [struct.unpack("<f", struct.pack("<f", random.uniform(-9, 9)))[0] for _ in range(rows)]

out = bytearray(b"PAR1")
chunks = []
schema = []


def column(name, kind, repetition, encoding, values, encode, text=False):
    """Adds a column chunk of two pages of version 2, the second three rows long."""
    start = len(out)
    raw = 0
    for part in (values[:-3], values[-3:]):
        valid = [v is not None for v in part]
        held = [v for v in part if v is not None]
        marks = levels(valid) if repetition == 1 else b""
        blob, size = page_v2(len(part), valid.count(False), len(part), encoding, marks,
                             encode(held), 0)
        out.extend(blob)
        raw += size
    meta = Struct().i32(1, kind).items(2, 5, [zigzag(encoding), zigzag(3)])
    meta.items(3, 8, [varint(len(name)) + name.encode()]).i32(4, 0).i64(5, len(values))
    meta.i64(6, raw).i64(7, len(out) - start).i64(9, start)
    chunks.append(Struct().i64(2, start).struct(3, meta).done())
    element = Struct().i32(1, kind).i32(3, repetition).text(4, name)
    if text:
        element.i32(6, 0).struct(10, Struct().struct(1, Struct()))
    schema.append(element.done())


def shared(values):
    """`DELTA_BYTE_ARRAY`: shared prefix lengths, rest lengths, then the rests."""
    raws = [v.encode() for v in values]
    prefixes, rests = [], []
    last = b""
    for value in raws:
        same = 0
        while same < min(len(last), len(value)) and last[same] == value[same]:
            same += 1
        prefixes.append(same)
        rests.append(value[same:])
        last = value
    lengths = delta_packed([len(r) for r in rests], 32)
    return delta_packed(prefixes, 32) + lengths + b"".join(rests)


def lengths(values):
    """`DELTA_LENGTH_BYTE_ARRAY`: the lengths, then the bytes."""
    raws = [v.encode() for v in values]
    return delta_packed([len(r) for r in raws], 32) + b"".join(raws)


column("n", 2, 1, 5, n, lambda held: delta_packed(held, 64))
column("m", 1, 0, 5, m, lambda held: delta_packed(held, 32))
column("k", 2, 0, 5, k, lambda held: delta_packed(held, 64))
column("s", 6, 0, 7, s, shared, True)
column("t", 6, 1, 6, t, lengths, True)
column("x", 5, 0, 9, x, lambda held: split(b"".join(struct.pack("<d", v) for v in held), 8))
column("y", 4, 0, 9, y, lambda held: split(b"".join(struct.pack("<f", v) for v in held), 4))
root = Struct().text(4, "schema").i32(5, len(schema)).done()
group = Struct().items(1, 12, chunks).i64(2, len(out) - 4).i64(3, rows).done()
footer = Struct().i32(1, 2).items(2, 12, [root] + schema).i64(3, rows).items(4, 12, [group])
footer = footer.text(6, "baryon test fixture").done()
out.extend(footer)
out.extend(struct.pack("<i", len(footer)))
out.extend(b"PAR1")
open("delta.parquet", "wb").write(out)

text = io.StringIO()
writer = csv.writer(text, lineterminator="\n")
writer.writerow(["n", "m", "k", "s", "t", "x", "y"])
for row in range(rows):
    cells = [n[row], m[row], k[row], s[row], t[row], x[row], str(numpy.float32(y[row]))]
    writer.writerow(["<none>" if cell is None else cell for cell in cells])
open("delta.csv", "w", newline="").write(text.getvalue())
```

And `other.parquet`:

```text
import datetime
from decimal import Decimal
import polars as pl

pl.DataFrame(
    {
        "when": [datetime.date(2026, 1, 2), datetime.date(2026, 1, 3), None],
        "stamp": [datetime.datetime(2026, 1, 2, 3, 4, 5)] * 3,
        "amount": [Decimal("1.25"), Decimal("2.50"), Decimal("3.75")],
        "items": [[1, 2], [], [3]],
        "count": [1, None, 3],
        "ratio": [0.5, None, 1.5],
        "label": ["a", "b", "c"],
        "big": [1, 300, -1],
    },
    schema={
        "when": pl.Date, "stamp": pl.Datetime("us"), "amount": pl.Decimal(10, 2),
        "items": pl.List(pl.Int64), "count": pl.Int64, "ratio": pl.Float64,
        "label": pl.String, "big": pl.Int64,
    },
).write_parquet("other.parquet", compression="snappy", statistics=False)
```

## `apache/`: files of other writers

Ten files of Apache's [`parquet-testing`](https://github.com/apache/parquet-testing)
collection, copied unchanged from its `data/` directory at commit
`0e00127a431f83388d38eebb22cf644d06e86a15`. They are licensed under the Apache License 2.0
and listed in [`THIRD-PARTY-NOTICES.md`](../../../../../THIRD-PARTY-NOTICES.md); the chief
designer agreed to carry about ten of them on 2026-11-21. They are the reader's check against
files it had no part in writing.

| File | Written by | Holds |
| --- | --- | --- |
| `alltypes_plain.parquet` | Impala 1.3.0 | 8 rows of plain values, not compressed |
| `alltypes_plain.snappy.parquet` | Impala 1.3.0 | 2 rows of plain values, Snappy |
| `alltypes_dictionary.parquet` | Impala 1.3.0 | 2 rows through dictionaries (`PLAIN_DICTIONARY`) |
| `datapage_v2.snappy.parquet` | parquet-mr 1.8.1 | 5 rows in version 2 pages, with a missing text and a list column |
| `delta_binary_packed.parquet` | parquet-mr 1.10.0 | 200 rows of `DELTA_BINARY_PACKED`, one column for each bit width from 0 to 64 |
| `delta_byte_array.parquet` | parquet-mr 1.10.0 | 1,000 rows of text as `DELTA_BYTE_ARRAY`, with missing values |
| `delta_encoding_optional_column.parquet` | parquet-mr 1.10.0 | 100 rows, whole numbers and text in the delta encodings, with missing values |
| `delta_length_byte_array.parquet` | not named in the file | 1,000 rows of text as `DELTA_LENGTH_BYTE_ARRAY`, Zstandard |
| `lz4_raw_compressed.parquet` | Arrow's C++ library (parquet-cpp 1.5.1) | 4 rows, `LZ4_RAW` |
| `rle_boolean_encoding.parquet` | not named in the file | 68 booleans as `RLE` in a version 2 page, with missing values, gzip |

Beside each is a `.csv` of the same name: the frame `read_parquet` gives, written by `to_csv`
with `na_rep="<none>"`. The schema of each is in the test,
`tests/build/frame_parquet_apache.rs`: every column a frame has a type for, each with
`| None`, less the `INT96` timestamp of the `alltypes` files and the list of `datapage_v2`.
Two files are read in part to keep the CSV files small: ten of the 66 columns of
`delta_binary_packed` (bit widths 0, 1, 7, 8, 31, 32, 33, 63 and 64, and `int_value`) and
three of the nine of `delta_byte_array`.

Each CSV was written by Baryon, not by polars, so it is only as good as this check, made
when it was written: the same program also wrote each frame with `to_parquet`, and polars
2.1.0 read that file as a table equal to the columns it reads from the Apache file, missing
values included. The check was run for every readable column of the two wide files as well.

