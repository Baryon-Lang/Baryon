# Arrays

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

`shape` is a tuple of integers with one entry per dimension, `(nx, ny, nz)`, or a single integer
for rank 1. The rank is the tuple's length, so the tuple must be written out where the array is
created; its entries may be any integer values.

The element type, when the expression does not fix it, is decided in this order:

1. the type the context needs, such as a parameter's or an annotated variable's type:
   `masses: Array[f32, 1] = Array.zeros(n)`;
2. otherwise the optional second argument, as in NumPy's `zeros(shape, dtype)`:
   `Array.zeros((n,), Vec3)` or `Array.zeros((n,), dtype=Vec3)`;
3. otherwise `f64`, as NumPy's default is `float64`.

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
([decision 0010](../decisions/0010-bounds-checks.md)). The compiler removes a check when it can
prove the index is in range, as for `for i in range(len(a)): a[i]`, so most loops pay nothing.
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

## Whole-array arithmetic

Arithmetic operators work element by element on whole arrays and views:

```python
x += alpha * direction
residual -= alpha * product
direction = residual + beta * direction
```

- Both operands must have the same shape, or one must be a single number, which applies to every
  element. Shapes of `Array` operands are checked when the line runs, and a mismatch panics; shapes
  of `Fixed` operands are checked when compiling. Broadcasting between different shapes, NumPy's
  `a + b[:, None]`, comes in Phase 3.
- The number mixing rules of [Types](types.md#number-mixing) apply to the element types.
- An expression of element-wise operations runs as a single loop with no temporary arrays.
  `x += alpha * direction` reads each element of `direction` once and writes each element of `x`
  once.
- Assigning an expression to a name, `direction = residual + beta * direction`, creates one new
  array for the result. The compiler may reuse the storage of the value the assignment replaces;
  that optimization is roadmap 3.1, and it does not change the meaning.

`@` is the matrix product for rank-2 arrays and `Fixed` matrices, and between a matrix and a
vector.

## Methods

Version 0 defines these. The standard library (roadmap 1.14) adds the rest of NumPy's familiar
set.

| Method | Result |
| --- | --- |
| `a.copy()` | A new contiguous array with the same elements |
| `a.sum()`, `a.min()`, `a.max()` | The sum, smallest and largest element |
| `a.dot(b)` | The dot product of two rank-1 arrays or vectors |
| `a.round()` | A new array with each element rounded to the nearest whole number, ties to even |
| `a.fill(value)` | Sets every element to `value`; `a` must be changeable |

Mathematical functions from `math`, such as `sqrt` and `exp`, apply to numbers. Their element-wise
forms on arrays arrive with roadmap 1.14.
