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

## Methods

Version 0 defines these. The standard library (roadmap 1.14) adds the rest of NumPy's familiar
set.

| Method | Result |
| --- | --- |
| `a.copy()` | A new contiguous array with the same elements |
| `a.sum()`, `a.min()`, `a.max()` | The sum, smallest and largest element |
| `a.dot(b)` | The dot product of two rank-1 arrays or vectors of the same length |
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

`a.dot(b)` of floats adds its products in the same tree: each product `a[i] * b[i]` is rounded,
never fused with the add that follows, and the products are summed as `sum` sums its elements. So
`a.dot(b)` has the same bits as `(a * b).sum()` and as `numpy.sum(a * b)`, without building
`a * b`. `numpy.dot` itself calls BLAS, whose order depends on the machine, so its last bits
can differ. For fewer than 8 elements, such as a 3-vector, the tree adds in plain order. The same reasoning fixes the order of reductions in
parallel loops; see [reproducible reductions](parallel-loops.md#reproducible-reductions).

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
`@` will call BLAS (roadmap 3.10), which adds in an order that depends on the machine, as
NumPy's `@` does. Where the last bits must be reproducible, use `dot` and `sum`, whose order is
fixed.

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

The functions of integers alone, such as `gcd`, `factorial` and `isqrt`, and those of
sequences, such as `fsum` and `prod`, come later in the standard library (roadmap 1.13).

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
