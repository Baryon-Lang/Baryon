# 0041. A sum of a whole-array expression is computed one pairwise leaf at a time

Status: Accepted, amended 2026-11-19
Date: 2026-11-11
Roadmap: 3.9

## Context

`(a * b).sum()` and `((u - v) ** 2).sum()` are the inner products, norms and residuals of most
numerical code. Today the expression is stored in a new array of `n` elements, and then that
array is summed: one allocation, `n` writes and `n` reads more than the sum needs. Measured on
4 million `f64` elements with the release runtime, `(a * b).sum()` takes 5.9 ms and `a.dot(b)`,
which computes the same bits without the array, 3.3 ms.

The result must keep its bits. `a.sum()` of floats adds in NumPy's pairwise tree (roadmap I.22,
chief designer, 2026-11-08), so `(a * b).sum()` has the bits of `numpy.sum(a * b)`. Adding one
element at a time would be one pass, but different bits, and more rounding error.

NumPy's tree splits a run of more than 128 values at the multiple of 8 nearest below its middle,
and adds a run of at most 128, a *leaf*, in 8 running totals. The leaves depend only on the
length, never on the values.

## Decision

### The order is unchanged

A one-pass sum computes each leaf's values in order into a buffer of 128 elements on the stack,
sums the buffer with the runtime's pairwise sum, and stores the leaf's sum. When every leaf is
done, the runtime adds the leaf sums in the tree's order. Each leaf sum is exactly the value the
tree computes for that leaf, so the result has the same bits as summing the stored array, and
as NumPy's.

The leaf sums need one small array, one `f64` per 64 to 128 elements, under 2% of the array a
stored expression needs.

### In the runtime: three functions

- `baryon_rt_pairwise_leaves(count)`: how many leaves the tree of `count` values has.
- `baryon_rt_pairwise_leaf_end(count, start)`: where the leaf that starts at `start` ends.
- `baryon_rt_pairwise_combine_f64(count, sums, leaves)` and its `f32` form: the tree's sum from
  the leaf sums, in order, then added to `0.0` as `sum` does.

Each leaf is summed by the existing `baryon_rt_sum_*` on a view of the buffer.

### In `mir`: three built-ins, no new rvalue

`Builtin::PairwiseLeaves`, `PairwiseLeafEnd` and `PairwiseCombine` call the runtime functions.
No new rvalue is needed. Lowering already writes `e.sum()` as an `Elementwise` assignment to a
temporary, a call of `Builtin::Sum` on it, and a drop of it. Array expansion, which decides how
every whole-array expression is computed, recognizes that shape: a temporary named only by those
three statements is never stored. It expands the expression with the same broadcasting and
length checks as any other, so the panics are the same, but each element goes into the buffer
instead of an array. The loop nest walks the expression in row-major order, fills the buffer, and
flushes it at each leaf's end.

Integer sums are left as they are: lowering adds integers one at a time over the stored array,
and a one-pass integer sum is a separate, smaller change.

### Not in this record

`e.sum(axis=k)` of an expression still stores `e`: lowering builds the reduction's loops over a
stored array, so array expansion never sees the expression and the sum together. Fusing it needs
lowering to hand the axis to array expansion, which is follow-up work.

`Fixed` arrays keep the stored form: they are small, and their sum is already one pass over the
stack.

## Consequences

- `(a * b).sum()` keeps its bits and no longer stores the product. On the same 4 million
  elements it takes 4.1 ms, down from 5.9 ms; `a.dot(b)` takes 3.3 ms. Each row is written in
  runs that end at a leaf's end, so the loop that computes the elements has no branch and
  vectorizes as the stored expression's loop did.
- Only array expansion and the backend change; no pass sees a new kind of statement.
- The buffer size, 128, is NumPy's leaf size. Changing NumPy's tree would change both the stored
  and the one-pass sums together, since both use the runtime's tree.

## Amendment, 2026-11-19: integer sums and sums along an axis

The chief designer accepted this amendment on 2026-11-19 (roadmap 3.9.1, idea I.46).

The two cases the record left out still store the expression: `(a * b + r).sum()` of integers
and `((u - v) * (u - v)).sum(axis=0)`. Measured with the release driver on 4 million elements,
each sum takes about 6 to 7 ms and builds a 32 MB temporary that it reads once. Lowering writes
both sums as its own loops over that temporary, so array expansion never sees that the
temporary is only summed.

### In `mir`: `Sum` takes integers, and one new built-in

- `Builtin::Sum` also takes an array of integers or `bool`s. It is the exact sum, added one
  element at a time in row-major order with checked `+` (decision 0030); a `bool` counts as 0
  or 1 and the destination is an `i64`.
- `Builtin::SumAlong` takes the array, `Borrow`ed, and the axis, an `i64` constant passed
  `Owned`. Its destination gets a new array with the array's lengths less the axis, of the
  element type, or `i64` for `bool`s. Each element is added in the order lowering's loops add
  it today: floats along the last axis in the pairwise tree, as `Sum` adds a row, and every
  other case one value at a time in row-major order.

Lowering writes either form only in the shape the record already uses: `t = e`, the call on
`t`, and a drop of `t`. A sum of a stored array keeps lowering's loops. Array expansion rewrites
every such call, so neither form reaches a backend; the LLVM backend reports a `SumAlong` it
is given instead of building it.

### In array expansion

The expression is expanded with the same broadcasting and length checks as before, so the
panics are the same. An integer sum keeps one running total, so an overflow panics at the same
element. A sum along an axis writes each value into its place in the result. A float sum along
the last axis writes one row at a time into a buffer as long as a row, and sums it with `Sum`,
so each row has the bits it has today; the buffer is one row, not the whole expression.

### Consequences

- The bits and the panics are unchanged, because each order is the one lowering uses today.
- `mir` changes in its own pull request, before the lowering and array expansion work.
