# Missing values

`Array[f64 | None, 1]` is an array with missing values: a one-dimensional array whose elements
are numbers, any of which may be `None`. It is how Baryon holds data with gaps in it: a sensor
that did not report, a survey answer left blank, an empty cell of a CSV file
([decision 0045](../decisions/0045-dataframe.md), part 3).

This chapter covers what such an array does today: it is built from a list, counted, read one
element at a time, copied, asked where its gaps are, and held as a column of a
[`DataFrame`](dataframes.md): see [In a DataFrame](#in-a-dataframe). Arithmetic, reductions
that skip `None`, `fillna`, `dropna` and the frame operations on such a column come next, with
roadmap item 3.13.6, and the compiler says so by name when a program asks for one.

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

- **Write the type.** A list written with numbers and `None` in it, `[1.5, None, 2.5]`, needs
  the type it is for: the annotation above, a parameter's type, or a return type. Without one
  the compiler reports that the list's items do not share one type. This rule is for now. It is
  decided that such a list takes the type `list[f64 | None]` with no annotation, so
  `Array([1.5, None, 2.5])` will build the same array; that comes with a later slice of roadmap
  item 3.13.6, the one after the DataFrame operations.
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

- **Cost:** one pass and one new array of `bool`s.

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

- **The schema gives the list its type.** `Array([1.0, None, 3.0])` needs no annotation of
  its own here: the frame's type says the column `bid` holds `f64 | None`.
- **A column reads as an array with missing values**, `quotes.bid` or `quotes["bid"]`, and
  does what one does: `len`, an element, `isna`, `notna`, `copy`.
- **A column can be replaced**, `quotes.bid = Array([None, None, 9.0])`, by an array of the
  frame's length. Another length stops the program.
- **`len`, `shape`, `columns` and `copy()`** work as for any frame, and the frame is passed
  and returned as any frame is.
- **The frame operations are not built yet** for a frame with such a column: choosing rows or
  columns, `head`, `tail`, `sort_values`, `groupby`, `merge`, `concat`, `drop_duplicates`,
  `describe`, `itertuples`, `DataFrame(rows)` and `print`. Each is a compile error that says
  so. They come with roadmap item 3.13.6, with `dropna` and `fillna`.

## What is not built

Each of these is a compile error that names what an array with missing values has today:

- operators, `bids + 1.0`, and comparisons, `bids == 1.5`;
- methods other than `isna`, `notna` and `copy`: `sum`, `mean`, `fillna`, `dropna`;
- a slice, a mask or a list of positions, `bids[1:3]`;
- assigning an element, `bids[0] = 2.0`;
- a `for` loop over the array, and `print` of it;
- one as a struct field.
