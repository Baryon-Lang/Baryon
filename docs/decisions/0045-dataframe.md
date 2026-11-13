# 0045. A `DataFrame` has a schema the compiler checks, columns that are arrays, and pandas's names

Status: Accepted (Collins, 2026-11-12)
Date: 2026-11-11
Roadmap: 3.13, 3.13.1 to 3.13.10, 3.14.4, D.21

## Context

Roadmap 3.13 adds the table of named columns that data science is written in: pandas's
`DataFrame`. The design plan asks for "typed columns, `filter`, `group_by`, `join`, `sort`,
`select`, compiled to typed loops" on Arrow's memory layout, and sized the row at 5 weeks. A
study of pandas for this project (2026-11-10) measured what that hides: matching the whole of
pandas is about 55 person-years, a native design of the same reach about 25, and a first useful
version about 2. So this record settles the shape of the type and cuts the first version into
small rows, each honestly sized, and says what is left out.

The forces:

1. **Baryon is compiled and statically typed.** pandas finds a misspelled column, or a text
   column used as a number, when the line runs, perhaps an hour into a job. Baryon's promise is
   that the compiler finds it.
2. **Columns must be plain typed arrays.** The speed of the language is in loops over
   `Array[f64, 1]`. A column that is anything else needs every array operation built a second
   time.
3. **The language has no closures, no operators on user types (D.19), no reference counting and
   no run-time type information.** pandas leans on all four: `df.apply(lambda ...)`, `Series`
   arithmetic, buffers shared between frames, and columns whose type is found at run time.
4. **Collins's standing choices.** Native first, never wrapped Python. Library names stay the
   ones Python users know wherever the behavior is the same (decisions 0039, 0040, 0042, 0043).
   A large copy is never hidden (the `dict.get` choice of 1.13.14).
5. **Missing values are open (D.21)**, and a table cannot be read from a real file without them.

### What the prototype showed

The design was tried as user code before this record was written: a struct with one array per
column, generic functions that pick rows out of a column, and a `dict` that groups. It builds
and runs on today's compiler, and it found what is missing:

- A frame as "one array per column" needs no new machinery to store, free, borrow or print its
  columns. Generic functions over the element type, `def take[T: Copy](column: Array[T, 1], ...)`,
  already compile once per column type.
- **Arrays cannot be compared element by element.** `prices > 100.0` is a type error today, and
  there is no `a[mask]` and no `a[indices]`. Filtering needs all three, and they are NumPy
  features that arrays should have with or without a `DataFrame`. They are sub-row 3.13.1.
- `Array.zeros` of a type parameter does not build, so a generic function cannot make its result
  array directly. Sub-row 3.13.1 fixes that too.
- `Array[f64 | None, 1]` is rejected today ("an array's elements must be a copy type"), so no
  program depends on any layout for missing array elements. D.21 can be settled with nothing to
  migrate.
- Text is the gap. Arrays hold only copy types, so a text column today can only be a
  `list[str]`: one allocation per row.

### Three ways to type a frame

