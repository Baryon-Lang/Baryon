# DataFrames

A `DataFrame` is a table: named columns of one length, each column an array. It is the type for
data that comes in rows of mixed types, such as trades, catalog entries or particles, and it
keeps pandas's names for what it does ([decision 0045](../decisions/0045-dataframe.md)).

Three things differ from pandas, and each is there to catch a mistake when compiling or to make
a loop fast:

- **The columns are part of the type.** A frame's type says which columns it has and what each
  holds, so a misspelled column name is a compile error, not a `KeyError` an hour into a run.
- **A column is an array.** `trades.price` is an `Array[f64, 1]`, with every array operation.
  There is no `Series` type to learn.
- **There is no row index.** Rows are numbered from 0, and nothing else.

```python
class Trade(Struct):
    price: f64
    volume: i64


def traded_value(trades: DataFrame[Trade]) -> f64:
    """The total value traded, in the currency of `price`."""
    return (trades.price * trades.volume).sum()


def main():
    trades: DataFrame[Trade] = DataFrame({
        "price": Array([189.5, 410.0, 171.0]),
        "volume": Array([100, 250, 50]),
    })
    print(len(trades), trades.shape, trades.columns)
    print(traded_value(trades))
    print(trades.price[trades.volume > 75])
```

```text
3 (3, 2) ['price', 'volume']
130000.0
[189.5 410. ]
```

