# Comparing missing values

An [array with missing values](missing-values.md) compares element by element, and the
answer has gaps where the array does. This chapter covers the comparisons and what the
masks they give can do ([decision 0045](../decisions/0045-dataframe.md), part 3).

## Comparisons

`==`, `!=`, `<`, `<=`, `>` and `>=` compare numbers element by element and give an
`Array[bool | None, 1]`: `None` wherever a side is `None`, as pandas's nullable columns do
(Collins, 2026-11-14). Whether a missing bid is above 2.0 is not known, so the answer is
missing too:

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    asks = Array([2.0, 3.0, 2.0])
    print(bids > 2.0)
    print(bids >= asks)
    quotes = DataFrame({"bid": bids.copy(), "ask": asks.copy()})
    print(quotes[quotes.bid > 2.0])
```

This prints `[False None True]` twice, then the one row whose bid is 2.5.

## The rules

- **The other side** is what [arithmetic](missing-values.md#arithmetic) takes: another
  array with missing values, an `Array` of numbers of one dimension, or a single number, on
  either side. Integers compare with floats as floats. The lengths must match.
- **As a mask of a frame, `None` chooses nothing.** `quotes[quotes.bid > 2.0]` keeps the
  rows where the comparison is `True` and leaves out those where it is `False` or `None`,
  as pandas does.
- **For a plain mask, fill it:** `(bids > 2.0).fillna(False)` is an `Array[bool, 1]`, which
  chooses elements of an array, `asks[(bids > 2.0).fillna(False)]`, and can be counted with
  `sum()`.
- **`==` is not a test for `None`.** `bids == None` is not written; ask `bids.isna()`.
- **Not built yet:** comparing text or `bool`s that may be `None`, a chain `1.0 < bids <
  2.0`, `in`, combining two such masks with `&`, `|` and `~`, and such a mask choosing
  elements of an array, `asks[bids > 2.0]`.
- **Cost:** one loop over the stored values and one pass over the bitmaps. As a mask of a
  frame, two more passes make the plain mask of a byte for each row.
