# CSV files

[What builds today](status.md#data-files) says which parts of this chapter the compiler builds
into programs yet.

A CSV file (comma-separated values) is a table as text: one row on each line, the cells split
by commas, and the first line naming the columns. Spreadsheets, databases and instruments all
write it. `DataFrame.read_csv(path)` reads one into a [`DataFrame`](dataframes.md), with the
name `pandas.read_csv` has ([decision 0045](../decisions/0045-dataframe.md), roadmap 3.14.4).
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
- **A field's type says how its cells are read.** The reader has `f64`, `i64`, `bool` and
  `str`, and each of them with `| None`.

  | Field | A cell holds |
  | --- | --- |
  | `f64` | a number as Python's `float` reads it, `1.5`, `2.5e3`, `inf`, `-inf` or `nan`, with spaces around it allowed |
  | `i64` | a whole number with an optional sign, `100`, `-3` or `+4`, with spaces around it allowed |
  | `bool` | `True` or `False`, in that case, all lower case or all upper case |
  | `str` | any text, kept as it is |

- **An empty cell is `None`** in a column whose field is a `T | None`, a
  [missing value](missing-values-in-frames.md). In a column of `f64` it is NaN, as pandas
  reads it. In a column of `str` it is the empty text. In a column of `i64` or `bool` it is
  an error, since those have no value for it.
  A row that ends before its last column has empty cells there.

The file is read as pandas reads one with its defaults:

- Cells are split by commas. A cell in double quotes may hold commas and line ends, and a
  double quote written twice, `""`, is one double quote. The quotes are not part of the text.
- A line ends with `"\n"`, `"\r\n"` or `"\r"`. A blank line is skipped.
- The file is UTF-8. A byte-order mark at its start is skipped.

## Errors

`DataFrame.read_csv` can fail in two ways, and the function that calls it handles both or
lists them in its `Raises`, as with any [error](errors.md).

- `OSError` when the file cannot be opened or read.
- `ValueError` when the file does not fit the schema. Its `message` names the file, or the
  column and the data row, counting the first row after the header as 1:

  | The file | `message` |
  | --- | --- |
  | has no column a field names | `trades.csv: the header has no column 'volume'` |
  | holds a cell that is not a value of its field's type | `column 'volume', row 2: could not read '1e3' as i64` |
  | has an empty cell where the field has no `None` | `column 'volume', row 2: the cell is empty, and the column's type has no None` |
  | has a row with more cells than the header | `trades.csv: expected 4 fields in row 2, saw 5` |
  | is empty | `trades.csv: no columns to read, the file is empty` |
  | is not UTF-8 | the message `decode` gives for the bytes |

## Cost

The whole file is held in memory while its columns are built, with 16 bytes more for each
cell of a column the schema names, and all of that is freed before `read_csv` returns. The
columns it gives are as large as the same columns built any other way. Reading a file of one
million rows and four columns, of text, `f64`, `i64` and `bool`, took 0.78 s on the
development machine, where pandas 3.0.5 took 0.20 s; the reader has not been tuned yet.

## Differences from pandas

- **The types are given, never guessed.** pandas reads every column and guesses each type
  from its cells. Here the schema says which columns are read and what each holds, so a file
  that does not fit is an error when it is read, not a column of another type later.
- **Only an empty cell is missing.** pandas also reads `NA`, `NaN`, `null`, `None` and a
  dozen spellings more as missing. Here they are text in a column of `str`, `nan` and `NaN`
  are the number in a column of `f64`, and anything else is an error.
- **A column of `str` keeps an empty cell as empty text.** pandas gives it NaN. Declare the
  field `str | None` to get `None`, as pandas's missing value is read here.
- **An `i64` cell is a whole number.** pandas reads `1.0` into an `int64` column; here it is
  an error, as is a `bool` cell written `1`, `0`, `yes` or `no`.
- **The schema chooses the columns.** pandas has `usecols=` for reading some of them. Here a
  column the schema does not name is skipped, and one it names that the file lacks is an
  error.
- **No keywords yet.** `sep=`, `header=`, `names=`, `skiprows=`, `nrows=`, `na_values=` and
  the rest are compile errors that name the keyword, never ignored.

## Not built yet

- Writing: `df.to_csv(path)`.
- Columns of the other number types, such as `f32` and `i32`: read `f64` or `i64` and
  convert.
- Every keyword of `pandas.read_csv`.
