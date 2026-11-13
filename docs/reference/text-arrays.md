# Arrays of text

`Array[str, 1]` is a text array: a one-dimensional array whose elements are strings. It is the
one array whose elements are not a [copy type](values-and-references.md#copy-types), and it is
what a text column of a [`DataFrame`](dataframes.md) will be
([decision 0045](../decisions/0045-dataframe.md)).

This chapter covers what a text array does today: it is built from a list, counted, read one
element at a time, looped over and copied. The rest is roadmap item 3.13.3, and the compiler
says so by name when a program asks for it.

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

## What is not built yet

Each of these is a compile error today, and the rest of roadmap item 3.13.3:

- **Assigning an element**, `names[0] = "beta"`. The elements lie end to end, so one cannot
  change length in place. Build a new array from a list.
- **Slices and masks**, `names[1:3]` and `names[keep]`.
- **Operators**: `names == "alpha"`, `"alpha" in names`, `names + other`.
- **Methods and attributes** other than `copy()`: `sum`, `shape`, `argsort` and the rest.
- **Built-in functions** other than `len`, `enumerate`, `zip` and `reversed`: `sorted`, `min`,
  `max`, `list`.
- **Showing one**: `print(names)`, `str(names)` and `f"{names}"`. Print the elements in a loop.
- **A struct field** of type `Array[str, 1]`. Hold a `list[str]` in the struct, or pass the
  array beside it.
- **A text column of a `DataFrame`**, which is the next piece of the same roadmap item.
