# Missing values

`Array[f64 | None, 1]` is an array with missing values: a one-dimensional array whose elements
are numbers, any of which may be `None`. It is how Baryon holds data with gaps in it: a sensor
that did not report, a survey answer left blank, an empty cell of a CSV file
([decision 0045](../decisions/0045-dataframe.md), part 3).

This chapter covers what such an array does today: it is built from a list, counted, read one
element at a time, copied, asked where its gaps are, turned into an array with no gaps by
[`dropna` and `fillna`](#taking-the-gaps-out), [reduced](#reductions-skip-none) by methods
that skip `None`, and held as a column of a [`DataFrame`](dataframes.md): see
[In a DataFrame](#in-a-dataframe). Arithmetic and the other frame operations on such a column
come next, with roadmap item 3.13.6, and the compiler says so by name when a program asks for
one.

## `None` is not `NaN`

A missing value is `None`. `NaN`, `float("nan")`, is a number: the result of a computation
such as `0.0 / 0.0`, as IEEE 754 defines it. pandas uses `NaN` for both, so it cannot tell a
gap from a failed computation, and a column of whole numbers with one gap turns into floats.
Baryon keeps them apart, as Arrow and Polars do:

- an `Array[f64, 1]` has no gaps, and may hold `NaN`;
- an `Array[f64 | None, 1]` may have gaps, and may also hold `NaN`;
- an `Array[i64 | None, 1]` stays whole numbers, gaps or not.

## Making one

`Array(items)` with `items` a `list[T | None]` gives an array with missing values holding a
copy of each item. `T` is a number type or `bool`:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    print(len(bids))
```

This prints `3`.

- **No type needs writing.** A list written with numbers and `None` in it,
  `[1.5, None, 2.5]`, is a `list[f64 | None]` on its own ([lists](types.md#lists)), so
  `bids = Array([1.5, None, 2.5])` builds the same array as the line above. An annotation is
  still how to pick another element type: `counts: Array[i32 | None, 1] = Array([1, None])`.
- **The list is borrowed**, so it stays usable.
- **One dimension.** `Array[f64 | None, 2]` is a compile error; missing values in arrays of
  every rank are roadmap item 3.24.
- **Text is not built yet.** `Array[str | None, 1]` is a compile error for now, and so is a
  column `name: str | None`.
- **Cost:** one pass over the list and two allocations.

## Reading an element

`bids[i]` gives a `f64 | None`, the element or `None`. The language makes a program check an
`f64 | None` before it uses the number, so a gap is never read as a number by accident:

```python
def total(values: Array[f64 | None, 1]) -> f64:
    result = 0.0
    for at in range(len(values)):
        value = values[at]
        if value is not None:
            result += value
    return result


def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    print(bids[0], bids[1], bids[-1])
    print(total(bids))
```

This prints `1.5 None 2.5`, then `4.0`.

- **An index out of range stops the program**, as it does for any array.
- **`len(bids)` counts every element**, the missing ones too.
- **`bids.copy()`** gives a new array with its own memory, gaps included. As with any array,
  `kept = bids` moves the array, and a copy keeps both.

## Where the gaps are

`bids.isna()` gives an `Array[bool, 1]` that is `True` where an element is `None`, and
`bids.notna()` one that is `True` where it is not. The names and the results are pandas's:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    print(bids.isna())
    print(bids.notna())
```

This prints `[False  True False]`, then `[ True False  True]`.

- **Counting the gaps:** `bids.isna().sum()` is the number of `None` elements, an `int`, since a
  sum of `bool`s counts the `True` ones, as in NumPy and pandas.
- **Cost:** one pass and one new array of `bool`s.

## Taking the gaps out

`bids.dropna()` gives the elements that are there, in order, and `bids.fillna(value)` gives
every element, with `value` where one was `None`. Both give a new `Array[f64, 1]`, an array
with no gaps, so everything an [array](arrays.md) does is open to the result:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    there = bids.dropna()
    print(there, there.sum())
    print(bids.fillna(0.0))
    print(bids.fillna(there.mean()))
```

This prints `[1.5 2.5] 4.0`, then `[1.5 0.  2.5]`, then `[1.5 2.  2.5]`.

- **The type changes, and that is the point.** The result cannot hold `None`, so the
  compiler knows the gaps are dealt with and asks for no check on the numbers. pandas gives
  back a column that may still hold `NaN`.
- **The value has the element's type.** `bids.fillna("none")` is a compile error. A whole
  number fills an array of floats, `bids.fillna(0)`, as it does anywhere a float is expected.
- **`value=`** may name the argument, as in pandas: `bids.fillna(value=0.0)`.
- **`bids` is read, not changed.** pandas's `inplace=`, `method=` and `limit=` are not built.
- **Cost:** `dropna` makes one pass over the bitmap for a mask of a byte for each element,
  which it frees, and one pass that copies the values kept. `fillna` copies the values and
  makes one pass over the bitmap.

## Reductions skip `None`

`count()`, `sum()`, `mean()`, `min()` and `max()` reduce the elements that are there and pass
over the gaps, as pandas does:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    print(bids.count(), len(bids))
    print(bids.sum(), bids.mean(), bids.min(), bids.max())
```

This prints `2 3`, then `4.0 2.0 1.5 2.5`.

- **`count()` is how many elements are there**, an `i64`; `len` counts the gaps too. It works
  for every element type. The others take integers and floats.
- **Each gives what the method gives `bids.dropna()`**, bit for bit: a float sum adds in the
  same [pairwise order](arrays.md#the-order-sum-adds-in), `mean()` of integers is an `f64`,
  and a `NaN` that is there makes `min()` and `max()` `NaN`, since `NaN` is a number and not
  a gap.
- **With no element there**, `count()` and `sum()` give 0 and `mean()` gives `NaN`, as for an
  empty array. `min()` and `max()` stop the program, as they do for an empty array: there is
  no smallest element. pandas gives `NaN` for those two; check `count()` first where every
  element may be missing.
- **Not built yet:** `var`, `std`, `argmin`, `argmax` and the built-ins `sum(bids)`,
  `min(bids)` and `max(bids)`.
- **Cost:** `count` is one pass over the bitmap. `min` and `max` add one pass over the
  values. `sum` and `mean` copy the elements that are there into a new array, add it, and
  free it.

## How it is stored

An array with missing values is two pieces of memory, whatever its length:

- the **values**, stored exactly as an `Array[f64, 1]` stores them, with a filler where an
  element is missing;
- the **validity bitmap**: one bit for each element, eight to a byte, 1 where the element is
  there and 0 where it is `None`.

So a gap costs one bit, and a million `f64 | None` elements take 8 megabytes and 125
kilobytes, not the 16 megabytes a list of `f64 | None` takes. This is the layout of a nullable
column in the Arrow columnar format, which is why a frame can later be written to Parquet, or
handed to Arrow, without copying.

## In a DataFrame

A field of a frame's schema written `T | None` is a column that may hold `None`, stored as an
array with missing values:

```python
class Quote(Struct):
    price: f64
    bid: f64 | None


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "price": Array([1.5, 2.5, 3.5]),
        "bid": Array([1.0, None, 3.0]),
    })
    print(len(quotes), quotes.bid[1], quotes.bid[2])
    print(quotes.bid.isna())
```

This prints `3 None 3.0`, then `[False  True False]`.

- **The schema gives the list its type.** The frame's type says the column `bid` holds
  `f64 | None`, so the items of `Array([1.0, None, 3.0])` are read as that type; with
  `bid: f32 | None` the same list would hold `f32`s.
- **A column reads as an array with missing values**, `quotes.bid` or `quotes["bid"]`, and
  does what one does: `len`, an element, `isna`, `notna`, `copy`, `dropna`, `fillna` and the
  reductions, so `quotes.bid.mean()` is the mean of the bids that are there.
- **A column can be replaced**, `quotes.bid = Array([None, None, 9.0])`, by an array of the
  frame's length. Another length stops the program.
- **`len`, `shape`, `columns` and `copy()`** work as for any frame, and the frame is passed
  and returned as any frame is.
- **Columns are chosen by name**, `quotes[["bid"]]`, each copied with its gaps.
- **Rows are chosen as any frame's are**: a mask, `quotes[quotes.bid.notna()]`, a slice,
  `quotes[1:3]`, `quotes.head(n)` and `quotes.tail(n)`. Each `None` stays with its row.
- **`sort_values` puts `None` last.** `quotes.sort_values("bid")` sorts the rows that have a
  bid and puts the rows whose bid is `None` after them, in the order they had, whether the
  sort is rising or falling. A `NaN` is a number, so it comes after the other numbers and
  before the `None`s. The sort is stable, as for any frame.
- **`assign`, `drop` and `rename` change the columns** as for any frame. A column `assign`
  gives may hold `None`, `quotes.assign(ask=quotes.bid.copy())`, and a list written with
  `None` for a column that already may hold it needs no annotation,
  `quotes.assign(bid=Array([None, 2.0, None]))`. Filling one column this way narrows it:
  `quotes.assign(bid=quotes.bid.fillna(0.0))`.
- **`concat` stacks frames with one schema**, each `None` staying with its row.
- **`drop_duplicates` counts a `None` as equal to a `None`** and as different from every
  number, so two rows with no bid repeat each other and a row with a bid of `0.0` repeats
  neither. pandas compares its nullable columns the same way.
- **`value_counts` counts no row with a `None`** in a column it counts by, as it counts none
  with a `NaN`, which is what pandas does unless told otherwise. The columns of its result
  keep their types, so `quotes.value_counts("bid")` has a `bid` of `f64 | None`.
- **`describe` sums up the values that are there.** The `count` of a column that may hold
  `None` is how many values it has, and the mean, the deviation and the quartiles are
  theirs. A column with no values at all has a count of 0 and `NaN` for the rest, where
  pandas shows `<NA>`.
- **`groupby` leaves out a row whose key is `None`**, as it leaves out one whose key is
  `NaN`, and the key column of the result keeps its type.
- **`agg` reduces the values that are there.** `count` is how many a group has and `sum`
  adds them. A group may have none: its count and sum are 0, its `mean`, `var`, `std` and
  `median` are `NaN`, as for a group of `NaN`s, and its `min`, `max`, `first` and `last`
  are `None`, so those four give a column that may hold `None`. pandas shows `<NA>` for
  all of them but the count and the sum.
- **The other frame operations are not built yet** for a frame with such a column: one row,
  `quotes[0]`, `merge`, `itertuples` and `DataFrame(rows)`. Each is a
  compile error that says so. They come with roadmap item 3.13.6. Until then, take the gaps
  out first, as the next section shows: the frame `dropna` or `fillna` gives does everything
  a frame does.

### `dropna` and `fillna` of a frame

`quotes.dropna()` gives the rows with no `None` in any column. `quotes.fillna(value)` gives
every row, with the value in place of each `None`. In the result a column that held
`f64 | None` holds `f64`:

```python
class Quote(Struct):
    symbol: str
    bid: f64 | None
    size: i64 | None


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "symbol": Array(["a", "b", "c"]),
        "bid": Array([1.5, None, 2.5]),
        "size": Array([10, 20, None]),
    })
    print(quotes.dropna())
    print(quotes.fillna({"bid": 0.0, "size": 0}))
```

This prints:

```text
  symbol  bid  size
0      a  1.5    10
  symbol  bid  size
0      a  1.5    10
1      b  0.0    20
2      c  2.5     0
```

- **One value fills every column that may hold `None`**, so it must have the type of each:
  `quotes[["bid"]].fillna(0.0)`. Where the columns have different types, as `bid` and `size`
  do here, the compiler reports the column the value does not fit.
- **A dict gives each column its own value**, `quotes.fillna({"bid": 0.0, "size": 0})`. Its
  keys are column names written as string literals. A column the dict leaves out keeps its
  type and its gaps; naming a column that cannot hold `None`, or one the frame does not have,
  is a compile error.
- **The result is an ordinary frame** once no column may hold `None`: it prints, sorts,
  groups and joins. A partial fill leaves a frame that still has such a column.
- **A frame with no column that may hold `None`** is copied by both, so code written for
  either kind of frame can call them.
- **`dropna` takes no arguments yet.** pandas's `subset=` is spelled by choosing the columns
  first, `quotes[["bid", "size"]].dropna()`, which keeps only those columns; `how=`, `thresh=`
  and `axis=` are not built.
- **Cost:** `dropna` makes one mask of a byte for each row for each column that may hold
  `None`, which it frees, then one pass over each column that copies the rows kept.
  `fillna` copies every column.

## Printing

`print(bids)` writes the elements between brackets with `None` for each gap, which is how
NumPy prints an array of objects, and `print(df)` writes `<NA>` for each missing cell, as
pandas does for its nullable columns. An f-string gives the same text.

```python
from math import nan


class Quote(Struct):
    bid: f64 | None
    size: i64 | None
    last: f64


def main():
    bids = Array([1.5, None, 2.25])
    print(bids)
    quotes: DataFrame[Quote] = DataFrame({
        "bid": bids,
        "size": Array([100, None, -3]),
        "last": Array([1.0, nan, 2.5]),
    })
    print(quotes)
```

```text
[1.5 None 2.25]
    bid  size  last
0   1.5   100   1.0
1  <NA>  <NA>   NaN
2  2.25    -3   2.5
```

- **`<NA>` is a gap and `NaN` is a number.** A real `NaN` prints as `NaN` in every column,
  one that may hold `None` included. pandas shows a `NaN` in a nullable column as `<NA>`;
  Baryon keeps the two apart.
- **Each float has its own decimals** in a column that may hold `None`: up to 6, without the
  zeros at the end, so `1.5` and `2.25` above. pandas writes its nullable columns this way.
  A column with no gaps in its type shares one number of decimals,
  as [a frame's floats](dataframes.md#printing) do.
- **A long array is cut** as NumPy cuts one: of more than 1000 elements the first three and
  the last three print, with `...` between. A long frame prints its first and last 5 rows.
- **Alone only.** A tuple or a list that holds an array with missing values, or a frame
  with such a column, is a compile error in `print`. Print each one alone.
- **Cost:** one text for each element or row shown.

## What is not built

Each of these is a compile error that names what an array with missing values has today:

- operators, `bids + 1.0`, and comparisons, `bids == 1.5`;
- methods other than `isna`, `notna`, `dropna`, `fillna`, `copy`, `count`, `sum`, `mean`,
  `min` and `max`: `var`, `std`, `argsort`;
- a slice, a mask or a list of positions, `bids[1:3]`;
- assigning an element, `bids[0] = 2.0`;
- a `for` loop over the array;
- `print` of a tuple or a list that holds one, or that holds a frame with such a column;
- one as a struct field.
