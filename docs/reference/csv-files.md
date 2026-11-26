# CSV files

[What builds today](status.md#data-files) says which parts of this chapter the compiler builds
into programs yet.

A CSV file (comma-separated values) is a table as text: one row on each line, the cells split
by commas, and the first line naming the columns. Spreadsheets, databases and instruments all
write it. `DataFrame.read_csv(path)` reads one into a [`DataFrame`](dataframes.md) and
`df.to_csv(path)` writes a frame as one, with the names pandas has
([decision 0045](../decisions/0045-dataframe.md), roadmap 3.14.4).
A table of numbers alone, with no header, is read into an `Array` with
[`Array.loadtxt`](data-files.md#text-tables).

## Reading a file

With `trades.csv` holding

```text
symbol,price,volume,venue
AAPL,189.5,100,XNAS
MSFT,,250,XNAS
"BRK,B",171.0,50,XNYS
```

this program reads three of its columns:

```python
class Trade(Struct):
    symbol: str
    price: f64 | None
    volume: i64


def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
    print(trades)
    print(trades.price.mean(), trades.volume.sum())
    priced = trades.dropna()
    print((priced.price * priced.volume).sum())
```

```text
  symbol  price  volume
0   AAPL  189.5     100
1   MSFT   <NA>     250
2  BRK,B  171.0      50
180.25 400
27500.0
```

- **The schema comes from the program.** A compiled program cannot learn a file's columns
  when it runs, so the frame's type is written down: a struct with one field for each column
  wanted, and an annotation, a parameter or a return type that names `DataFrame[Trade]`.
  Without one, `DataFrame.read_csv(path)` is a compile error that says so.
  `Array.loadtxt` takes its rank from the context the same way.
- **Columns are matched by name.** Each field is read from the column whose header is the
  field's name, wherever it stands in the file. A column the schema does not name, `venue`
  above, is skipped. When a name is in the header twice, the first column is read.
- **A field's type says how its cells are read.** The reader has the floats `f64` and `f32`,
  every whole number type from `i8` to `u64`, `bool` and `str`, and each of them with
  `| None`.

  | Field | A cell holds |
  | --- | --- |
  | `f64` | a number as Python's `float` reads it, `1.5`, `2.5e3`, `inf`, `-inf` or `nan`, with spaces around it allowed |
  | `f32` | the same, rounded to the nearest `f32` |
  | `i64` | a whole number with an optional sign, `100`, `-3` or `+4`, with spaces around it allowed |
  | `i8`, `i16`, `i32`, `u8`, `u16`, `u32`, `u64` | the same, within the type's range: `-128` to `127` for an `i8`, `0` to `18446744073709551615` for a `u64` |
  | `bool` | `True` or `False`, in that case, all lower case or all upper case |
  | `str` | any text, kept as it is |

- **An empty cell is `None`** in a column whose field is a `T | None`, a
  [missing value](missing-values-in-frames.md). In a column of `f64` it is NaN, as pandas
  reads it. In a column of `str` it is the empty text. In a column of whole numbers or of
  `bool` it is an error, since those have no value for it.
  A row that ends before its last column has empty cells there.

The file is read as pandas reads one with its defaults:

- Cells are split by commas. A cell in double quotes may hold commas and line ends, and a
  double quote written twice, `""`, is one double quote. The quotes are not part of the text.
- A line ends with `"\n"`, `"\r\n"` or `"\r"`. A blank line is skipped.
- The file is UTF-8. A byte-order mark at its start is skipped.

## Writing a file

`df.to_csv(path)` writes a frame as a CSV file and gives nothing back. A file that is there
already is replaced.

```python
def main() -> Raises[None, OSError]:
    trades = DataFrame({
        "symbol": Array(["AAPL", "MSFT", "BRK,B"]),
        "price": Array([189.5, None, 171.0]),
        "volume": Array([100, 250, 50]),
    })
    trades.to_csv("trades.csv")
```

```text
symbol,price,volume
AAPL,189.5,100
MSFT,,250
"BRK,B",171.0,50
```

The bytes are the ones `pandas.DataFrame.to_csv(path, index=False)` writes for the same
columns. A frame has no row index ([decision 0045](../decisions/0045-dataframe.md)), so no
index column is written.

- **The first line names the columns**, in the frame's order, and each row follows on a line
  of its own, ended by `"\n"`. A frame of no rows writes the header alone.
- **A cell is written as `print` writes the value.** An `f64` is the shortest text that reads
  back as the same number, `189.5`, `100.0` or `1e-07`. A whole number of any width is its
  digits, and a `bool` is `True` or `False`.
- **`None` and NaN are empty cells.** `read_csv` reads the empty cell back as `None` into a
  column that may hold it and as NaN into a column of `f64`.
- **Text is quoted only where it must be**: a cell that holds a comma, a double quote or a
  line end goes in double quotes, with each double quote inside written twice. A column's
  name is quoted the same way.
- **A frame of one column writes an empty cell as `""`**, since a blank line is skipped when
  the file is read.

The writer has columns of `f64` and `f32`, of every whole number type, of `bool` and of
`str`, and each of them with `| None`. A column of another type, such as complex numbers, is
a compile error that names it.

An `f32` is written as NumPy writes one, which is what pandas writes: the fewest digits that
read back as the same `f32`, in scientific form from a million up, `1.2345675e+06`, where
`print` keeps `1234567.5`.

Two values do not come back as they were when the file is read again. An empty text in a
column of `str | None` is written as an empty cell, which reads back as `None`. NaN in a
column of `f64 | None` is written as an empty cell too, which reads back as `None`.

## Keywords

Both functions take the path first. Three keywords of pandas are built so far:

| Keyword | Of | Meaning | Default |
| --- | --- | --- | --- |
| `sep=` | both | The character between the cells of a row | `","` |
| `na_values=` | `read_csv` | More cells to read as missing: a `str` or a `list[str]` | none |
| `na_rep=` | `to_csv` | The text written for a missing value: a `str` | `""` |

```python
def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.txt", sep=";", na_values=["NA", "-999"])
    trades.to_csv("trades.tsv", sep="\t", na_rep="NA")
```

- **`sep` is one character, written out.** It is a string literal of one ASCII character,
  such as `";"`, `"|"` or `"\t"`, a tab. A variable, a text of several characters, a quote
  and a backslash are compile errors. pandas also takes a text of several characters, which
  it reads as a pattern.
- **Quotes follow the separator.** A cell is quoted when it is read with the separator
  inside it, and `to_csv` quotes a cell that holds the separator, a quote or a line end. So
  a comma is plain text in a file split by `;`, and the file is the one pandas writes with
  the same `sep`.
- **The wrong separator is an error when the file is read.** A file split by `;` read with
  no `sep` has one column, whose name is the whole first line, so the first field has no
  column and `read_csv` raises `ValueError`.
- **`na_values` adds spellings of a missing cell.** Only an empty cell is missing without
  it. A cell that is exactly one of the texts given, in quotes or not, is read as an empty
  cell is read in its column: `None` where the field has `| None`, NaN in a column of
  floats, empty text in a column of `str`, and a `ValueError` in a column of whole numbers
  or `bool`s with no `None`. The value is any `str` or `list[str]`, so a program can work
  the list out.
- **The text is compared, not the number yet.** `na_values=["-999"]` leaves a cell written
  `-999.0` or ` -999` as it is, where pandas reads a column of numbers by value and takes
  both as missing. Until numbers are matched by value, give each spelling the file uses:
  `na_values=["-999", "-999.0"]`. pandas also takes a dict that names spellings for each
  column, which is a compile error here.
- **`na_rep` is written for every missing value**: a `None` of any column and a NaN of a
  column of floats, as in pandas. It is quoted as any text is, when it holds the separator,
  a quote or a line end. An empty text in a column of `str | None` is not missing and stays
  an empty cell.

Any other keyword is a compile error that names it, never ignored. The next to come, in this
order, are `nrows=` and `skiprows=`, then `header=False` and `columns=` of `to_csv` (roadmap
3.14.4.2).

## Errors

`DataFrame.read_csv` can fail in two ways, and the function that calls it handles both or
lists them in its `Raises`, as with any [error](errors.md). `to_csv` can fail with `OSError`
alone, when the file cannot be made or written.

- `OSError` when the file cannot be opened or read.
- `ValueError` when the file does not fit the schema. Its `message` names the file, or the
  column and the data row, counting the first row after the header as 1:

  | The file | `message` |
  | --- | --- |
  | has no column a field names | `trades.csv: the header has no column 'volume'` |
  | holds a cell that is not a value of its field's type | `column 'volume', row 2: could not read '1e3' as i64` |
  | holds a number outside the range of its field's type | `column 'lots', row 2: could not read '70000' as i16` |
  | has an empty cell where the field has no `None` | `column 'volume', row 2: the cell is empty, and the column's type has no None` |
  | has a row with more cells than the header | `trades.csv: expected 4 fields in row 2, saw 5` |
  | is empty | `trades.csv: no columns to read, the file is empty` |
  | is not UTF-8 | the message `decode` gives for the bytes |

## Cost

The whole file is held in memory while its columns are built, with 16 bytes more for each
cell of a column the schema names, and all of that is freed before `read_csv` returns. The
columns it gives are as large as the same columns built any other way. A column of `f32` or
of a whole number type narrower than `i64` is read as `f64` or `i64` first, 8 bytes for each
cell, and narrowed. Reading a file of one
million rows and four columns, of text, `f64`, `i64` and `bool`, took 0.78 s on the
development machine, where pandas 3.0.5 took 0.20 s; the reader has not been tuned yet.

`to_csv` builds the text of every cell before it writes the first, and frees it all before
it returns. Writing the same million rows
took 2.5 s, where pandas took 2.0 s.

## Differences from pandas

- **The types are given, never guessed.** pandas reads every column and guesses each type
  from its cells. Here the schema says which columns are read and what each holds, so a file
  that does not fit is an error when it is read, not a column of another type later.
- **Only an empty cell is missing.** pandas also reads `NA`, `NaN`, `null`, `None` and a
  dozen spellings more as missing. Here they are text in a column of `str`, `nan` and `NaN`
  are the number in a column of `f64`, and anything else is an error, unless
  [`na_values=`](#keywords) names them.
- **A column of `str` keeps an empty cell as empty text.** pandas gives it NaN. Declare the
  field `str | None` to get `None`, as pandas's missing value is read here.
- **An `i64` cell is a whole number.** pandas reads `1.0` into an `int64` column; here it is
  an error, as is a `bool` cell written `1`, `0`, `yes` or `no`.
- **A number too large for its column is an error.** Given `dtype="int16"`, pandas reads
  `70000` as 4464, the low 16 bits. Here it is a `ValueError` that names the cell.
- **The schema chooses the columns.** pandas has `usecols=` for reading some of them. Here a
  column the schema does not name is skipped, and one it names that the file lacks is an
  error.
- **Three keywords so far.** [`sep=`, `na_values=` and `na_rep=`](#keywords) are built.
  `header=`, `names=`, `skiprows=`, `nrows=` and the rest are compile errors that name the
  keyword, never ignored. The same holds for `to_csv`: `index=`, `columns=`,
  `float_format=` and the rest.
- **`to_csv` writes no index column.** pandas writes its row index as a first column unless
  `index=False` is given. A frame has no index here, so the file is the one pandas writes
  with `index=False`.
- **`to_csv` needs a path.** In pandas, `df.to_csv()` with no path gives the text back.

## Not built yet

- The keywords of `pandas.read_csv` and of `to_csv` other than `sep=`, `na_values=` and
  `na_rep=` (roadmap 3.14.4.2).
- Columns of complex numbers.
