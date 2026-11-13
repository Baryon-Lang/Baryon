# Arrays of text

`Array[str, 1]` is a text array: a one-dimensional array whose elements are strings. It is the
one array whose elements are not a [copy type](values-and-references.md#copy-types), and it is
what a text column of a [`DataFrame`](dataframes.md) is
([decision 0045](../decisions/0045-dataframe.md)): see [In a DataFrame](#in-a-dataframe).

This chapter covers what a text array does today: it is built from a list, counted, read one
element at a time, looped over, copied, cut into parts by a slice, a mask or positions,
compared, searched, printed, sorted and reduced.
The rest is roadmap idea I.60, and the compiler says so by name when a program asks for it.

## Why not a list of strings

A `list[str]` makes one allocation for each string. A million names are a million small pieces
of memory, scattered, each freed on its own.

A text array keeps all its text in two pieces, whatever its length:

- the **bytes** of every element in UTF-8, one after another, with nothing between them;
- the **offsets**: where each element starts in the bytes, and where the last one ends.

So `Array(["ab", "", "cde"])` stores the five bytes `abcde` and the four offsets `0, 2, 2, 5`.
Element `i` is the bytes from offset `i` up to offset `i + 1`. This is the layout the Arrow
columnar format calls `large_utf8`, which is why a frame can later be written to Parquet, or
handed to Arrow, without copying its text.

## Making one

`Array(words)` with `words` a `list[str]` gives a text array holding a copy of each string:

```python
words = ["alpha", "", "gamma delta"]
names = Array(words)
print(len(names))
print(len(words))
```

This prints `3` twice: the list is read, not taken, and stays usable.

- **The type is written `Array[str, 1]`**, as a parameter, a return type or an annotation.
  `Array[str, 2]` is a compile error: a text array has one dimension.
- **An empty one needs its type**, as an empty list does: `names: Array[str, 1] = Array([])`.
- **`Array.zeros`, `Array.ones` and `Array.fill` cannot make one yet.** They are compile errors
  that point to `Array(words)`.
- **Cost:** two allocations and two passes over the list. The first pass adds up the bytes, the
  second copies them, so the array is exactly as large as its text.

## Reading elements

`names[i]` is the element at position `i`, a `str`. A negative position counts from the end, and
a position outside the array stops the program, as for any array:

```python
names = Array(["alpha", "", "gamma delta"])
print(names[0], names[-1])
print(len(names[2]), names[2].split())
print(names[0] == "alpha", names[0] < names[2])
```

This prints `alpha gamma delta`, then `11 ['gamma', 'delta']`, then `True True`.

The element is **borrowed** from the array, as an element of a `list[str]` is borrowed from its
list. Reading it copies nothing and allocates nothing: the `str` points at the array's own
bytes. It can be printed, compared, measured, sliced and passed to a function. To keep it after
the array is gone, or to put it somewhere else, copy it:

```python
first = names[0].copy()
```

`kept = names[0]` without the `copy()` is a compile error, the same one a list element gives,
since it would move the text out of the array.

## Slices, masks and positions

`names[1:3]` is a new text array of the elements a slice takes, and `names[mask]` and
`names[positions]` choose elements as they do for
[an array of numbers](arrays.md#choosing-elements):

```python
names = Array(["delta", "", "alpha", "beta"])
lengths = Array([len(name) for name in names])
for name in names[lengths > 4][::-1]:
    print(name)
```

This prints `alpha`, then `delta`.

- **A slice follows Python**: a bound left out is an end of the array, a negative bound counts
  from the end, a bound past an end stops there, and the step may be negative. A step of 0
  stops the program.
- **Each is a copy.** A slice of an array of numbers is a view of that array's memory; a slice
  of a text array owns its text, so it can be kept in a variable, and it stays whole after
  the array it came from changes. An element cannot be assigned through one.
- **A mask has the array's length**, and a position lies inside the array. Another length, or
  a position outside, stops the program.
- **Cost:** one copy of each element chosen, and for a slice 8 bytes for each as well.

## Comparisons and `in`

Comparing a text array with one `str`, or with another text array, compares element by
element and gives an `Array[bool, 1]`, a mask:

```python
names = Array(["delta", "", "alpha", "beta"])
print(names == "alpha")
print(len(names[names >= "b"]))
print("beta" in names, "gamma" not in names)
```

This prints `[False False  True False]`, then `2`, for `delta` and `beta`, then `True True`.

- **All six comparisons work**, `==`, `!=`, `<`, `<=`, `>` and `>=`, with the `str` on
  either side. Text is ordered as Python orders `str`, by code point, so `"Zed" < "alpha"`.
- **Two arrays have one length.** Another length stops the program.
- **`text in names`** is one `bool`: whether an element equals `text`. It stops at the first.
- **The mask is an ordinary array of `bool`s**, so it combines with `&`, `|` and `~` and
  chooses rows of a frame: `trades[(trades.symbol == "IBM") & (trades.price > 100.0)]`.
- **Only text compares with text.** `names == 5` is a compile error, where NumPy gives an
  array of `False`.
- **Cost:** one pass and one new array of `bool`s; no text is copied.

## Printing

`print(names)` and `f"{names}"` show a text array as NumPy shows an array of strings: each
element in quotes, a space between two, the whole in brackets.

```python
names = Array(["alpha", "", "it's"])
print(names)
```

This prints `['alpha' '' "it's"]`.

- **Each element is written as Python's `repr` writes a `str`**: in single quotes, or in
  double quotes when it holds a single quote and no double one, with `\n`, `\t` and `\\`
  for a new line, a tab and a backslash. Other control characters are written as they are,
  where `repr` would write `\x00`.
- **A line holds at most 75 characters**, and the next line starts under the first element.
- **More than 1000 elements are cut short**: the first three, `...`, and the last three.
- **`str(names)` gives the same text.** A list or tuple that holds a text array cannot be
  shown yet.
- **Cost:** one copy of each element shown.

## Loops

A `for` loop reads the elements in order, each borrowed for one pass:

```python
names = Array(["alpha", "", "gamma delta"])
longest = 0
for name in names:
    if len(name) > longest:
        longest = len(name)
print(longest)
```

This prints `11`. `enumerate(names)`, `reversed(names)` and `zip(names, other)` work as they do
for a list, and so does a comprehension: `[len(name) for name in names]`.

## Copies and moves

A text array has one owner and moves on assignment, as every array does. `names.copy()` gives a
new array with its own bytes:

```python
kept = names.copy()
moved = names
print(kept[0], moved[0])
```

After `moved = names`, reading `names` is a compile error; `kept` and `moved` are two arrays.

A text array can be a parameter, a return value, an item of a list or a tuple, a value of a
dict, and a `T | None`.

## Distinct elements

`names.unique()` gives the distinct texts, each once, in the order they first appear, as a new
text array, and `names.nunique()` gives how many there are, as they do for
[an array of numbers](arrays.md#distinct-elements) and in pandas:

```python
symbols = Array(["IBM", "AAPL", "IBM", "ibm"])
print(symbols.nunique())
for symbol in symbols.unique():
    print(symbol)
```

This prints `3`, then `IBM`, `AAPL` and `ibm`. Texts are compared exactly, so `"IBM"` and
`"ibm"` are two. Cost: a sort of the array, about `n log n` comparisons of text, and a copy of
each distinct text.

## Order, smallest and largest

`names.argsort()` gives the positions that put the elements in rising order, an
`Array[i64, 1]`, and `names.min()` and `names.max()` give the element that comes first and
last:

```python
names = Array(["delta", "alpha", "Zed"])
print(names.argsort())
print(names.min(), names.max())
```

This prints `[2 1 0]`, then `Zed delta`.

- **The order is Python's `<` on `str`**, by code point, so capitals come before small
  letters. Equal elements keep the order they stand in.
- **`min` and `max` give a new `str`**, a copy, which stays after the array is gone. An
  empty array has neither, and stops the program.
- **Sort the array itself** with `names[names.argsort()]`.
- **Cost:** `argsort` is a sort, about `n log n` comparisons of text; `min` and `max` are
  one pass.

## In a DataFrame

A field of type `str` in a frame's schema is a text column: `df.symbol` is an `Array[str, 1]`.
The frame is built from its columns, as any frame is, and the text array moves into it:

```python
class Trade(Struct):
    symbol: str
    price: f64


def main():
    symbols = Array(["IBM", "AAPL", "MSFT"])
    trades: DataFrame[Trade] = DataFrame({"symbol": symbols, "price": Array([3.0, 1.0, 2.0])})
    print(len(trades), trades.shape, trades.columns)
    cheapest = 0
    for i in range(len(trades)):
        if trades.price[i] < trades.price[cheapest]:
            cheapest = i
    print(trades.symbol[cheapest])
    print(trades)
```

This prints `3 (3, 2) ['symbol', 'price']`, then `AAPL`, then the frame as pandas prints it:

```text
  symbol  price
0    IBM    3.0
1   AAPL    1.0
2   MSFT    2.0
```

Text is printed without quotes and to the right, and a cell of 50 characters or more shows its
first 46 and `...`, as in pandas. A text column of another length
than the frame's other columns panics when the frame is built, as a column of numbers does.

Everything this page says of a text array holds for the column: `len(trades.symbol)`,
`trades.symbol[i]`, `for symbol in trades.symbol`, and `trades.symbol.copy()` to keep it apart
from the frame. `trades.symbol = Array(words)` replaces the whole column, and panics when the
new array has another length. `trades.copy()` copies the text with the numbers.

Rows are chosen from such a frame as from any other, and the text goes with them: `df.head(n)`,
`df.tail(n)`, a slice `df[1:3]`, a mask `df[df.price > 1.5]`, and `df.sort_values("price")`.
A sort by the text column itself, `df.sort_values("symbol")`, puts the rows in the order of
Python's `<` on `str`: by code point, so `"Zed"` comes before `"apple"`, as in pandas.

```python
print(trades[trades.price > 1.5].sort_values("price"))
```

```text
  symbol  price
0   MSFT    2.0
1    IBM    3.0
```

Its columns are chosen and changed as any frame's are: `df[["symbol", "price"]]`,
`df.drop(columns=["price"])`, `df.rename(columns={"symbol": "ticker"})`, and
`df.assign(symbol=Array(words))`, which also adds a text column to a frame of numbers.
`concat([first, second])` stacks such frames, text and numbers alike, and
`df.drop_duplicates()` leaves out the rows that repeat, comparing text exactly, so `"IBM"`
and `"ibm"` are two values. `left.merge(right, on="symbol")` joins on a text key the same
way, and carries the text columns of both frames into the result.
`df.groupby("symbol").agg(...)` groups rows by text, the groups in the order of the text,
and `agg` reduces a text column with `count`, `first`, `last`, `min` and `max`.

Rows work as for any frame with a schema. `DataFrame(rows)` copies each `str` field of a list
of structs into a text column. `df[i]` and the rows of `for row in df.itertuples()` are
structs whose `str` fields own a copy of their text, so a row stays whole after the frame
changes or is gone. That copy is one allocation for each `str` field of each row read; a loop
that needs one text column is cheaper as `for symbol in df.symbol`, which borrows.

```python
first = trades[0]
trades.symbol = Array(["a", "b", "c"])
print(first.symbol)
```

```text
IBM
```

## What is not built

Each of these is a compile error, and roadmap idea I.60 unless it says otherwise:

- **Assigning an element**, `names[0] = "beta"`. The elements lie end to end, so one cannot
  change length in place. Build a new array from a list.
- **Arithmetic operators**: `names + other`.
- **Methods and attributes** other than `copy`, `unique`, `nunique`, `argsort`, `min` and
  `max`: `shape`, `size` and the rest.
- **Built-in functions** other than `len`, `enumerate`, `zip` and `reversed`: `sorted`, `min`,
  `max`, `list`.
- **Showing a list or tuple that holds a text array.** `print(names)`, `str(names)` and
  `f"{names}"` work.
- **A struct field** of type `Array[str, 1]` (roadmap idea I.49). Hold a `list[str]` in the
  struct, or pass the array beside it.
