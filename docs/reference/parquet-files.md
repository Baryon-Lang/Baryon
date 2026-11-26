# Parquet files

[What builds today](status.md#data-files) says which parts of this chapter the compiler builds
into programs yet.

A Parquet file is a table stored column by column, compressed, with the name and type of each
column written in the file. Data warehouses, Spark, pandas and polars all write it, and it is
the usual format for a table too large to keep as text. `DataFrame.read_parquet(path)` reads
one into a [`DataFrame`](dataframes.md), with the name pandas has
([decision 0045](../decisions/0045-dataframe.md), roadmap 3.14.6.3). The reader is written in
Baryon from the format's specification, so a program that reads Parquet needs no other
library. Writing a frame as Parquet, `df.to_parquet(path)`, is not built yet (roadmap
3.14.6.4).

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
- data pages of version 1 and version 2.

## Errors

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
  | uses an encoding or a codec the reader lacks | `trades.parquet: the column 'price' uses the encoding 5, which is not read yet` |
  | is damaged | `trades.parquet: the metadata of the file is damaged`, or `the pages of the column 'price' are damaged` |

  Rows count from 1. A damaged file is an error and never a read outside the file's bytes,
  but the reader does not check the pages' checksums, so a file whose values were changed
  without breaking its structure is read as it stands.

## Cost

The whole file is read into memory, then each column the schema names is decompressed and
decoded, one at a time, and the file's bytes are freed before `read_parquet` returns. While a
column is decoded its values are held twice. The columns of the frame are as large as the
same columns built any other way.

A file of one million rows and fifteen columns, one of each type the reader has, took 1.9 s
to read with no compression on the development machine, 2.2 s with Snappy, 2.0 s with LZ4,
2.7 s with Zstandard and 3.1 s with gzip, where polars 2.1.0 took 0.04 to 0.07 s. The reader
has not been tuned yet: it decodes on one thread and rebuilds each float from its bytes by
arithmetic.

## Differences from pandas

- **The types are given, never taken from the file.** pandas gives each column the type the
  file names. Here the schema says which columns are read and what each holds, and a file
  that does not fit is an error when it is read.
- **The schema chooses the columns.** pandas has `columns=` for reading some of them. Here a
  column the schema does not name is skipped, and one it names that the file lacks is an
  error.
- **A number too large for its field is an error**, never wrapped around.
- **No keywords yet.** `columns=`, `engine=`, `filters=` and the rest are compile errors that
  name the keyword, never ignored.
- **No index.** A file pandas wrote with a named index holds it as one more column, which a
  field of that name reads like any other. A frame has no index here
  ([decision 0045](../decisions/0045-dataframe.md)).

## Not built yet

- Writing: `df.to_parquet(path)` (roadmap 3.14.6.4).
- Columns of dates, times, timestamps and decimals, and of bytes that are not text: a frame
  has no column type for them yet.
- Nested columns: lists, maps and structs.
- The delta encodings (`DELTA_BINARY_PACKED`, `DELTA_LENGTH_BYTE_ARRAY`, `DELTA_BYTE_ARRAY`)
  and `BYTE_STREAM_SPLIT`, which some writers use when asked for the format's version 2, and
  the Brotli, LZO and older LZ4 codecs.
- Reading some of the row groups, or a file larger than memory.
- Files written by pandas with pyarrow have not been tested yet. The tests read files
  written by polars and files built by hand from the specification, and files from Apache's
  `parquet-testing` collection, written by Impala, by parquet-mr and by Arrow's C++ library,
  were read by hand with the values polars reads from them.