This page describes what the compiler builds today. Decision 0045 lists the whole first version;
[What builds today](status.md#dataframes) says which parts are still to come.

## The type

`DataFrame[S]` is a frame with one column for each field of the struct `S`, in the order `S`
declares them. A field `price: f64` is a column named `price` that holds `f64`s. The struct is
the *schema*: the list of column names and types.

```python
class Trade(Struct):
    price: f64
    volume: i64


def load() -> DataFrame[Trade]: ...
```

A column holds numbers of any number type, or `bool`s. Text columns and missing values are
roadmap items 3.13.3 and 3.13.6.

Two frame types are the same when their columns have the same names and types in the same
order. The struct only names them: a frame built from the columns `price: f64` and
`volume: i64` can be passed where a `DataFrame[Trade]` is expected, whether or not it was
declared with `Trade`. The compiler also works out the type of a frame no struct names, and
writes it by its columns in messages: `DataFrame[price: f64, volume: i64]`.

A frame is stored as one array per column, each contiguous, in Arrow's layout. So a loop over one
column touches only that column's memory, which is what makes `DataFrame[Particle]` a good
container for a simulation: one array per field.

A frame can be a variable, a parameter, a return value, and an item of a list, tuple or
`T | None`. It cannot be a struct field yet.

## Building a frame

`DataFrame({"name": array, ...})` builds a frame from arrays of rank 1, as pandas does. The
columns are in the order written.

```python
prices = Array([189.5, 410.0, 171.0])
volumes = Array([100, 250, 50])
trades = DataFrame({"price": prices, "volume": volumes})
```

- **The arrays move into the frame.** `prices` and `volumes` are not usable after the line
  above, as after any [move](values-and-references.md#moves). Nothing is copied, whatever the
  size. Write `prices.copy()` to keep the array too.
- **Each name is a string literal.** The names become part of the frame's type, so the compiler
  must see them.
- **The columns must have one length.** Columns of two lengths panic when the frame is built,
  as arrays of two lengths panic in arithmetic.

Where the context names the type, as in `trades: DataFrame[Trade] = DataFrame({...})` or a
`return` from a function that returns `DataFrame[Trade]`, a literal column takes its element
type from the schema: `Array([1, 2])` for the column `price: f64` holds `f64`s.

`DataFrame(rows)` builds a frame from a list of structs, one row for each, in order. The struct
is the frame's schema, so `DataFrame(rows)` with `rows: list[Trade]` is a `DataFrame[Trade]`.

```python
rows = [Trade(189.5, 100), Trade(410.0, 250), Trade(171.0, 50)]
trades = DataFrame(rows)
```

The list is read, not moved: each field is copied into its column in one pass over the list, and
`rows` is still usable afterwards. An empty list gives a frame of no rows; `DataFrame([])` needs
the context to name the type, as `empty: DataFrame[Trade] = DataFrame([])` does.

## Columns

`df.name` and `df["name"]` both read the column `name`, as in pandas. Each is an array of rank 1
inside the frame: reading it copies nothing.

```python
mean = trades.price.mean()
largest = trades["volume"].max()
value = trades.price * trades.volume
expensive = trades.price[trades.price > 200.0]
for price in trades.price:
    print(price)
```

A name the frame does not have is a compile error that lists the names it has:

```text
error: `DataFrame[Trade]` has no column `cost`
  --> main.byn:20:9
   |
20 |     a = trades.cost
   |         ^^^^^^^^^^^ not a column
   = note: its columns are `price`, `volume`
```

`df["name"]` takes a string literal, for the same reason a new frame's names are literals.

**Changing a column.** A column is changed in place like any array, and a frame passed as
`InOut` is changed for the caller:

```python
def reprice(trades: InOut[DataFrame[Trade]], factor: f64):
    trades.price *= factor
    trades["volume"][0] = 7
```

**Replacing a column.** `df["name"] = array` and `df.name = array` put a new array in place of
the column. The array moves in and the old column is freed. Its length must be the frame's number
of rows, or the assignment panics. Its element type must be the column's: a frame's type never
changes, so assignment cannot add a column. Adding and removing columns give a new frame
([`assign` and `drop`](#choosing-and-changing-columns)).

**A column cannot be moved out of its frame.** `kept = trades.price` is an error, since it would
leave the frame with a column missing. Copy the column to keep it on its own:
`kept = trades.price.copy()`.

## Rows

`df[i]` is row `i` as a value of the schema's struct: `trades[0]` is a `Trade`. Rows are numbered
from 0, and a negative literal counts from the end, so `trades[-1]` is the last row.

```python
first = trades[0]
print(first.price, first.volume)
print(trades[-1])
```

```text
189.5 100
Trade(price=171.0, volume=50)
```

- **A row is a copy.** The frame stores columns, not rows, so `df[i]` builds a new struct from
  element `i` of each column. Changing the struct does not change the frame, and a row cannot be
  assigned: write `df.price[i] = 200.0` to change one value.
- **A row number out of range panics**, as an array index does.
- **Prefer columns in a hot loop.** `df[i]` reads every column to build one struct. A loop that
  needs two columns of a wide frame is faster reading `df.price[i]` and `df.volume[i]`, and
  faster still as whole-array arithmetic on the columns.
- **The frame needs a struct to give.** A frame whose columns the compiler worked out, such as
  `DataFrame({"price": prices})`, has no struct for a row, and `df[i]` on it is a compile error.
  Declare it with one, `trades: DataFrame[Trade] = DataFrame({...})`, to read its rows.

## Choosing rows

A mask, a slice, `head` and `tail` each give a new frame with the same columns and some of the
rows:

| Expression | The rows it keeps |
| --- | --- |
| `df[mask]` | The rows where `mask`, an `Array[bool, 1]`, is `True` |
| `df[i:j]`, `df[i:j:k]` | The rows a slice chooses, as Python's slice of a list does |
| `df.head(n)` | The first `n` rows; all but the last `-n` when `n` is negative |
| `df.tail(n)` | The last `n` rows; all but the first `-n` when `n` is negative |

`n` is 5 when left out, and can be given by name, `df.head(n=3)`.

```python
costly = trades[trades.price > 180.0]
busy = trades[(trades.price > 180.0) & (trades.volume >= 200)]
print(costly)
print(busy)
print(trades[::-1].head(2))
```

```text
   price  volume
0  189.5     100
1  410.0     250
   price  volume
0  410.0     250
   price  volume
0  171.0      50
1  410.0     250
```

- **A mask is built from columns.** A comparison of a column is an array of `bool`, and `&`,
  `|` and `~` combine such arrays; each comparison needs its own parentheses, as in pandas. A
  mask of another length than the frame panics.
- **The result is a new frame.** Its columns are copies of the rows chosen, at the cost of one
  pass over each column, so changing it does not change the frame it came from. To keep only
  some rows of a frame, assign the result back: `trades = trades[trades.volume > 0]`.
- **Rows are numbered again from 0**, since a frame has no row index
  ([decision 0045](../decisions/0045-dataframe.md)). pandas would print `busy` with the number
  1, the row's number in `trades`.
- **A slice never panics for its bounds.** Bounds past either end stop at the end, as for a
  Python list, so `df[:100]` of a frame of 3 rows is all 3. A step of 0 panics.
- **Rows chosen cannot be assigned.** `df[mask] = ...` is a compile error; assign by column,
  `df.price[mask] = 0.0`.

## Looping over rows

`for row in df.itertuples():` runs once for each row, with `row` the schema's struct, as
`df[i]` gives it:

```python
for row in trades.itertuples():
    print(row.price, row.volume)

busy = [row for row in trades.itertuples() if row.volume >= 100]
```

```text
189.5 100
410.0 250
171.0 50
```

- **Each row is a new struct**, a copy of one element of each column, so the loop may keep
  it, as the comprehension above does. Changing it does not change the frame.
- **The frame is borrowed for the loop.** The body cannot change the frame it loops over; to
  change a column row by row, loop over the numbers, `for i in range(len(df)):`.
- **`itertuples` takes no arguments and belongs in a loop.** A frame has no row index to
  leave out, and its rows are already named by the struct. It works inside `enumerate` and
  `zip` and in a comprehension; `rows = df.itertuples()` on its own is a compile error.
- **Prefer columns for arithmetic.** A loop over rows reads every column for every row.
  `(df.price * df.volume).sum()` does the same work as one pass over two arrays, and is the
  form the compiler vectorizes.
- **The frame needs a struct to give**, as for `df[i]`.

## Choosing and changing columns

A frame's type lists its columns, so a frame never gains, loses or renames a column in place.
Each of these gives a new frame, whose type the compiler works out:

| Expression | The frame it gives |
| --- | --- |
| `df[["a", "b"]]` | Copies of the columns named, in the order written |
| `df.assign(name=values)` | The frame with a column added at the end, or replaced where it stands when `name` is already a column |
| `df.drop(columns=["a"])`, `df.drop(columns="a")` | The frame without the columns named |
| `df.rename(columns={"old": "new"})` | The frame with columns under new names |

```python
priced = trades.assign(notional=trades.price * trades.volume)
print(priced)
print(priced[["volume", "price"]])
value = priced.drop(columns=["volume"]).rename(columns={"notional": "value"})
print(value)
```

```text
   price  volume  notional
0  189.5     100   18950.0
1  410.0     250  102500.0
2  171.0      50    8550.0
   volume  price
0     100  189.5
1     250  410.0
2      50  171.0
   price     value
0  189.5   18950.0
1  410.0  102500.0
2  171.0    8550.0
```

- **`assign`, `drop` and `rename` take the frame.** Its columns move into the result, which
  costs nothing however many rows there are, and the old name is gone, as for any moved
  value: after the first line above, `trades` cannot be read. This is where Baryon differs
  from pandas, which copies. To keep the old frame too, take a copy: `trades.copy().assign(...)`.
  A frame borrowed from a caller cannot be taken, so a function that changes the columns of
  its parameter copies it, or takes it as `Owned[DataFrame[Trade]]`.
- **`df[["a", "b"]]` reads the frame and copies** the columns it names, at the cost of one
  pass over each, so the frame stays as it was.
- **Column names are string literals**, checked when compiling: a name the frame does not
  have is a compile error in all four, also in `rename`, where pandas ignores it. Naming one
  column twice, leaving no columns, and giving two columns one name are compile errors too.
- **A new column is an array of rank 1** with the frame's number of rows, or `assign` panics.
  The array moves in, so a column of the same or another frame is copied first:
  `df.assign(backup=df.price.copy())`. To give every row one value, build the array:
  `df.assign(fee=Array.fill(len(df), 0.25))`.
- **The arguments of `assign` are computed before the frame is taken**, so they can read
  it, as `trades.price * trades.volume` does above. Each one sees the frame as it was: a
  column added earlier in the same call cannot be read by a later argument.
- **The result has no struct for its rows** until it meets one: its type is written
  `DataFrame[price: f64, volume: i64, notional: f64]` in messages. Assign it to a variable,
  pass it or return it as `DataFrame[Priced]`, where the struct `Priced` lists the same
  columns in the same order, and `df[i]` and `itertuples` give `Priced` values.

## Sorting

`df.sort_values(by)` gives a new frame with the same columns and the rows in the order of
one column, or of several:

| Expression | The order of the rows |
| --- | --- |
| `df.sort_values("price")` | Rising `price` |
| `df.sort_values("price", ascending=False)` | Falling `price` |
| `df.sort_values(["symbol", "price"])` | Rising `symbol`; rows with one `symbol` by rising `price` |
| `df.sort_values(["symbol", "price"], ascending=[True, False])` | The same, with `price` falling |

The columns can also be given by name, `df.sort_values(by="price")`. With
`trades` holding the prices 189.5, 410.0, 171.0 and 189.5:

```python
print(trades.sort_values("price"))
print(trades.sort_values(["price", "volume"], ascending=[False, True]))
```

```text
   price  volume
0  171.0      50
1  189.5     100
2  189.5     300
3  410.0     250
   price  volume
0  410.0     250
1  189.5     100
2  189.5     300
3  171.0      50
```

- **The sort is stable.** Rows equal in every column sorted by keep the order they had,
  whichever way the sort runs. pandas promises this only when asked, with `kind="stable"`;
  here it is always so ([decision 0045](../decisions/0045-dataframe.md)).
- **NaN goes last**, rising or falling, as in pandas. In a `bool` column `False` comes
  before `True`.
- **The result is a new frame**, and the frame sorted stays as it was. To keep only the
  sorted rows, assign the result back: `trades = trades.sort_values("price")`.
- **Rows are numbered again from 0**, since a frame has no row index. pandas would print each
  row with the number it had before the sort.
- **`ascending` is a `bool`**, which need not be known when compiling, or a list written
  out with one `bool` for each column. The columns are string literals, checked when
  compiling.
- **Cost:** for `n` rows, about `n log n` comparisons for each column sorted by, two arrays
  of `n` integers while sorting, and one copy of every column.
- **A complex column has no order**, so sorting by one is a compile error. pandas's other
  arguments, such as `na_position=`, `key=` and `inplace=`, are not built.

## Grouping

`df.groupby(keys).agg(...)` gives a new frame with one row for each group: the rows of `df`
that have one value of the key column, or one combination of values of several. Each argument
of `agg` names a new column and says, as a pair, which column to read and how to make one
value of a group's values:

```python
by_symbol = trades.groupby("symbol").agg(
    total=("volume", "sum"),
    mean_price=("price", "mean"),
    trades=("price", "count"),
)
print(by_symbol)
print(by_symbol.sort_values("total", ascending=False).head(1))
```

With five trades of the symbols 7, 3, 7, 3 and 9, this prints:

```text
   symbol  total  mean_price  trades
0       3    300      407.75       2
1       7    400      190.25       2
2       9     75      171.00       1
   symbol  total  mean_price  trades
0       7    400      190.25       2
```

| Reduction | The value for a group | Its type |
| --- | --- | --- |
| `"sum"` | The sum of the values | `i64` for integers and `bool`; the column's type for floats |
| `"min"`, `"max"` | The least and the greatest value | The column's type |
| `"first"`, `"last"` | The first and the last value, in the frame's order | The column's type |
| `"count"` | How many values there are | `i64` |
| `"mean"` | The mean | `f64`; `f32` for an `f32` column |
| `"var"`, `"std"` | The sample variance and standard deviation, dividing by one less than the count | The same |
| `"median"` | The middle value, or the mean of the two middle values | The same |

- **The result's columns** are the key columns, in the order given to `groupby`, then the
  new columns, in the order given to `agg`. The keys are ordinary columns, since a frame has
  no row index; pandas makes them the index, and shows this frame after `reset_index()`.
- **The groups come out sorted by their keys**, rising, and rows keep their order inside a
  group, which is what `first` and `last` see.
- **NaN is left out, as in pandas.** A row whose key is NaN belongs to no group. A NaN
  value is skipped by every reduction, so `count` counts the values that are numbers. A
  group with no numbers has the sum 0, the count 0, and NaN for the rest; `var` and `std`
  of one value are NaN.
- **`groupby` and `agg` are written together.** The groups are not a value: `g =
  df.groupby("symbol")` is a compile error. Group by several columns with a list,
  `df.groupby(["symbol", "day"])`.
- **Every name is a string literal**, checked when compiling: the keys, the columns read
  and the reductions. A key column cannot be read by `agg`, since the result holds it
  already, and a complex column can be neither a key nor a value.
- **The result is a new frame**, with no struct for its rows until it meets one, as for
  [`assign`](#choosing-and-changing-columns).
- **Cost:** for `n` rows, a sort by the keys, about `n log n` comparisons for each key
  column, then one pass over a column for each argument of `agg`. A column that is not
  `f64` or `i64` is converted to one of them first, which copies it.
- **Sums of floats are correctly rounded**: they use `math.fsum`, which loses nothing to
  the order of the values. pandas's sums are compensated but not exact, so a last digit can
  differ; so can that of `var` and `std` of an `f32` column, which are worked out in `f64`
  here and then rounded.

## `len`, `shape` and `columns`

| Expression | Result |
| --- | --- |
| `len(df)` | The number of rows, an `i64` |
| `df.shape` | `(rows, columns)`, a `tuple[i64, i64]` |
| `df.columns` | The column names in order, a new `list[str]` |

`shape` and `columns` are the frame's own attributes. A column with one of those names is still
reached as `df["shape"]`, as in pandas.

## `copy`

`df.copy()` is a new frame with a copy of every column. It costs one pass over the frame's
memory. Assigning a frame, `other = trades`, moves it instead and copies nothing; `trades` is
then no longer usable.

## Printing

`print(df)` writes the table pandas prints for the same frame, and a frame in an f-string,
`f"{df}"`, becomes the same text:

```python
trades: DataFrame[Trade] = DataFrame({"price": prices, "volume": volumes})
print(trades)
```

```text
   price  volume
0   1.50      10
1   2.25     200
2  -3.00       3
```

The first column numbers the rows from 0. The rules are those of pandas 3 at its default
options:

- **Floats.** A column of floats shows up to 6 decimals, and as many as its longest value
  needs: 1.5 and 2.25 print as `1.50` and `2.25`. The column switches to powers of ten,
  `1.500000e+10`, when a value is under 0.000001 in size and not zero, or when one is over
  1,000,000 and the column would be over 12 characters wide. A missing number prints as `NaN`.
- **Long frames.** A frame of more than 60 rows prints its first 5 and last 5 rows, with a row
  of dots between them.
- **Wide frames.** Columns that do not fit in 80 characters are left out of the middle, with
  `...` in their place.
- **The size.** A frame cut either way ends with its size, such as `[100 rows x 2 columns]`.
- **No rows.** A frame with no rows prints `Empty DataFrame`, then its column names.

```text
        a   b
0     0.0   0
1     1.5   1
2     3.0   2
3     4.5   3
4     6.0   4
..    ...  ..
95  142.5  95
96  144.0  96
97  145.5  97
98  147.0  98
99  148.5  99

[100 rows x 2 columns]
```

Printing formats only the rows it shows, so it costs the same for ten rows or ten million.

Three limits today:

- The width is always 80 characters. pandas measures the terminal's width when it prints to
  one.
- A column of complex numbers prints each value as `print` does, `(1+2j)`, where pandas
  prints `1.0+2.0j`.
- A frame inside a list or tuple prints as its columns, one array after another, and not as
  a table. Print each frame on its own.

The formatting is written in Baryon, in the standard library's `frames.byn`
([decision 0038](../decisions/0038-standard-library-in-baryon.md)).
