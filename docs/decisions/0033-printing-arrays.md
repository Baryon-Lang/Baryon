# 0033. `print` writes an array as NumPy does

Status: Accepted
Date: 2026-11-09
Roadmap: D.17, 1.10.15

## Context

`print` writes every other value as Python writes it ([`print`](../reference/syntax.md#what-print-writes-for-each-value)).
An array has no Python form of its own, and the two forms a reader of this language already
knows disagree:

| | NumPy, `print(numpy_array)` | Python, `print(numpy_array.tolist())` |
| --- | --- | --- |
| `[1.0, 2.0, 3.0]` | `[1. 2. 3.]` | `[1.0, 2.0, 3.0]` |
| `[0.1 + 0.2, 1 / 3]` | `[0.3        0.33333333]` | `[0.30000000000000004, 0.3333333333333333]` |
| Integers `[1, 2, 3]` | `[1 2 3]` | `[1, 2, 3]` |
| A 2 × 2 matrix | `[[1.  2.5]` and ` [3.  4. ]` on two lines, columns aligned | `[[1.0, 2.5], [3.0, 4.0]]` on one line |
| 2000 elements | `[0.000e+00 1.000e+00 2.000e+00 ... 1.997e+03 1.998e+03 1.999e+03]` | All 2000 numbers |

These are NumPy 2.5's default print options, checked by running it. Roadmap D.17 asks which one
`print` writes; nothing prints an array until it is answered.

What bears on the choice:

- **Arrays here already follow NumPy, not Python lists.** `a.sum()` adds in NumPy's pairwise
  order so it gives `numpy.sum`'s bits ([Arrays](../reference/arrays.md#the-order-sum-adds-in)),
  and `random.Philox` gives NumPy's numbers ([0028](0028-philox-random-numbers.md)). A
  physicist porting a NumPy script compares its output line by line with the old one.
- **Arrays can be huge.** The language exists to make enormous arrays routine. Printing one by
  mistake, or to look at its corners, must not write 10⁸ numbers. NumPy summarizes any array of
  more than 1000 elements to its first and last three along each axis; Python's list form has no
  such rule, so we would invent one.
- **Matrices are read as grids.** Rows on their own lines with aligned columns are how a matrix
  reads on paper; one long line of nested brackets is not.
- **NumPy's form hides digits.** It prints 8 significant digits by default, so
  `0.30000000000000004` shows as `0.3`, while `print` of the same `f64` alone shows every digit.
  Python's form never hides a digit. NumPy users already live with this: `print(x)` and
  `print(a)` differ the same way there.
- **Cost.** Python's form is a loop over elements with `, ` between them: a few dozen lines in the
  runtime. NumPy's is a real formatter: one width and one notation chosen per array, scientific
  notation when the largest and smallest magnitudes differ by more than 1000, the largest
  reaches 10⁸ or the smallest is below 10⁻⁴, a
  sign column, line wrapping at 75 characters, and the summary. It is a few hundred lines of
  runtime code, testable byte for byte against NumPy's output.

## Decision

Collins chose this on 2026-11-09.

**`print` writes an `Array` or `Fixed` array exactly as NumPy's `print` writes an
`ndarray` with the default print options**, byte for byte:

- Floats with up to 8 significant digits, trailing zeros dropped (`1.`, `2.5`), one notation
  and width per array; integers plain (`[1 2 3]`); `bool` as `[ True False]`.
- Each row of a rank-2 or higher array on its own line, columns aligned, a blank line between
  the blocks of a rank-3 array.
- More than 1000 elements: the first and last 3 along each axis, `...` between.
- Lines wrapped at 75 characters as NumPy wraps them.
- Inside a tuple or a struct, the same form, `Grid(cells=[1. 2. 3.])`, not NumPy's
  `array([1., 2., 3.])`: printing a struct favors reading over evaluating, as an enum member
  printing `Phase.GAS` already does.
- A `list` keeps Python's form, `[1.0, 2.0]`, as it does in Python next to NumPy.

The tests compare the runtime's output with NumPy's for the same arrays, as the Philox tests do
([0028](0028-philox-random-numbers.md)). Options such as precision or the summary threshold are
not part of this decision; they would come with `numpy.set_printoptions`'s equivalent, if ever.

The alternative is Python's list form, `[1.0, 2.0, 3.0]`, nested for higher ranks, every digit
shown, with a summary rule of our own for large arrays. It is cheaper to build and never hides a
digit, but a ported NumPy script prints different lines, a matrix reads as one line, and the
summary rule would be ours alone. A third form, NumPy's layout with Python's shortest digits,
matches neither and is not proposed.

## Consequences

- A NumPy user sees the output they expect, and a script ported from NumPy prints the same
  lines, which is how ports get checked.
- Printing a huge array is safe: at most a few dozen numbers appear.
- `print(a)` can show fewer digits than `print(a[0])`. To see every digit, print the element, or
  use an f-string with a format specification.
- The runtime gains a formatter of a few hundred lines that tracks NumPy's defaults. If NumPy
  ever changes them, as NumPy 2.0 did for scalars' `repr`, this follows the version the record
  names, until a new record says otherwise.
- Building it is roadmap item 1.10.15, after release 0.2.0 (roadmap R.2).
