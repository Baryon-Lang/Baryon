# Sorting, grouping and joining DataFrames

This page continues [DataFrames](dataframes.md), which describes the type, its columns and
its rows. The operations here read every row of a frame and build a new frame:
`sort_values` puts the rows in order, `groupby(...).agg(...)` reduces the rows that share a
key to one row, `merge` pairs the rows of two frames, `drop_duplicates` leaves out the
rows that repeat, `value_counts` counts them, and `concat` stacks frames. Each keeps
pandas's name and arguments ([decision 0045](../decisions/0045-dataframe.md)), and the
compiler works out the columns of each result.

The examples use a frame `trades` with the columns `symbol`, `price` and `volume`.

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
- **A column of `str` is sorted by code point**, the order of Python's `<` on `str`, as in
  pandas: `"Zed"` comes before `"apple"`, and the empty text comes first. The text is
  turned into whole numbers in that order first, which costs one more sort of that column
  and an array of `n` integers.
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
  group, which is what `first` and `last` see. A key of `str` is in the order of Python's
  `<`, by code point, and texts are one group only when exactly equal.
- **A column of `str` has five of the reductions**: `count`, an `i64`, and `first`,
  `last`, `min` and `max`, which give a `str`, the smallest and largest by `<`. The
  others are compile errors. pandas's `sum` of text, which joins the texts of a group, is
  not built. `min` and `max` cost one sort of the column, comparing text.
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
  [`assign`](dataframes.md#choosing-and-changing-columns).
- **Cost:** for `n` rows, a sort by the keys, about `n log n` comparisons for each key
  column, then one pass over a column for each argument of `agg`. A column that is not
  `f64` or `i64` is converted to one of them first, which copies it.
- **Sums of floats are correctly rounded**: they use `math.fsum`, which loses nothing to
  the order of the values. pandas's sums are compensated but not exact, so a last digit can
  differ; so can that of `var` and `std` of an `f32` column, which are worked out in `f64`
  here and then rounded.

## Joining

`left.merge(right, on=keys)` joins two frames: the result has one row for each pair of a row
of `left` and a row of `right` whose key columns hold the same values. This is the inner
join, so a row with no partner in the other frame is left out.

```python
class Listing(Struct):
    symbol: i64
    lot: i64

listings: DataFrame[Listing] = DataFrame({
    "symbol": Array([3, 7, 8]),
    "lot": Array([10, 100, 1]),
})
print(trades.merge(listings, on="symbol"))
```

With four trades of the symbols 7, 3, 7 and 5, this prints:

```text
   symbol  price  volume  lot
0       7  189.5     100  100
1       3  410.0     250   10
2       7  191.0     300  100
```

- **The keys have one name and one type in both frames.** Join on several columns with a
  list, `on=["symbol", "day"]`. A key of two types is a compile error; to join columns of
  two names, `rename` one first.
- **The result's columns** are those of `left`, in order, then those of `right` that are
  no keys. A column the two frames share that is no key appears twice, as `price_x` from
  the left and `price_y` from the right, as in pandas.
- **The rows follow `left`**: its rows in order, and for one row of `left` its partners in
  the order of `right`. pandas documents the same order but does not always give it when
  rows have several partners; sort both results to compare them.
- **NaN joins NaN**, as in pandas, though the two are not equal.
- **A key of `str` joins texts that are exactly equal**: `"IBM"` and `"ibm"` are no
  partners. It costs one copy of the key's text from both frames and a sort of it.
- **Both frames are read, not taken**, and the result is a new frame, with no struct for
  its rows until it meets one.
- **Only the inner join is built.** `how="inner"` can be written out; `"left"`, `"right"`
  and `"outer"` give rows with missing values, which come with roadmap item 3.13.6.
  pandas's other arguments, such as `left_on=`, `right_on=` and `suffixes=`, are not built.
- **Cost:** for `n` rows in the two frames together and `k` pairs, a sort of `n` rows for
  each key column, then `n + k` steps and one copy of each column of the result.

## Rows that repeat

`df.drop_duplicates()` gives a new frame without the rows that repeat: of the rows that
hold the same value in every column, the first is kept. `subset` names the columns to
compare, one or a list.

```python
print(trades.drop_duplicates(subset="symbol"))
```

With four trades of the symbols 7, 3, 7 and 5, this prints the first trade of each symbol:

```text
   symbol  price  volume
0       7  189.5     100
1       3  410.0     250
2       5   20.0      10
```

- **The rows kept stay in the frame's order**, and the result has the frame's type.
- **Two NaN are the same value here**, as in pandas, though the two are not equal. `0.0`
  and `-0.0` are the same value too.
- **Text is compared exactly**: `"IBM"` and `"ibm"` are two values. A column of `str` costs
  one more sort of that column, comparing text.
- **The frame is read, not taken.**
- **A complex column cannot be compared yet**: leave it out with `subset`.
- **Only the first of the rows that repeat can be kept.** pandas's `keep="last"` and
  `keep=False`, and its `inplace=` and `ignore_index=`, are not built.
- **Cost:** for `n` rows, a sort of `n` row positions for each column compared, then one
  pass and one copy of the rows kept.

## Counting values

`df.value_counts("symbol")` gives a new frame with one row for each distinct value of the
column: the value, then `count`, the number of rows that hold it. A list of columns counts
the rows by those columns together, and no argument counts by every column.

```python
print(trades.value_counts("symbol"))
```

With four trades of the symbols 7, 3, 7 and 5, this prints:

```text
   symbol  count
0       7      2
1       3      1
2       5      1
```

- **The largest count comes first.** Values with the same count stay in the order they
  first appear in the frame, as in pandas.
- **The result is a frame**, where pandas gives a `Series` with the values as its index: a
  Baryon frame has no row index. Its columns are the ones named, in the order named, and
  `count`, an `i64`. Read them as `counts.symbol` and `counts.count`.
- **A row with a NaN in a column counted by is not counted**, as in pandas. Empty text is
  a value like any other.
- **No column counted by may be named `count`**, since the result would have two columns
  of that name. This is a compile error; `rename` the column first.
- **The frame is read, not taken.** A complex column cannot be counted by yet.
- **pandas's other arguments are not built**: `sort=`, `ascending=`, `normalize=` and
  `dropna=`. Sort the result with `sort_values` for another order.
- **Cost:** for `n` rows, a sort of `n` row positions for each column counted by, one more
  sort for a column of `str`, then one pass and a sort of the distinct values by count.

## Stacking frames

`concat([first, second])` gives a new frame with the rows of `first`, then those of
`second`. `concat` is a built-in name, as `DataFrame` is, and takes any number of frames,
written out in a list.

```python
week = concat([monday, tuesday])
print(len(week) == len(monday) + len(tuesday))
```

This prints `True`.

- **The frames have one schema**: the same columns, in the same order, of the same types.
  Another schema is a compile error, where pandas would fill the columns a frame lacks
  with missing values. Bring the frames into one form first, with `df[["a", "b"]]`,
  `rename` or `assign`.
- **The result has the type of the first frame**, so its rows are that frame's struct.
- **Every frame is read, not taken**, and can be used afterwards.
- **The rows are numbered anew**, as pandas's `ignore_index=True` does: a frame has no
  row index to keep.
- **pandas's other arguments are not built**: `axis=`, `join=`, `keys=` and the rest.
- **Cost:** each frame after the first copies the rows stacked so far, so stack many
  frames with care. One frame alone is copied once.
