# Missing values in a DataFrame

What a [`DataFrame`](dataframes.md) does with a column that may hold `None`, which is an
[array with missing values](missing-values.md)
([decision 0045](../decisions/0045-dataframe.md), part 3).

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
  `quotes[1:3]`, `quotes.head(n)` and `quotes.tail(n)`. Each `None` stays with its row. A
  mask may be a [comparison](missing-comparisons.md) with such a column,
  `quotes[quotes.bid > 2.0]`.
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
- **`merge` joins a `None` to a `None`**, as it joins a `NaN` to a `NaN` and as pandas
  joins its nullable keys, and to no number. Columns that may hold `None` come through a
  join from either frame, each `None` on its row.
- **A row holds a `T | None`.** `quotes[0]` and each row of `quotes.itertuples()` are the
  schema's struct, whose field `bid` is an `f64 | None` the program checks before it uses
  the number. `DataFrame(rows)` builds the frame from a list of that struct, each field
  that is `None` becoming a gap in its column; it costs one more pass and a list of the
  field for each such column.
- **Every frame operation takes such a column**, and
  [arithmetic](missing-values.md#arithmetic) on one gives a column that may hold `None`.
  Take the gaps out first, as the next section shows, where the result must be plain
  numbers.

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
- **`subset=` names the columns `dropna` looks at**, one name or a list, written as string
  literals: `quotes.dropna(subset=["bid"])` keeps the rows that have a bid. Only the columns
  looked at lose their `None` in the result's type; `size` above would still be
  `i64 | None`.
- **`how="all"` drops a row only when every column looked at is `None`**, where the
  default, `how="any"`, drops it for one. After `how="all"` a kept row may still have a
  `None`, so no column changes its type. Both arguments are literals, since the result's
  type depends on them. pandas's `thresh=` and `axis=` are not built.
- **Cost:** `dropna` makes one mask of a byte for each row for each column it looks at that
  may hold `None`, which it frees, then one pass over each column that copies the rows kept.
  `fillna` copies every column.