- **A. The schema is part of the type.** `DataFrame[Trade]`, where `Trade` is a struct that
  lists the columns. Column names and types are checked when compiling. Python users know the
  spelling from typed pandas (`pandera`'s `DataFrame[Schema]`).
- **B. Columns are found at run time**, as in pandas and in Polars for Rust: `df["price"]` is a
  column of unknown type, and the program asks for it as an `f64` column and panics if it is
  not. It can be written as a library today, and it reads any file without a schema. Every
  mistake the compiler could have found waits for the run.
- **C. Both**: A now, and a run-time-typed table later for exploring files whose columns are
  not known.

## Decision

Collins accepted this on 2026-11-12, choosing the schema in the type over columns found at run
time, `None` with a validity bit over `NaN` as missing and over masked arrays, and pandas's
names without a row index over Polars's names and over a row index.

### 1. The schema is part of the type (way A)

`DataFrame[S]` is a built-in type, as `Array` is. `S` is a struct whose fields are the columns,
in order:

```python
class Trade(Struct):
    symbol: str
    price: f64
    volume: i64


def main() -> Raises[None, OSError]:
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
    large = trades[trades.volume > 1000]
    by_symbol = large.groupby("symbol").agg(
        shares=("volume", "sum"),
        mean_price=("price", "mean"),
    )
    print(by_symbol.sort_values("shares", ascending=False).head(10))
```

- **A column is an array.** `trades.price`, also written `trades["price"]`, is an
  `Array[f64, 1]`, borrowed from the frame. Every array operation works on it unchanged:
  `trades.price.mean()`, `stats.median(trades.price)`, `trades.price * trades.volume`. There is
  no `Series` type.
- **A column name is checked when compiling.** A name in `df["price"]`, `sort_values("price")`,
  `groupby("symbol")` or `merge(other, on="symbol")` is a string literal or a `Final` constant.
  A name the schema does not have, or a column of the wrong type for the operation, is a compile
  error that lists the columns. A name computed at run time is a compile error.
- **The compiler works out the schema of a result.** `by_symbol` above is a frame with the
  columns `symbol: str`, `shares: i64` and `mean_price: f64`, and needs no declaration. Two frame
  types are the same when their columns have the same names and types in the same order, so that
  result can be passed to a parameter declared `DataFrame[Summary]` whose struct lists those
  three fields. An error or the editor shows an unnamed schema as
  `DataFrame[symbol: str, shares: i64, mean_price: f64]`.
- **A row is the struct.** `trades[i]` is a `Trade`, `for row in trades.itertuples()` yields
  `Trade`s, `DataFrame(rows)` builds a frame from a `list[Trade]`, and
  `DataFrame({"price": prices, "volume": volumes})` builds one from arrays, as pandas does. So
  `DataFrame[Particle]` is also the struct-of-arrays container a simulation wants: one
  contiguous array per field, which `simd` loops need. Rows of a frame whose schema has no
  struct are reached by column only; naming the schema gives them back.
- **Column types.** In the first version: every number type, `bool`, `str`, and `T | None` of
  each (part 3). `Fixed` arrays, `Copy` structs and enums as columns come after it.

### 2. Storage is Arrow's

A frame owns one buffer per column, laid out as the Arrow columnar format lays out a primitive
array: the values, contiguous. So a frame can later be handed to or taken from Arrow, Parquet
and Python without copying (3.14.6, 3.17).

- A number or `bool` column is an `Array[T, 1]`.
- A text column is an `Array[str, 1]`, a new kind of array: one buffer of UTF-8 bytes and one
  of offsets, as Arrow's `large_utf8`, with no allocation per row. Reading an element borrows
  it, as reading a `list[str]` element does. This is the one place the rule "array elements are
  copy types" is lifted.
- A frame is one chunk. It has **no row index**: rows are numbered from 0, and nothing else.

### 3. A missing value is `None`, stored as one bit (answers D.21)

One form serves arrays and tables: an element type `T | None`.

- `Array[f64 | None, 1]` stores its values as `Array[f64, 1]` does, plus Arrow's validity
  bitmap, one bit per element. A column is missing-capable when its schema field says so:
  `price: f64 | None`.
- Reading an element gives an `f64 | None`, which the language already makes the program check
  before use. Missing data cannot be read as a number by accident.
- **`NaN` is a number, not a missing value.** `float("nan")` in an `f64` column is the result of
  a computation, as IEEE defines it. pandas mixes the two; Arrow and Polars keep them apart.
- Reductions skip `None`, as pandas's `skipna=True` and NumPy's masked arrays do: `sum`, `mean`,
  `min`, `max`, `count`. Element-wise arithmetic gives `None` where either side is `None`.
- `isna`, `notna`, `fillna` and `dropna` keep pandas's names. `dropna()` and `fillna(value)`
  return a frame whose columns are no longer `T | None`: the type records that the check was
  done.
- A left or outer `merge` makes the other side's columns `T | None` in the result's schema.
- NumPy's separate masked-array type (`numpy.ma`) is not provided. Roadmap 3.24 builds this one
  form out for arrays of every rank.

### 4. pandas's names, where the behavior is the same

| Operation | Spelling |
| --- | --- |
| Build | `DataFrame({"a": x, "b": y})`, `DataFrame(rows)`, `DataFrame.read_csv(path)` |
| Size and names | `len(df)`, `df.shape`, `df.columns` |
| Column | `df.price`, `df["price"]` |
| Row and rows | `df[i]`, `df[i:j]`, `df.head(n)`, `df.tail(n)`, `df.itertuples()` |
| Choose columns | `df[["a", "b"]]`, `df.drop(columns=["c"])`, `df.rename(columns={"a": "x"})` |
| Add a column | `df.assign(notional=df.price * df.volume)` |
| Filter | `df[mask]`, with `mask` an `Array[bool, 1]` such as `(df.price > 100.0) & (df.volume > 0)` |
| Sort | `df.sort_values("price")`, `df.sort_values(["symbol", "price"], ascending=[True, False])` |
| Group | `df.groupby("symbol").agg(total=("volume", "sum"))` with `sum`, `mean`, `min`, `max`, `count`, `first`, `last`, `std`, `var` and `median` |
| Join | `left.merge(right, on="symbol", how="inner")`, `how` one of `inner`, `left`, `right`, `outer` |
| Stack | `concat([a, b])` of frames with one schema |
| Summaries | `df.describe()`, `df.drop_duplicates()`, `value_counts`, `unique`, `nunique` |
| Missing | `isna`, `notna`, `fillna`, `dropna` |
| Write | `df.to_csv(path)`, `print(df)` |

`unique()` and `nunique()` are methods of a rank-1 array, so `df.symbol.unique()` reads as it
does in pandas, and `unique` gives the values in the order they first appear, as pandas does
and `np.unique` does not. `df.value_counts("symbol")` is a method of the frame and gives a frame
with the columns `symbol` and `count`. Collins chose this on 2026-11-13 over NumPy's sorted
order and over methods of the frame only.

`df.describe()` gives a frame whose first column, `statistic`, a column of `str`, holds the
names pandas has as its row index (`count`, `mean`, `std`, `min`, `25%`, `50%`, `75%`,
`max`), followed by one `f64` column for each column of numbers under its own name. Polars
lays `describe` out the same way. Collins chose this on 2026-11-13 over one row for each
column and over leaving `describe` out until frames have row labels.

`DataFrame` and `concat` are built-in names, as `Array` is. `read_csv` takes its schema from the
context, as `Array.loadtxt` takes its rank (decision 0043), and raises when the file's header
lacks a column of the schema or a cell does not parse; its details are roadmap 3.14.4's.

Differences from pandas, each for a reason and each listed in the reference:

- **No row index.** No `loc`, `set_index`, `reset_index` or index alignment. `groupby` returns
  its keys as ordinary columns (pandas's `as_index=False`), `merge` joins on columns only, and
  `to_csv` writes no index column. `print(df)` shows row numbers, as pandas shows its default
  index, so printed output can be compared with pandas's.
- **A variable keeps its type**, so a column is not added in place: `df["x"] = values` on a
  frame without `x` is a compile error that suggests `assign`. Assigning to a column that
  exists, `df["price"] = df.price * 1.1`, works.
- **`assign`, `drop` and `rename` take the frame, they do not copy it.** The columns move into
  the result, at no cost, and the old name is gone, as with any moved value; `df.copy()` first
  keeps both. Operations that pick rows (`df[mask]`, `sort_values`, `head`, `merge`,
  `groupby`, `df[["a", "b"]]`) read the frame and build new columns, one pass each.
- **`sort_values` is always stable**, and puts `None` last. Group keys come out sorted, as
  pandas's default does.
- Arguments that pandas checks when the line runs, such as `how="sideways"`, are compile errors.

### 5. How it is built

The compiler owns the schema; Baryon owns the loops.

- `types` gains the frame type. The checker computes each result's schema. Lowering turns a
  frame into its columns and each frame operation into one call per column of a generic
  function, so the mid-level IR, the passes and the backend see only arrays and calls.
- The column loops (take rows, filter, sort order, group, join) are generic functions written
  in Baryon in the standard library (decision 0038), tested against pandas. Only the validity
  bitmap and the text array add to the runtime.
- Each operation is first one plain loop per column. Parallel loops, fused filters and a
  benchmark against pandas and Polars at 10 million rows come last (3.13.10), with evidence.

### 6. The first version, in rows

| Row | What | Weeks |
| --- | --- | --- |
| 3.13.1 | Arrays: element-wise comparisons giving `Array[bool, n]`, `&`, `\|` and `~` on them, `a[mask]`, `a[indices]`, `a.argsort()`, and `Array.zeros` of a type parameter | 2 |
| 3.13.2 | The frame type: `DataFrame[S]` of number and `bool` columns, built from arrays and from rows, `len`, `shape`, `columns`, columns read and assigned, `df[i]`, `copy()`, `print` as pandas prints | 3 |
| 3.13.3 | Text columns: `Array[str, 1]` in Arrow's layout | 3 |
| 3.13.4 | Rows and columns: `df[mask]`, `df[i:j]`, `head`, `tail`, `itertuples`, `df[["a", "b"]]`, `assign`, `drop`, `rename` | 2 |
| 3.13.5 | `sort_values` by one or more columns | 1.5 |
| 3.13.6 | Missing values in columns: `T \| None` fields, the validity bitmap, `isna`, `notna`, `fillna`, `dropna`, reductions that skip `None` | 4 |
| 3.13.7 | `groupby(...).agg(...)` by one or more key columns | 4 |
| 3.13.8 | `merge`: inner, left, right and outer, on one or more columns | 4 |
| 3.13.9 | `concat`, `drop_duplicates`, `value_counts`, `unique`, `nunique`, `describe` | 2 |
| 3.13.10 | A guide chapter with a worked analysis beside its pandas original, and a benchmark against pandas and Polars | 1.5 |

27 weeks, against the 5 the row had. Reading and writing CSV is 3.14.4, resized from 1 week to
3: quoting, numbers, text and empty cells into `T | None` columns.

### 7. Deliberately out of the first version

Each of these is real work the study counted, not an oversight. Idea I.49 keeps the list.

- Dates, times, time zones, `resample` and `rolling`: Baryon has no date type yet.
- Reshaping: `pivot`, `melt`, `stack`, multi-level columns.
- `apply` with a function per row, `query` and `eval` strings, the `.str` methods on a column.
- Categorical columns, and enums, `Fixed` arrays and structs as column types.
- A table whose columns are found at run time (way C), for opening a file without a schema.
- Functions generic over "any frame with a `price` column".
- Lazy query plans, frames larger than memory, and chunked columns (idea I.48).
- Parquet, Excel, SQL and JSON; Parquet is 3.14.6.

## Consequences

- A misspelled column, a sum of a text column and a join on columns of two types are compile
  errors. A missing value cannot be used unchecked. That is the part pandas cannot offer.
- Columns are arrays, so every array feature, present and future, is a column feature, and
  nothing is built twice. The price is that 3.13.1 and 3.13.3 are array work the `DataFrame`
  waits for.
- A schema must be written down to read a file. Exploring an unknown CSV the pandas way waits
  for way C, which the REPL (3.11) will make more pressing.
- The compiler learns a type that only it can express: the checker computes schemas, which a
  library cannot do in Baryon today. The design plan's hope that analytics is "all library" does
  not hold for the frame's type; it does hold for its loops.
- With no index, pandas code that leans on `loc` and index alignment must be rewritten with
  `merge` and masks, and with frames moved by `assign`, some lines need a `copy()` pandas does
  not. Both are flagged in the reference.
- D.21 is answered for the whole language: 3.24 and AstroBaryon's tables (4.2.6) use
  `T | None` elements, and AstroPy's masked columns map onto them.
- D.19 is not needed: masks and column arithmetic are operations on built-in arrays.
- The first version is about 30 weeks with CSV, not 5, and far short of pandas. The roadmap
  rows say so.
