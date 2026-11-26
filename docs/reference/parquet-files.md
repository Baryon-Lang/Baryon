# Parquet files

[What builds today](status.md#data-files) says which parts of this chapter the compiler builds
into programs yet.

A Parquet file is a table stored column by column, compressed, with the name and type of each
column written in the file. Data warehouses, Spark, pandas and polars all write it, and it is
the usual format for a table too large to keep as text. `DataFrame.read_parquet(path)` reads
one into a [`DataFrame`](dataframes.md) and `df.to_parquet(path)` writes a frame as one, with
the names pandas has ([decision 0045](../decisions/0045-dataframe.md), roadmap 3.14.6.3 and
3.14.6.4). The reader and the writer are written in Baryon from the format's specification,
so a program that uses Parquet needs no other library.

## Reading a file

With `trades.parquet` holding the columns `symbol`, `price`, `volume` and `venue`, this
program reads three of them:

```python
class Trade(Struct):
    symbol: str
    price: f64 | None
    volume: i64


def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")
    print(trades)
    print(trades.price.mean(), trades.volume.sum())
```

```text
  symbol  price  volume
0   AAPL  189.5     100
1   MSFT   <NA>     250
2  BRK,B  171.0      50
180.25 400
```

- **The schema comes from the program.** The file names its columns and their types, but a
  compiled program must know them before it runs, so the frame's type is written down, as
  for [`read_csv`](csv-files.md#reading-a-file): a struct with one field for each column
  wanted, and an annotation, a parameter or a return type that names `DataFrame[Trade]`.
  Without one, `DataFrame.read_parquet(path)` is a compile error that says so.
- **Columns are matched by name.** Each field is read from the file's column of the field's
  name, wherever it stands in the file. A column the schema does not name, `venue` above, is
  never decoded, which is what makes a wide file cheap to read in part.
- **A field's type says what its column may hold.** The file's own type must be one the
  field reads, and the reader checks it before it reads a value:

  | Field | The file's column holds |
  | --- | --- |
  | `f64` | 64-bit floats (`DOUBLE`) or 32-bit floats (`FLOAT`) |
  | `f32` | 32-bit floats (`FLOAT`) |
  | `i8` to `i64`, `u8` to `u64` | whole numbers of 32 or 64 bits (`INT32`, `INT64`), signed or unsigned, each value within the field's range |
  | `bool` | booleans (`BOOLEAN`) |
  | `str` | bytes (`BYTE_ARRAY`) that are UTF-8 text, whether or not the file marks them as text |

  The names in capitals are the ones the Parquet format gives its types, and an error names
  them. A column of whole numbers is not read into a field of floats, nor floats into whole
  numbers: write the field with the kind of number the file holds and convert the column
  afterwards.
- **A missing value is `None`** in a column whose field is a `T | None`, a
  [missing value](missing-values-in-frames.md). In a column of `f64` or `f32` with no `None`
  it is NaN, as `read_csv` reads an empty cell. In a column of whole numbers, of `bool` or of
  `str` with no `None` it is an error, since those have no value for it. A NaN the file
  stores stays NaN.
- **Every row group is read**, one after another, into one frame. A row group is a slice of
  the rows that a writer stores together; a large file has many.

The reader has what the common writers use by default:

- compression with Snappy, gzip, Zstandard and LZ4 (the raw block form, `LZ4_RAW`), and
  files with none, through the codecs of [Compression](compression.md);
- values stored plainly (`PLAIN`), through a dictionary (`RLE_DICTIONARY` and the older
  `PLAIN_DICTIONARY`), and booleans stored as runs (`RLE`);
- the encodings writers choose for the format's version 2: whole numbers as packed
  differences (`DELTA_BINARY_PACKED`), text as lengths then bytes or as shared prefixes
  (`DELTA_LENGTH_BYTE_ARRAY`, `DELTA_BYTE_ARRAY`), and numbers as split byte streams
  (`BYTE_STREAM_SPLIT`);
- data pages of version 1 and version 2.

## Writing a file

`df.to_parquet(path)` writes a frame as a Parquet file and gives nothing back. A file that is
there already is replaced.

```python
def main() -> Raises[None, OSError]:
    trades = DataFrame({
        "symbol": Array(["AAPL", "MSFT", "BRK,B"]),
        "price": Array([189.5, None, 171.0]),
        "volume": Array([100, 250, 50]),
    })
    trades.to_parquet("trades.parquet")
```

pandas, polars and any other Parquet reader open the file and find the same columns, with
the same types:

- **Each column keeps its type.** An `f64` is a `DOUBLE` and an `f32` a `FLOAT`. An `i64` or
  a `u64` is an `INT64` and a narrower whole number an `INT32`, each marked with its width
  and whether it has a sign, so an `i8` reads back as an `int8` and a `u64` as a `uint64`.
  A `bool` is a `BOOLEAN`, and a `str` is bytes marked as UTF-8 text.
- **`None` is a missing value**, in a column of any type that may hold it; such a column is
  marked optional and the others required. A NaN is written as a NaN, so the two stay
  apart, and an empty text stays apart from a missing one. Every float comes back bit for
  bit, except that any NaN is written as the one quiet NaN.
- **The pages are compressed with Snappy**, the codec pandas uses when none is named.
- **The rows are cut into row groups** of 1,048,576 rows, which is what pyarrow writes, and
  each column of a group into pages of 65,536 rows. A frame of no rows writes a file with
  the schema and no row group.
- **Text is written through a dictionary**, as pandas and polars write it: each distinct
  text of a row group once, and the column as small numbers that point at them
  (`RLE_DICTIONARY`). A column of symbols or of country names then takes a few bits for
  each row. A row group whose distinct texts would take more than 1,048,576 bytes, pyarrow's
  limit, is written plainly, so a column of names that are all different costs nothing
  more.
- **Numbers that repeat are written through a dictionary too**: a column of floats or of
  whole numbers with few distinct values, such as a year, a count or a category code. A
  row group is written plainly when the dictionary and the numbers that point into it
  would not be smaller than the values themselves, when the distinct values would take
  more than 1,048,576 bytes or half of the values' own bytes, or when nearly every one of
  its first numbers is new, as in a column of ids or of measurements. Numbers are told
  apart by their bits, so `0.0` and `-0.0` stay two values. Booleans are always plain.
- **Each column of a row group carries its statistics**: how many values are missing, the
  smallest and the largest. A reader asked for the rows where `price > 100` passes over a
  row group whose largest price is below that, so a search of a large file reads less.
  They are the statistics polars writes for the same frame: NaN is left out, a zero is
  written as `-0.0` when it is the smallest and `0.0` when the largest, and text is ordered
  by its UTF-8 bytes. A text longer than 4,096 bytes is not written as a smallest or
  largest value.

The writer has columns of `f64` and `f32`, of every whole number type, of `bool` and of
`str`, and each of them with `| None`. A column of another type, such as complex numbers, is
a compile error that names it.

## Errors

`to_parquet` can fail with `OSError` alone, when the file cannot be made or written.
`DataFrame.read_parquet` can fail in two ways, and the function that calls it handles both or
lists them in its `Raises`, as with any [error](errors.md).

- `OSError` when the file cannot be opened or read, or a page's compressed bytes cannot be
  decompressed.
- `ValueError` when the file is not what the schema says. Its `message` names the file and
  the column:

  | The file | `message` |
  | --- | --- |
  | is not a Parquet file | `trades.parquet: not a Parquet file` |
  | ends early | `trades.parquet: the file is cut short, with no metadata at its end` |
  | has no column a field names | `trades.parquet: the file has no column 'volume'` |
  | holds another kind of value than the field | `trades.parquet: the column 'price' holds text, which is not read into a column of f64` |
  | holds a number outside the range of the field's type | `trades.parquet: column 'lots', row 2: could not read 300 as u8` |
  | has a missing value where the field has no `None` | `trades.parquet: column 'volume', row 2: the value is missing, and the column's type has no None` |
  | holds a date, a time, a timestamp or a decimal | `trades.parquet: the column 'when' holds a date, a time, a decimal or another kind of value that is not read yet` |
  | holds a list, a map or a struct | `trades.parquet: the column 'items' is nested, which is not read yet` |
  | uses a codec the reader lacks | `trades.parquet: the file is compressed with Brotli, which is not read yet` |
  | is damaged | `trades.parquet: the metadata of the file is damaged`, or `the pages of the column 'price' are damaged` |

  Rows count from 1. A damaged file is an error and never a read outside the file's bytes,
  but the reader does not check the pages' checksums, so a file whose values were changed
  without breaking its structure is read as it stands.

## Cost

The whole file is read into memory, then each column the schema names is decompressed and
decoded, one at a time, and the file's bytes are freed before `read_parquet` returns. While a
column is decoded its values are held twice. The columns of the frame are as large as the
same columns built any other way.

A file of one million rows and fifteen columns of random values, one of each type the
reader has, took 0.32 s to read with no compression on the development machine, 0.37 s with
LZ4, 0.38 s with Snappy, 0.70 s with Zstandard and 0.84 s with gzip, in a program built with
`--release`, where polars 2.1.0 took 0.02 to 0.05 s. What is left of the gap is the design:
the reader decodes on one thread and rebuilds each number from its bytes by arithmetic, a
value at a time. Built without `--release` the same program takes about four times as long.

`to_parquet` holds each column as plain bytes and again as compressed pages before it puts
the file together, about twice the frame's size besides the frame, and frees it all before
it returns. Writing the same million rows took about 0.7 s, built with `--release`, where
polars took 0.10 s, and the file was 55.4 MB, where polars made 58.6 MB. The statistics are
about a tenth of that time.

## Differences from pandas

- **The types are given, never taken from the file.** pandas gives each column the type the
  file names. Here the schema says which columns are read and what each holds, and a file
  that does not fit is an error when it is read.
- **The schema chooses the columns.** pandas has `columns=` for reading some of them. Here a
  column the schema does not name is skipped, and one it names that the file lacks is an
  error.
- **A number too large for its field is an error**, never wrapped around.
- **No keywords yet.** `columns=`, `engine=`, `filters=` and the rest of `read_parquet`, and
  `compression=`, `index=` and the rest of `to_parquet`, are compile errors that name the
  keyword, never ignored.
- **No index.** A file pandas wrote with a named index holds it as one more column, which a
  field of that name reads like any other. A frame has no index here
  ([decision 0045](../decisions/0045-dataframe.md)), so `to_parquet` writes none, and the
  file carries none of the notes pandas adds for itself: pandas reads it with a plain row
  index and the types the file names.
- **`to_parquet` needs a path.** In pandas, `df.to_parquet()` with no path gives the bytes
  back.

## Not built yet

- Writing with another codec than Snappy, and the page indexes some writers add beside the
  statistics.
- Columns of dates, times, timestamps and decimals, and of bytes that are not text: a frame
  has no column type for them yet.
- Nested columns: lists, maps and structs.
- The Brotli, LZO and older LZ4 codecs.
- A column whose name cannot be a field's name, such as one with a space or a colon in it.
- Reading some of the row groups, or a file larger than memory.
- Tests with pyarrow. Two files written by pyarrow 26.0.1 were read here, and pyarrow read a
  file written here with every value matching, but both were checked by hand and not in the
  tests. The tests read files written by polars, files built by hand from the specification
  and ten files of Apache's `parquet-testing` collection, written by Impala, by parquet-mr
  and by Arrow's C++ library, and polars reads the files written here.
