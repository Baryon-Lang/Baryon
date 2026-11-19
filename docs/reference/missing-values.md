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
copy of each item. `T` is a number type, `bool` or `str`:

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
- **Text works the same way.** `Array(["Vega", None])` is an `Array[str | None, 1]`:
  see [text that may be `None`](#text-that-may-be-none).
- **Cost:** one pass over the list and two allocations.

## Reading an element

`bids[i]` gives a `f64 | None`, the element or `None`. The language makes a program check an
`f64 | None` before it uses the number, so a gap is never read as a number by accident:

```python
def total(values: Array[f64 | None, 1]) -> f64:
    result = 0.0
    for value in values:
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
- **`bids[i] = 2.0` and `bids[i] = None` change one element** of an array of numbers or
  `bool`s, and take any `f64 | None`. So does every element a mask, positions, a list or
  a slice chooses: `bids[bids > 100.0] = None`, `bids[1:3] = 0.0`. An element of text
  cannot be assigned, as in [an array of text](text-arrays.md): build the array again
  with `fillna` or from a list.
- **A `for` loop gives each element in turn**, an `f64 | None` here, alone or inside
  `enumerate`, `zip` and `reversed`, and in a comprehension:
  `[bid for bid in bids if bid is not None]`. The array is borrowed for the loop, as any
  array is. Not built yet: a loop over text that may be `None`; read `names[at]` in a loop
  over `range(len(names))`.
- **`len(bids)` counts every element**, the missing ones too.
- **`bids.copy()`** gives a new array with its own memory, gaps included. As with any array,
  `kept = bids` moves the array, and a copy keeps both.

## Choosing elements

A mask, an array of positions and a slice choose elements as they do for
[an array of numbers](arrays.md#choosing-elements), and give a new array with missing
values, each `None` staying a `None`:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0])
    print(bids[1:3])
    print(bids[Array([3, 0, 1])])
    print(bids[bids.notna()])
    print(bids[bids > 2.0])
```

This prints `[None 2.5]`, `[4.0 1.5 None]`, `[1.5 2.5 4.0]` and `[2.5 4.0]`.

- **A mask is an `Array[bool, 1]` of the array's length**, or one that may hold `None`
  itself, such as a [comparison](missing-comparisons.md): that one chooses where it is
  `True` and not where it is `None`, as pandas does.
- **Positions are an `Array[i64, 1]`**, in any order and with repeats. A position outside
  the array stops the program.
- **A slice is a copy**, not a view, as a slice of [an array of text](text-arrays.md) is:
  the bits that mark the gaps cannot be shared from the middle of a byte. Its bounds are
  clamped to the array as Python's are, and a step of 0 stops the program.
- **A `list` chooses too**, as the array of its items does: `bids[[0, 2]]`.
- **Cost:** what the same choice costs an array of numbers, and one more pass that
  chooses the bits. A slice costs an array of 8 bytes for each element it takes as well.

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

`count()`, `sum()`, `mean()`, `min()`, `max()`, `var()` and `std()` reduce the elements that
are there and pass over the gaps, as pandas does:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    print(bids.count(), len(bids))
    print(bids.sum(), bids.mean(), bids.min(), bids.max())
```

This prints `2 3`, then `4.0 2.0 1.5 2.5`.

- **`count()` is how many elements are there**, an `i64`; `len` counts the gaps too. It works
  for every element type. The others take integers and floats.
- **A mask of `bool`s has `sum()`, `any()` and `all()`**, which skip `None` too: see
  [Comparing missing values](missing-comparisons.md#the-rules).
- **Each gives what the method gives `bids.dropna()`**, bit for bit: a float sum adds in the
  same [pairwise order](arrays.md#the-order-sum-adds-in), `mean()` of integers is an `f64`,
  and a `NaN` that is there makes `min()` and `max()` `NaN`, since `NaN` is a number and not
  a gap.
- **`var()` and `std()` take floats and `ddof=`**, as they do on
  [any array](arrays.md): `ddof` is 0 when left out, which is NumPy's default. pandas's is
  1, so write `bids.std(ddof=1)` for the number pandas prints.
- **With no element there**, `count()` and `sum()` give 0 and `mean()`, `var()` and `std()`
  give `NaN`, as for an empty array. `min()` and `max()` stop the program, as they do for an
  empty array: there is no smallest element. pandas gives `NaN` for those two; check
  `count()` first where every element may be missing.
- **Not built yet:** `argmin`, `argmax` and the built-ins `sum(bids)`,
  `min(bids)` and `max(bids)`.
- **Cost:** `count` is one pass over the bitmap. `min` and `max` add one pass over the
  values. `sum`, `mean`, `var` and `std` copy the elements that are there into a new
  array, reduce it, and free it.

## Sorting positions

`bids.argsort()` gives the positions that put the elements in rising order, an
`Array[i64, 1]`, as it does for [any array](arrays.md#sorting-positions). The positions of
the `None`s come last, after any `NaN`, in the order they stand in, which is where pandas
puts them:

```python
def main():
    bids: Array[f64 | None, 1] = Array([3.0, None, 1.0, 2.0])
    order = bids.argsort()
    print(order)        # [2 3 0 1]
    print(bids[order])  # [1.0 2.0 3.0 None]
```

- **Text too.** `Array[str | None, 1]` is put in the order of `<` on `str`, and `bool`s
  with `False` first.
- **Equal elements keep their order**, so the sort is stable.
- **Cost:** about `n log n` comparisons and five arrays of `n` positions, of which the
  result is kept. Text costs one more sort.

## Distinct elements

`bids.unique()` gives the distinct elements in the order they first appear, an array of the
same type, and `bids.nunique()` how many there are, an `i64`, as they do for
[any array](arrays.md#distinct-elements):

```python
def main():
    bids: Array[f64 | None, 1] = Array([3.0, None, 1.0, 3.0, None])
    print(bids.unique())   # [3.0 None 1.0]
    print(bids.nunique())  # 2
```

- **`unique` keeps one `None`**, where the first one stands, as pandas does.
- **`nunique` does not count `None`**, nor a `NaN`, as pandas does not: it is how many
  distinct values are there.
- **Text and `bool`s too.**
- **Cost:** a sort, about `n log n` comparisons, and six arrays of `n` integers that are
  freed, one more sort for text.

## Arithmetic

`+`, `-`, `*`, `/`, `//` and `%`, and `-bids`, work element by element on numbers, and give
`None` wherever a side is `None`, as pandas's nullable columns do:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    asks = Array([2.0, 3.0, 4.0])
    print(asks - bids)
    print((bids + asks) / 2.0)
```

This prints `[0.5 None 1.5]`, then `[1.75 None 3.25]`.

- **The other side** is another array with missing values, an `Array` of numbers of one
  dimension, or a single number, on either side. The result is an `Array[T | None, 1]`
  whose `T` is the type the operator gives two plain arrays, so integers and floats mix as
  they do [there](arrays.md#whole-array-arithmetic).
- **The lengths must match.** Arrays of different lengths stop the program with both
  lengths in the message. An array with missing values does not
  [broadcast](arrays.md#broadcasting), not even one of length 1.
- **A `None` never causes an error.** What is stored under a `None` is not computed with:
  the largest `i64` beside a `None` does not overflow, and a zero under a `None`, or beside
  one, does not divide. Two integers that are both there overflow as they do in any array,
  and `//` and `%` by a zero that is there stop the program.
- **`/` of two integers gives floats**, as it does for plain arrays, and `inf` or `NaN` for
  a zero divisor that is there.
- **A `NaN` the arithmetic makes stays a `NaN`.** `2.5 % 0.0` is `NaN`, a number, and not a
  gap. pandas turns it into `<NA>`.
- **A column works the same way**, since a column that may hold `None` reads as such an
  array: `quotes.assign(mid=(quotes.bid + quotes.ask) / 2.0)`.
- **Not built yet:** `**`, the bitwise operators, and the
  in-place forms: write `bids = bids + 1.0` for `bids += 1.0`.
- **Cost:** floats are one fused loop over the stored values, like any
  [array expression](arrays.md#whole-array-arithmetic), plus one pass over the bitmaps.
  Integers copy the elements that are there out of each array first, compute, and put the
  results back, which is one more pass and one more array for each array in the expression.

## Comparisons

`bids > 2.0` gives an `Array[bool | None, 1]`, `None` wherever `bids` is, and so do
`names == "ada"` on text and `flags == True` on `bool`s that may be `None`. Such masks
combine with `&`, `|`, `^` and `~`, and choose rows of a frame, `quotes[quotes.bid > 2.0]`,
and elements of an array. See [Comparing missing values](missing-comparisons.md).

## Text that may be `None`

An `Array[str | None, 1]` holds text with gaps, and a frame's field `name: str | None` is a
column of it. An empty text and a missing one are different values:

```python
def main():
    names = Array(["Vega", None, ""])
    print(names)
    print(len(names), names.count())
    first = names[0]
    if first is not None:
        print(first.upper())
    print(names.fillna("?"))
    print(names.dropna())
```

```text
['Vega' None '']
3 2
VEGA
['Vega' '?' '']
['Vega' '']
```

- **It has what an array of numbers with gaps has:** `len`, an element read, `isna()`,
  `notna()`, `count()`, `copy()`, and `dropna()` and `fillna(text)`, which give a
  [text array](text-arrays.md). It has no `sum`, `mean`, `min` or `max` yet.
- **An element read is a copy.** `names[0]` gives a new `str | None` that owns its text, so
  it can be kept in a variable, a list or a struct after the array is gone. A
  [text array](text-arrays.md) with no gaps lends its element instead. Cost: one allocation
  for each read, of the size of the text.
- **In a frame** such a column does everything
  [a column that may hold `None`](#in-a-dataframe) does. `sort_values` puts the texts in
  the order of `<` with each `None` last, and `agg` has `count`, `first`, `last`, `min` and
  `max` for it, which give `None` for a group whose texts are all missing.
- **Stored** as a text array with an empty text under each `None`, and the validity bitmap,
  which is Arrow's layout for nullable text.

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

A column of a frame whose field is a `T | None` is an array with missing values, and
`dropna`, `fillna`, `sort_values`, `groupby`, `merge` and the rest know its gaps. That is
its own chapter: [Missing values in a DataFrame](missing-values-in-frames.md).

## Printing

`print(bids)` writes the elements between brackets with `None` for each gap, which is how
NumPy prints an array of objects, and `print(df)` writes `<NA>` for each missing cell, as
pandas does for its nullable columns. An f-string gives the same text. Text is written as a
[text array](text-arrays.md) writes it, in quotes alone and without them in a frame.

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

- `**`, the bitwise operators and the in-place operators, `bids += 1.0`;
- methods other than `isna`, `notna`, `dropna`, `fillna`, `copy`, `argsort`, `unique`,
  `nunique`, `count`, `sum`, `mean`, `min`, `max`, `var` and `std`; and `min` and `max` of
  text;
- assigning an element of text;
- a `for` loop over text that may be `None`;
- `print` of a tuple or a list that holds one, or that holds a frame with such a column;
- one as a struct field.
