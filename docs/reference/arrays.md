# Arrays

[What builds today](status.md#arrays) says which parts of this chapter the compiler builds into
programs yet.

Arrays are the language's reason to exist, so they are built in, not a library. There are two
array types, chosen by one question: is the shape known when the code is written
([decision 0007](../decisions/0007-array-types.md))?

| Type | Shape known | Stored | Use it for |
| --- | --- | --- | --- |
| `Array[T, rank]` | at run time | on the heap | grids, particle arrays, matrices of any size |
| `Fixed[T, shape]` | when compiling | inline, in registers or on the stack | positions, velocities, small tensors |

`Array[f64, 3]` is a 3-dimensional grid of any size. `Fixed[f64, (3,)]` is a 3-vector, and
`Fixed[f64, (3, 3)]` a 3 × 3 matrix. The rank (number of dimensions) of an `Array` is part of its
type; the lengths of its dimensions are not.

## Element types

An array's elements are all of one copy type: a number, `bool`, a `Fixed` array, or a struct
declared `Copy`. `Array[Vec3, 1]` with `Vec3 = Fixed[f64, (3,)]` stores N positions as 3N
consecutive `f64`s, with no pointers between them.

Elements are copy types so that reading an element is always a copy and never a borrow, which keeps
array code free of ownership questions. Arrays of non-copy values (an array of lists) are not in
version 0; use `list` for those.

The one exception is text: `Array[str, 1]` holds strings, stored end to end, and reading an
element borrows it. It has its own chapter, [Arrays of text](text-arrays.md).

An element may also be missing: `Array[f64 | None, 1]` holds numbers any of which may be
`None`, stored as the numbers and one bit for each. See [Missing values](missing-values.md).

## Layout

Arrays are row-major ([decision 0008](../decisions/0008-row-major-arrays.md)): the last index
varies fastest, as in C and NumPy. In `a[i, j, k]`, neighbours in `k` are neighbours in memory, so
the innermost loop of a loop nest should run over the last index:

```python
for i in range(nx):
    for j in range(ny):
        for k in range(nz):
            total += a[i, j, k]  # walks memory in order
```

A new array is contiguous. A view (a slice) has an offset and a stride per dimension, which is how a
view can skip elements or pick a column without copying.

## Creating arrays

| Expression | Result |
| --- | --- |
| `Array.zeros(shape)` | Every element zero |
| `Array.ones(shape)` | Every element one |
| `Array.fill(shape, value)` | Every element `value`; the element type is `value`'s type |
| `Array(values)` | A rank-1 array with the elements of the list `values` |
| `a.copy()` | A new contiguous array with `a`'s elements |
| `Array.fromfile(path, dtype)` | A rank-1 array of the elements in a binary file; see [Array files](files-and-processes.md#array-files) |
| `Array.loadtxt(fname)` | An array of the numbers in a text table, such as a CSV file; see [Text tables](data-files.md#text-tables) |
| `Array.load(file)` | An array read from NumPy's `.npy` file; see [`.npy` files](data-files.md#npy-files) |

`shape` is a tuple of integers with one entry per dimension, `(nx, ny, nz)`, or a single integer
for rank 1. The rank is the tuple's length, so the tuple must be written out where the array is
created; its entries may be any integer values.

The element type, when the expression does not fix it, is decided in this order:

1. the type the context needs, such as a parameter's or an annotated variable's type:
   `masses: Array[f32, 1] = Array.zeros(n)`;
2. otherwise the optional second argument, as in NumPy's `zeros(shape, dtype)`:
   `Array.zeros((n,), Vec3)` or `Array.zeros((n,), dtype=Vec3)`;
3. otherwise `f64`, as NumPy's default is `float64`.

In a generic function the element type may be the function's type parameter, when the parameter
is bounded by `Number` or `Float`. The array is then built at the type of each call:

```python
def twice[T: Number](values: Array[T, 1]) -> Array[T, 1]:
    result = Array.zeros(len(values), T)
    for at in range(len(values)):
        result[at] = values[at] + values[at]
    return result
```

`twice(Array([1, 2, 3]))` gives `[2 4 6]`, an array of `int`, and `twice(Array([1.5, 2.0]))`
gives `[3. 4.]`. A parameter with no bound has no zero, so `Array.zeros(n, T)` is an error there.

A `Fixed` array is created by calling its type with its elements, or with one of the same
functions without the shape:

```python
Vec3 = Fixed[f64, (3,)]
Mat3 = Fixed[f64, (3, 3)]

origin = Vec3.zeros()
up = Vec3(0.0, 0.0, 1.0)
identity = Mat3((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))
```

A rank-1 `Fixed` array unpacks like a tuple of its elements: `x, y, z = up`.

## Shape

`a.shape` is a tuple of the dimension lengths, `len(a)` is the length of the first dimension, and
`a.size` is the total number of elements, as in NumPy. For a `Fixed` array, all three are
compile-time values.

## Indexing

`a[i, j, k]` takes one integer index per dimension and gives the element, by copy. Assigning to it,
`a[i, j, k] = 1.0`, changes the element, which requires that `a` can be changed: a local variable or
an `InOut` parameter.

Indices count from 0. A negative literal counts from the end, as in Python: `a[-1]` is the last
element, and `a[:-1]` all but the last. An index computed at run time must be in range, from 0 to
the dimension's length minus 1; a negative one panics. This differs from Python on purpose: in a
stencil, `u[i - 1]` with `i` at 0 is almost always a bug, and Python's rule would silently read the
far edge of the grid instead of stopping.

### Bounds checks

Every index is checked against its dimension in every build mode, and an index out of range panics
with the index, the dimension and its length
([decision 0010](../decisions/0010-bounds-checks.md)). Inside `for i in range(start, stop)`, the
compiler makes a check once, before the loop, when the index is `i`, a value the loop does not
change, or a constant, plus or minus a constant, and the array's shape does not change in the loop:
`a[i]`, `u[i - 1]` and `b[row, i + 1]` all qualify. If those checks pass for the first and last
pass, the loop runs without them; if one fails, the loop runs with every check and panics at the
same element, with the same message, as it would otherwise. So most loops pay nothing.
Indices into `Fixed` arrays are checked when compiling wherever the index is a compile-time value.

### Unchecked access

Inside `with unsafe:`, `a.unchecked[i, j]` reads or writes an element with no bounds check. An index
out of range there is undefined behavior: the program may crash or silently corrupt memory. Use it
only where a benchmark shows the check costs time the compiler could not remove, and say in a
comment why the index is in range.

```python
with unsafe:
    total += a.unchecked[indices[k]]  # every entry of indices is < len(a), checked at load time
```

## Slicing

`a[start:stop:step]` on a dimension selects a range of it, as in Python; `a[:, 0]` takes column 0 of
a matrix. A slice is a view: it refers to `a`'s memory and copies nothing. How views may be used is
in [Values and references](values-and-references.md#slices).

`start` and `stop` must lie within the dimension, and a slice out of range panics rather than being
shortened. Python shortens it silently; here, as with indices, a range that does not fit the array
is treated as the bug it usually is. Negative literals count from the end, as with indices.

`None` among the indices adds a dimension of length 1 to the view, for
[broadcasting](#broadcasting): `x[:, None]` is a vector seen as a column.

## Whole-array arithmetic

Arithmetic operators work element by element on whole arrays and views:

```python
x += alpha * direction
residual -= alpha * product
direction = residual + beta * direction
```

- The operands are two arrays whose shapes [broadcast](#broadcasting), as NumPy's do, or an
  array and a single number, which applies to every element. Shapes of `Array` operands are
  checked when the line runs, and shapes that do not broadcast panic; shapes of `Fixed` operands
  are checked when compiling. An `Array` and a `Fixed` array do not combine.
- The number mixing rules of [Types](types.md#number-mixing) apply to the element types.
- `-a` negates each element and `+a` keeps it. Both need an array of numbers.
- An expression of element-wise operations runs as a single loop with no temporary arrays.
  `x += alpha * direction` reads each element of `direction` once and writes each element of `x`
  once.
- An intermediate result stored in a name and read once, later in the same function, is not built
  either, when its elements are floats or complex numbers. The compiler moves its expression into
  the one that reads it, so these three lines run as one loop, as `c = (a * b + d) * 0.5` does:

  ```python
  t = a * b
  u = t + d
  c = u * 0.5
  ```

  Results are identical to the bit, and a shape mismatch still panics at the line it is on.
  Reading `t` a second time, changing `a`, `b` or `d` between the lines, a call between them, or
  integer elements keep each line its own loop.
- Assigning an expression to a name, `direction = residual + beta * direction`, means a new array
  for the result. When the expression reads the array the name holds, as this one reads
  `direction`, only whole and not through a slice, and the name owns its array (a variable, or an
  `Owned` or `InOut` parameter), the compiler writes the result into that array instead, so the
  update allocates nothing. `x += alpha * direction` is written in place the same way. The
  meaning does not change: each element is read before it is written.

`@` is the matrix product for rank-2 arrays and `Fixed` matrices, and between a matrix and a
vector; see [The matrix product](#the-matrix-product).

### Broadcasting

Two arrays of different shapes combine element by element when their shapes *broadcast*: when
one can be stretched to the other's shape by repeating it. The rules are NumPy's, so code ported
from NumPy gives the same results:

1. The shapes line up from the last dimension. An array of lower rank counts as having extra
   leading dimensions of length 1: a vector of length 4 against a 3 × 4 matrix is a 1 × 4 row.
2. In each dimension the lengths must be equal, or one of them must be 1.
3. The result has the higher rank, and in each dimension the length that is not 1. An array of
   length 1 in a dimension repeats its one element along it.

| `a` | `b` | `a + b` |
| --- | --- | --- |
| `(3, 4)` | `(4,)` | `(3, 4)`: `b` added to every row |
| `(3, 4)` | `(3, 1)` | `(3, 4)`: `b[i, 0]` added to every element of row `i` |
| `(3, 1)` | `(4,)` | `(3, 4)`: every sum `a[i, 0] + b[j]` |
| `(4,)` | `(1,)` | `(4,)`: the one element of `b` added to each |
| `(2, 3)` | `(4,)` | a panic: 3 and 4 differ and neither is 1 |

```python
def centered(samples: Array[f64, 2]) -> Array[f64, 2]:
    n, m = samples.shape
    means = Array.zeros(m)
    for i in range(n):
        means += samples[i, :]
    means /= n
    return samples - means  # each row minus the column means
```

`None` in a subscript adds a dimension of length 1, as NumPy's `None` (`numpy.newaxis`) does,
so a vector can stand as a column or a row. It takes no dimension of the array, so every
dimension still needs its index or slice: `x[:, None]` and `x[None, :]` of a vector `x` are
views of shape `(n, 1)` and `(1, n)`. Combining the two gives every pair, with no loop written:

```python
def separations(x: Array[f64, 1]) -> Array[f64, 2]:
    return x[:, None] - x[None, :]  # entry [i, j] is x[i] - x[j]
```

Like any slice, `x[:, None]` is a view of `x`'s memory, not a copy; NumPy's `x[None]`, which
leaves the last dimension out, is written `x[None, :]` here.

The rank of the result is known when compiling, so `Array[f64, 2] + Array[f64, 1]` is an
`Array[f64, 2]`. Whether an `Array` repeats along a dimension is known only when the line runs,
from its length there; a pair of lengths that are neither equal nor 1 panics, naming the
result's dimension, the result's length there and the length that does not fit:

```text
panic: array lengths differ in dimension 1: 3 and 4
```

`Fixed` shapes broadcast when compiling: `Fixed[f64, (3, 1)]` plus `Fixed[f64, (4,)]` is a
`Fixed[f64, (3, 4)]`, and shapes that do not broadcast are a compile error.

Writing into an existing array never changes its shape, as in NumPy. So `x += b`, `x -= b` and
the other in-place operators, and assigning into a slice, `grid[:, :] = row`, take operands that
broadcast to the target's shape: `grid += row` adds `row` to every row of `grid`, but `x += b`
with `x` of length 1 and `b` of length 3 panics, since `x` cannot grow. `x = x + b` gives a new
array of length 3 instead.

Cost: an operand that repeats is read again from memory, never copied, so `grid - means` reads
`means` once per row and allocates only the result. For an `Array` the program decides once per
line, before the loop, whether anything repeats; when nothing does, the loop is the same one
arrays of equal shapes run, so broadcasting costs nothing where it is not used.

### Comparisons

A comparison with an array on either side compares element by element and gives an array of
`bool`s, as in NumPy:

```python
def main():
    prices = Array([189.5, 410.0, 190.25, 171.0])
    volume = Array([100, 250, 300, 50])
    print(volume > 75)                          # [ True  True  True False]
    print((prices > 180.0) & (volume >= 250))   # [False  True  True False]
    print(~(prices == 410.0))                   # [ True False  True  True]
```

- `==`, `!=`, `<`, `<=`, `>` and `>=` compare two arrays whose shapes
  [broadcast](#broadcasting), or an array and a single value. The result is an
  `Array[bool, n]`, or a `Fixed` array of `bool`s, of the broadcast shape. Elements compare as
  two single values do, so the [number mixing](types.md#number-mixing) rules apply:
  `counts > 1.5` compares an integer array as floats.
- `&`, `|` and `^` combine two arrays of `bool`s element by element: and, or, and exclusive or.
  `~mask` is `not` of each element. All four also apply to integer arrays, bit by bit.
- Python's `and`, `or` and `not` take one `bool`, not an array, so write `&`, `|` and `~`.
  They bind tighter than comparisons, as in Python: the parentheses in
  `(prices > 180.0) & (volume >= 250)` are needed.
- A chain, `0.0 < a < 1.0`, is an error on arrays, since it means `and`. Write
  `(0.0 < a) & (a < 1.0)`.
- An array of `bool`s is not a condition: `if a > 0.0:` is an error, as it is ambiguous in
  NumPy.

A comparison is one loop that writes one byte per element. A comparison of two expressions,
`a * 2.0 <= b + 1.0`, computes both sides inside that loop and stores neither.

### Choosing elements

An array in a subscript chooses elements of a rank-1 `Array`, as in NumPy:

```python
def main():
    prices = Array([189.5, 410.0, 190.25, 171.0])
    volume = Array([100, 250, 300, 50])
    print(prices[volume > 75])     # [189.5  410.   190.25]
    order = Array([3, 0, 0, 2])
    print(prices[order])           # [171.   189.5  189.5  190.25]
    prices[prices > 400.0] = 400.0
    print(prices)                  # [189.5  400.   190.25 171.  ]
```

- `a[mask]`, with `mask` an `Array[bool, 1]` of the same length, is the elements of `a` where
  `mask` is `True`, in order. A mask of another length panics.
- `a[positions]`, with `positions` an `Array[i64, 1]`, is the elements at those positions, in
  that order; a position may repeat. Each position is bounds-checked as an index is, so one
  below 0 or past the end panics. NumPy counts a negative position from the end; here only a
  negative literal index does, as in [Indexing](#indexing).
- Either is a new array that owns its elements, not a view, so it can be kept in a name,
  returned, or chosen from again: `volume[order][volume[order] > 60]`.
- `a[mask] = value` and `a[positions] = value` set every chosen element to one value, which is
  computed first.

Cost: `a[positions]` is one loop and one allocation. `a[mask]` reads the mask twice, once to
count and once to copy, so the result is allocated once at its final size.

## Methods

Version 0 defines these. The standard library (roadmap 1.14) adds the rest of NumPy's familiar
set.

| Method | Result |
| --- | --- |
| `a.copy()` | A new contiguous array with the same elements |
| `a.sum()`, `a.min()`, `a.max()` | The sum, smallest and largest element. The sum of `bool` elements counts the `True` ones and is an `int`, as in NumPy: `(a > 0.0).sum()` is how many elements are positive |
| `a.sum(axis=k)`, `a.min(axis=k)`, `a.max(axis=k)` | The sums, smallest and largest elements along axis `k`: an array with that axis removed. See [Reductions along an axis](#reductions-along-an-axis) |
| `a.mean()`, `a.mean(axis=k)` | The mean of the elements, or of each line along axis `k`, with NumPy's bits. See [Means and variances](#means-and-variances) |
| `a.var(ddof=0)`, `a.std(ddof=0)`, each with `axis=k` | The variance and standard deviation of float elements, dividing by `n - ddof`, with NumPy's bits |
| `a.dot(b)` | The dot product of two rank-1 arrays or vectors of the same length |
| `a.argsort()` | The positions that put a rank-1 `Array` in rising order, an `Array[i64, 1]`. See [Sorting positions](#sorting-positions) |
| `a.unique()`, `a.nunique()` | The distinct elements of a rank-1 `Array` in the order they first appear, and how many there are. See [Distinct elements](#distinct-elements) |
| `a.round()` | A new array with each element rounded to the nearest whole number, ties to even |
| `a.fill(value)` | Sets every element to `value`; `a` must be changeable |
| `a.tofile(path)` | Writes every element to a binary file, as NumPy's `tofile` does; an `Array` only. See [Array files](files-and-processes.md#array-files) |

`a.min()` and `a.max()` of an empty array panic, since there is no smallest element of none;
NumPy raises `ValueError`. If any element is a float NaN, the result is NaN, as `numpy.min`
gives. Python's `min(x, y)` of two numbers keeps its own rule instead: it returns `y` only when
`y < x`.

`a.dot(b)` of two `Array`s of different lengths panics, as whole-array arithmetic does:

```text
panic: array lengths differ in dimension 0: 3 and 4
```

`a.fill(value)` changes `a`, so `a` must be a local variable or an `InOut` parameter, as for
assigning to an element. `value` converts to the element type as an assignment would.

```python
from math import sqrt


def residual_norm(residual: Array[f64, 1]) -> f64:
    return sqrt(residual.dot(residual))


def reset(forces: InOut[Array[f64, 1]]):
    forces.fill(0.0)
```

### Sorting positions

`a.argsort()` gives the positions of the elements of a rank-1 `Array` in rising order, so
`a[a.argsort()]` is `a` sorted and `a.argsort()[0]` is where its smallest element stands.

```python
prices = Array([189.5, 410.0, 171.0])
order = prices.argsort()
print(order, prices[order])
```

This prints `[2 0 1] [171.  189.5 410. ]`.

- **Equal elements keep the order they stand in**: the sort is stable, as NumPy's is with
  `kind="stable"`. NumPy's default sort may order equal elements another way.
- **NaN goes last**, as in NumPy. `False` comes before `True`.
- **Complex elements have no order** and are a compile error; NumPy sorts them by their
  real part.
- `a` is read, not changed. Arrays of higher rank and NumPy's `axis=` and `kind=` are not
  built.
- **Cost:** about `n log n` comparisons, and two arrays of `n` positions while it runs.

### Distinct elements

`a.unique()` gives the distinct elements of a rank-1 `Array`, each once, in the order they
first appear, as a new array of the same element type. `a.nunique()` gives how many there
are, an `i64`. Both follow pandas, so they work the same on a column of a
[`DataFrame`](dataframes.md), `df.day.unique()`
([decision 0045](../decisions/0045-dataframe.md)), and on
[an array of text](text-arrays.md#distinct-elements).

```python
days = Array([3, 1, 3, 2, 1])
print(days.unique(), days.nunique())
```

This prints `[3 1 2] 3`.

- **The order is the order of first appearance**, as in pandas. NumPy's `np.unique` sorts
  instead; for that, sort the result: `u = a.unique()`, then `u[u.argsort()]`.
- **NaN is one element of `unique`, and `nunique` does not count it**, as in pandas. `0.0`
  and `-0.0` are one element, the one that comes first.
- **Complex elements are a compile error** for now, since the elements are compared by
  sorting them.
- `a` is read, not changed. Arrays of higher rank and pandas's `dropna=` are not built.
- **Cost:** about `n log n` comparisons, and four arrays of `n` integers while it runs.

### The order `sum` adds in

Floating-point addition is not associative, so the order a sum adds in changes its last bits and
how much rounding error it collects. `a.sum()` of floats fixes the order: NumPy's pairwise tree,
so the result has the same bits as `numpy.sum` of the same array. Rounding error grows with the
logarithm of the length rather than the length, at the same cost as adding one element at a time.

The elements are taken in row-major order (decision 0008) as one run of `n` values, and summed by
this tree, written in Python:

```python
def pairwise(v):
    n = len(v)
    if n < 8:
        total = -0.0
        for x in v:
            total += x
        return total
    if n <= 128:
        t = list(v[:8])
        whole = n - n % 8
        for i in range(8, whole, 8):
            for lane in range(8):
                t[lane] += v[i + lane]
        total = ((t[0] + t[1]) + (t[2] + t[3])) + ((t[4] + t[5]) + (t[6] + t[7]))
        for x in v[whole:]:
            total += x
        return total
    half = n // 2
    split = half - half % 8
    return pairwise(v[:split]) + pairwise(v[split:])

a_sum = 0.0 + pairwise(elements)
```

The `0.0` added last is the identity of `+`, so an empty array sums to `0.0`. An `f32` array adds
in `f32` throughout, as NumPy does. Integer sums add one element at a time: integer addition is
exact, so the order cannot change the result.

A sum of a whole-array expression, such as `(a * b).sum()` or `((u - v) * (u - v)).sum()`,
adds in the same tree, so it has the same bits as storing the expression and summing it, and as
`numpy.sum`. It never stores the expression: the compiler computes the elements one leaf of the
tree at a time, at most 128 of them, into a small buffer, sums each leaf, and adds the leaf sums
in the tree's order ([decision 0041](../decisions/0041-one-pass-reductions.md)). That saves an
array as large as the expression and a pass over it. It applies to `Array`s of floats; a sum
along an axis of an expression still stores the expression first.

`a.dot(b)` of floats adds its products in the same tree: each product `a[i] * b[i]` is rounded,
never fused with the add that follows, and the products are summed as `sum` sums its elements. So
`a.dot(b)` has the same bits as `(a * b).sum()` and as `numpy.sum(a * b)`, without building
`a * b`. `numpy.dot` itself calls BLAS, whose order depends on the machine, so its last bits
can differ. For fewer than 8 elements, such as a 3-vector, the tree adds in plain order. The same reasoning fixes the order of reductions in
parallel loops; see [reproducible reductions](parallel-loops.md#reproducible-reductions).

### Reductions along an axis

`a.sum(axis=k)`, `a.min(axis=k)` and `a.max(axis=k)` reduce along one axis, as NumPy's do. The
result has `a`'s lengths without axis `k`: each of its elements reduces the line of elements of
`a` along `k` that passes through it. For a grid of shape `(rows, columns)`, `axis=0` gives one
value per column and `axis=1` one per row. A negative axis counts from the last, so `axis=-1` is
the last axis; the axis can also be written without its name, `a.sum(1)`.

| `a` | `a.sum(axis=0)` | `a.sum(axis=1)` |
| --- | --- | --- |
| `Array[f64, 2]` of shape `(3, 4)` | `Array[f64, 1]` of length 4 | `Array[f64, 1]` of length 3 |
| `Array[f64, 3]` of shape `(2, 3, 4)` | `Array[f64, 2]` of shape `(3, 4)` | `Array[f64, 2]` of shape `(2, 4)` |
| `Fixed[f64, (2, 3)]` | `Fixed[f64, (3,)]` | `Fixed[f64, (2,)]` |
| `Array[f64, 1]` of length 5 | one `f64`, the same as `a.sum()` | an error |

The axis is a constant whole number: a literal, a negated literal or a `Final` constant, since the
result's rank depends on it. An axis the array does not have is an error when compiling:

```text
error: axis 2 is out of range for `Array[f64, 2]`, which has 2 axes
```

A float sum along an axis has the same bits as `numpy.sum(a, axis=k)`. NumPy adds in two orders,
and Baryon follows both. Along the last axis each line is one row in memory, and each row is
summed by the pairwise tree of [`a.sum()`](#the-order-sum-adds-in). Along any other axis, each
result starts at `0.0` and the elements are added one at a time from the first, which reads the
array in row-major order. Integer sums are exact in any order, and overflow is checked as `+`
checks it.

`a.min(axis=k)` and `a.max(axis=k)` follow the rules of `a.min()`: a NaN anywhere in a line makes
that result NaN, and an axis of length 0 panics, even when the other axes are long, as NumPy
raises. An empty result, from a length of 0 in another axis, is fine.

```python
def column_means(samples: Array[f64, 2]) -> Array[f64, 1]:
    """The mean of each column of a table of samples, one row per sample."""
    return samples.sum(axis=0) / samples.shape[0]


def peak_per_channel(signal: Array[f64, 2]) -> Array[f64, 1]:
    """The largest value of each row, one row per channel."""
    return signal.max(axis=-1)
```

Each reduction reads the array once and allocates only its result. Version 0 reduces along one
axis at a time; NumPy's tuple of axes and `keepdims=` are not built.

### Means and variances

`a.mean()`, `a.var()` and `a.std()` are NumPy's `mean`, `var` and `std` of the whole array, and
give the same bits as NumPy, because they round in the same places NumPy does:

- `a.mean()` is `a.sum()` divided by `n`, the number of elements. The mean of floats keeps their
  type, so the mean of an `Array[f32, 1]` is an `f32`, and the mean of integers is an `f64`.
- `a.var()` is the mean squared distance from the mean: the [pairwise sum](#the-order-sum-adds-in)
  of `(a - m) * (a - m)`, where `m` is `a.mean()`, divided by `n - ddof`. The keyword `ddof`,
  "delta degrees of freedom", is an `int` and defaults to 0, as in NumPy, which gives the
  population variance; `ddof=1` gives the sample variance, the one Python's
  `statistics.variance` computes.
- `a.std()` is the square root of `a.var()`, with the same `ddof`.

`var` and `std` take arrays of floats; `mean` takes integers too. The mean of an integer array
sums the integers exactly, as `a.sum()` does, and converts the total to `f64` once, which has
NumPy's bits while the total stays below 2⁵³. Complex arrays are not built.

An empty array has no mean: `a.mean()` is NaN, 0 / 0, as NumPy's is, without NumPy's warning.
When `n - ddof` is 0 or less, the variance divides by 0 and is infinity or NaN, again as NumPy's
is.

```python
from math import sqrt


def standardize(x: Array[f64, 1]) -> Array[f64, 1]:
    """`x` shifted to mean 0 and scaled to standard deviation 1: z-scores."""
    return (x - x.mean()) / x.std()


def standard_error(samples: Array[f64, 1]) -> f64:
    """The standard error of the mean of independent samples."""
    return samples.std(ddof=1) / sqrt(f64(len(samples)))
```

`a.mean()` reads the array once. `a.var()` and `a.std()` read it twice, once for the mean and
once for the squares, which are summed as they are computed, one leaf of the pairwise tree at a
time ([decision 0041](../decisions/0041-one-pass-reductions.md)), so no second array is
allocated. NumPy's two-pass formula is used rather than a one-pass running update because it is
NumPy's: a one-pass formula would round in other places and give other bits.

Each also takes an axis, as [`a.sum(axis=k)`](#reductions-along-an-axis) does, by position or as
`axis=`, alongside `ddof`. The result drops that axis, and each element is the statistic of one
line along it: `a.mean(axis=k)` is `a.sum(axis=k)` divided by the length along `k`, with the
bits of `numpy.mean(a, axis=k)`, and `a.var(axis=k, ddof=1)` subtracts each line's mean from the
line, squares, sums along `k` in `a.sum(axis=k)`'s order and divides by `n - ddof`, with the bits
of `numpy.var`. Along an axis they take floats only.

```python
def standardize_columns(table: Array[f64, 2]) -> Array[f64, 2]:
    """Each column of a table of samples, one row per sample, shifted to mean 0 and scaled to
    sample standard deviation 1."""
    return (table - table.mean(axis=0)) / table.std(axis=0, ddof=1)
```

Along an axis, `a.var()` stores the squared differences in an array the size of `a` before
summing them, since one-pass sums along an axis are not built (roadmap idea I.46): one extra
array, written and read once.

### The matrix product

`a @ b` multiplies two `Array`s or two `Fixed` arrays, as NumPy's `@` does:

| `a` | `b` | `a @ b` |
| --- | --- | --- |
| matrix, shape `(n, k)` | matrix, shape `(k, m)` | matrix, shape `(n, m)` |
| matrix, shape `(n, k)` | vector, length `k` | vector, length `n` |
| vector, length `k` | matrix, shape `(k, m)` | vector, length `m` |

For the dot product of two vectors, use `a.dot(b)`. Entry `[i, j]` of the result is the sum over
`p` of `a[i, p] * b[p, j]`, and the number mixing rules of [Types](types.md#number-mixing) give
its element type. The result is a new array, so `x = m @ x` reads the old `x` throughout.

The lengths that meet, `k`, must match. For `Fixed` arrays the compiler checks that; for `Array`s
a mismatch panics when the line runs and names `a`'s last dimension:

```text
panic: array lengths differ in dimension 1: 3 and 2
```

```python
def apply(m: Array[f64, 2], x: Array[f64, 1], b: Array[f64, 1]) -> Array[f64, 1]:
    return b - m @ x
```

Each entry adds its products one at a time, from `p = 0`. This order is not part of the language:
with the build option `--blas` (roadmap 3.10.2), `@` will call BLAS, which adds in an order that
depends on the machine, as NumPy's `@` does. Where the last bits must be reproducible, use `dot`
and `sum`, whose order is fixed.

## Printing

`print` writes an `Array` or a `Fixed` array exactly as NumPy's `print` writes an `ndarray` with
its default options, so a script ported from NumPy prints the same lines
([decision 0033](../decisions/0033-printing-arrays.md)):

```python
def main():
    a = Array([1.0, 2.5, 0.1 + 0.2])
    print(a)
    grid = Array.zeros((2, 3))
    grid[1, 2] = -7.0
    print(grid)
    print(Array.fill(2000, 1.5))
```

prints

```text
[1.  2.5 0.3]
[[ 0.  0.  0.]
 [ 0.  0. -7.]]
[1.5 1.5 1.5 ... 1.5 1.5 1.5]
```

The rules are NumPy's:

- **One format per array.** All elements share one width, so columns line up. Floats show
  their fewest digits, at most 8 after the point, so `0.1 + 0.2` prints `0.3`; a whole number
  keeps its point, `1.`. Floats switch to scientific notation, `1.e-05`, when the largest
  magnitude reaches 10⁸ (10⁶ for `f32`), the smallest is below 10⁻⁴, or the largest is more
  than 1000 times the smallest. Integers print plain, `[1 2 3]`; `bool` as `[ True False]`; a
  complex number as `1.+2.j`.
- **Rows on their own lines.** Each row of a matrix starts a line, and a rank-3 array leaves a
  blank line between its blocks. A row too long for 75 characters wraps.
- **Huge arrays are summarized.** An array of more than 1000 elements shows only the first and
  last 3 positions along each dimension, with `...` between, however large it is.
- **An empty array** prints `[]`, whatever its shape.

Inside a tuple or a struct an array prints the same way, `Grid(cells=[1. 2. 3.])`, not as
NumPy's `repr` writes it. A `list` keeps Python's form, `[1.0, 2.5]`. A view prints the elements
it sees: `print(a[::-1])` writes them reversed. To see every digit of an element, print the
element itself: `print(a[2])` writes `0.30000000000000004`. Arrays of `bool`, integers, floats
and complex numbers print; an array of `str` or of structs does not build yet.

Cost: one call into the runtime, which reads and formats at most 1000 elements, however large
the array.

## Math functions

Mathematical functions from `math`, such as `sqrt` and `exp`, apply to numbers. Their element-wise
forms on arrays arrive with roadmap 1.14. These are the functions of Python's `math` module for
real numbers, with Python's names:

| Kind | Functions |
| --- | --- |
| Powers and roots | `sqrt(x)`, `cbrt(x)`, `pow(x, y)`, `hypot(x, y)` |
| Exponentials and logarithms | `exp(x)`, `exp2(x)`, `expm1(x)`, `log(x)`, `log(x, base)`, `log2(x)`, `log10(x)`, `log1p(x)` |
| Trigonometry, in radians | `sin(x)`, `cos(x)`, `tan(x)`, `asin(x)`, `acos(x)`, `atan(x)`, `atan2(y, x)`, `degrees(x)`, `radians(x)` |
| Hyperbolic functions | `sinh(x)`, `cosh(x)`, `tanh(x)`, `asinh(x)`, `acosh(x)`, `atanh(x)` |
| Special functions | `erf(x)`, `erfc(x)`, `gamma(x)`, `lgamma(x)` |
| Sign, remainder and fused multiply-add | `fabs(x)`, `copysign(x, y)`, `fmod(x, y)`, `fma(x, y, z)` |
| Rounding to an `int` | `floor(x)`, `ceil(x)`, `trunc(x)` |
| Tests, giving a `bool` | `isnan(x)`, `isinf(x)`, `isfinite(x)` |

`expm1(x)` and `log1p(x)` stay accurate when `x` is close to zero, where `exp(x) - 1` and
`log(1 + x)` lose every digit. `fma(x, y, z)` is `x * y + z` rounded once. `fmod(x, y)` keeps the
sign of `x`, as C's does, where `x % y` takes the sign of `y`, as Python's does: `fmod(-7.5, 2.0)`
is `-1.5` and `-7.5 % 2.0` is `0.5`. `lgamma` is safe to call from a `prange` loop.

The functions of integers alone, such as `gcd`, `factorial` and `isqrt`, and `prod` come later
in the standard library (roadmap 1.13).

The constants `pi`, `e`, `tau`, `inf` and `nan` are each an `f64` with Python's value.

Most return a float. An `f32` or `f64` argument keeps its type, so `sqrt` of an `f32` is an
`f32`, as `numpy.sqrt` gives. An integer converts to `f64` first, so `sqrt(2)` is
`1.4142135623730951`, as in Python. `pow(x, y)` mixes its arguments as `x ** y` does, so
`pow(x, 0.5)` on an `f32` stays `f32`, and `pow` of an `f32` and an `f64` is an error that asks for
a conversion. `fma` and the other functions of two or three numbers mix the same way. `floor`,
`ceil` and `trunc` return an `int`, as in Python, and panic when the result does not fit in one,
as every conversion of a float to an integer does; `isnan`, `isinf` and `isfinite` return a
`bool`:

```python
from math import atan2, degrees, floor, sqrt


def rms(total: f64, n: int) -> f64:
    return sqrt(total / n)


def bearing(dx: f64, dy: f64) -> f64:
    return degrees(atan2(dy, dx))


def cell(x: f64, width: f64) -> int:
    return floor(x / width)
```

They and float `**` follow IEEE 754 and the C math library, as NumPy does, never raising and never
returning a complex number. This is the rule [float division by zero](types.md#numbers) already
follows:

| Expression | Python | Here |
| --- | --- | --- |
| `sqrt(-1.0)` | `ValueError` | `nan` |
| `log(0.0)` | `ValueError` | `-inf` |
| `atanh(1.0)` | `ValueError` | `inf` |
| `fmod(1.0, 0.0)` | `ValueError` | `nan` |
| `log(-1.0)` | `ValueError` | `nan` |
| `(-8.0) ** (1 / 3)` | a complex number | `nan` |
| `(-8.0) ** 3.0` | `-512.0` | `-512.0` |
| `0.0 ** -1.0` | `ZeroDivisionError` | `inf` |

A negative base with a whole-number exponent gives a real result, as in Python. `x ** y` on
floats is C's `pow(x, y)`. `**` on integers needs a non-negative exponent; a negative one panics.

### Exact sums

`fsum(values)` adds a `list[f64]` or an `Array[f64, 1]` as if with infinite precision and rounds
the result once, to the nearest `f64`, as Python's `math.fsum` does (Shewchuk's algorithm). No
digit is lost to cancellation, however large the terms and in whatever order they come:

```python
from math import fsum


def total_energy(kinetic: list[f64], potential: list[f64]) -> f64:
    return fsum(kinetic) + fsum(potential)
```

| `a` | `a.sum()` | `fsum(a)` |
| --- | --- | --- |
| `Array([1e100, 1.0, -1e100])` | `0.0` | `1.0` |
| `Array([1.0, 1e-16, 1e-16, 1e-16])` | `1.0` | `1.0000000000000002` |

`sum` of the list `[1e100, 1.0, -1e100]` is `1.0` as well, since a list's float `sum` is
compensated as CPython 3.12's is. That compensation catches most cancellation, but only `fsum`
is exact for every input.

Use it where terms cancel: an energy summed over a long orbital integration, a residual, the
check of a result computed another way. `sum` is the fast default: it compensates a list's sum
and adds an array's [pairwise](#the-order-sum-adds-in),
which is accurate enough for most data but can still lose digits when large terms cancel.
`fsum` is several times slower than `sum`, about six float operations per element for each
partial sum it keeps, and it keeps only a few for typical data. It allocates one small list.

A list display such as `fsum([1, 2.5])` reads its numbers as `f64`s, as Python converts them.
Other element types are an error that asks for a conversion: `f64(n)` of each `int`. A table,
`Array[f64, 2]`, is summed one row at a time.

Infinities and NaN give their IEEE 754 sum, as the [other `math` functions](#math-functions) do,
where Python raises:

| Expression | Python | Here |
| --- | --- | --- |
| `fsum([inf, 1.0])` | `inf` | `inf` |
| `fsum([inf, -inf])` | `ValueError` | `nan` |
| `fsum([1e308, 1e308, -1e308])` | `OverflowError` | `inf` |

The last row is a sum whose running total overflows although the exact sum, `1e308`, is an
`f64`; both languages give up on it.

### Double-double floats

`f64x2` holds a value as the unevaluated sum of two `f64`s, `hi + lo`, for about 32 significant
digits where an `f64` has 16 ([decision 0051](../decisions/0051-double-double-floats.md)). Use it
for the few lines that need the digits: the residual of an ill-conditioned solve, a long
accumulation, a check of an `f64` result. Each operation costs about 5 to 20 `f64` operations;
the range stays an `f64`'s.

```python
from math import f64x2


def residual(a: f64, x: f64, b: f64) -> f64:
    """`b - a * x` with the product kept exact, then rounded once."""
    product = f64x2.of(a).mul(f64x2.of(x))
    return f64x2.of(b).sub(product).to_f64()
```

`f64x2.of(x)` makes one from an `f64`, exactly, as the constructor `f64x2(x, 0.0)` does. It
holds that `f64` and nothing more: `f64x2.of(0.1)` is the `f64` nearest 0.1, and
`f64x2.of(1.0).div(f64x2.of(10.0))` is 0.1 to 32 digits. Until a struct can define operators
([decision 0058](../decisions/0058-operators-are-dunder-methods.md), roadmap 3.34), the
operations are methods, each taking and returning an `f64x2`, so `a * b + c` is written
`a.mul(b).add(c)`:

| Method | Gives |
| --- | --- |
| `a.add(b)`, `a.sub(b)`, `a.mul(b)`, `a.div(b)` | `a + b`, `a - b`, `a * b`, `a / b` |
| `a.sqrt()`, `a.neg()`, `a.abs()` | The square root, `-a`, `abs(a)` |
| `a.eq(b)`, `a.ne(b)`, `a.lt(b)`, `a.le(b)`, `a.gt(b)`, `a.ge(b)` | `==`, `!=`, `<`, `<=`, `>`, `>=` as a `bool` |
| `a.to_f64()` | The value rounded to an `f64` |
| `a.to_str()` | 31 significant digits: `f64x2.of(2.0).sqrt().to_str()` is `1.414213562373095048801688724210e+00` |

Each operation is accurate to a few units of 2**-104, about 1e-32 relative; the last digit of
`to_str` can be one off. An infinite or NaN result is the one an `f64` would give, and `sqrt` of
a negative value is NaN, as for the other `math` functions. `f64x2` is `Copy`. `print(a)` writes
its two fields; print `a.to_str()` for the decimal value. Keep `f64x2` code out of
`with fastmath:`, whose reordering silently removes the extra digits.

## Random numbers

`random.Philox` is a counter-based random number generator: the numbers of stream `k` of a seed
are a fixed function of the seed, `k` and how many came before, so a run is reproducible and
parallel streams never share state ([decision 0028](../decisions/0028-philox-random-numbers.md)).

- `Philox(seed, stream=0)` makes a generator. `seed` and `stream` are `int`s; a negative one
  panics.
- `generator.uniform()` draws the next `f64` in `[0, 1)`. It changes the generator, so
  `generator` must be a variable or an `InOut` parameter.

A `Philox` moves; it is not a copy type, so two values can never draw the same numbers by
accident. Give each `prange` chunk its own stream, as the
[Monte Carlo example](../examples/monte_carlo_pi.byn) does:

```python
from random import Philox


def mean_of_draws(seed: int, n: int) -> f64:
    generator = Philox(seed, stream=0)
    total = 0.0
    for _ in range(n):
        total += generator.uniform()
    return total / n
```

The numbers are NumPy's, bit for bit: `Philox(seed, stream=k)` draws what
`numpy.random.Generator(numpy.random.Philox(key=seed).jumped(k)).random()` draws. NumPy's
`Philox(seed)` without `key=` hashes the seed first, so it draws different numbers. More
distributions and generators come with the standard library (roadmap 1.14).
