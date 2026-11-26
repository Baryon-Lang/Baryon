# Parquet files for the reader's tests

The build tests in [`../../build/frame_parquet.rs`](../../build/frame_parquet.rs) read these
files with `DataFrame.read_parquet` (roadmap 3.14.6.3), so the tests show that Baryon reads
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
- `pages_v2.parquet` and `pages_v2_gzip.parquet` hold ten rows in data pages of version 2:
  `n`, an `INT64` through a dictionary, in one page of bit-packed runs and one of repeated
  runs; `s`, text in two pages; `b`, booleans stored as runs; and `x`, a `DOUBLE` whose
  pages say they are not compressed, though the gzip file's other pages are.
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

The files of version 2 pages come from this script, run as `python pages.py pages_v2.parquet 0`
and `python pages.py pages_v2_gzip.parquet 2`, the number being the codec:

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

write(sys.argv[1], int(sys.argv[2]))
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
