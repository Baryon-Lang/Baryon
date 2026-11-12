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
(`assign` and `drop`, roadmap 3.13.4).

**A column cannot be moved out of its frame.** `kept = trades.price` is an error, since it would
leave the frame with a column missing. Copy the column to keep it on its own:
`kept = trades.price.copy()`.

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
