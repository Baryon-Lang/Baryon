# Comparing missing values

An [array with missing values](missing-values.md) compares element by element, and the
answer has gaps where the array does. This chapter covers the comparisons, what the masks
they give can do and how two such masks [combine](#combining-masks)
([decision 0045](../decisions/0045-dataframe.md), part 3).

## Comparisons

`==`, `!=`, `<`, `<=`, `>` and `>=` compare element by element and give an
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

Text compares the same way, by the order Python gives `str`, and `bool`s compare by `==`
and `!=`:

```python
def main():
    names: Array[str | None, 1] = Array(["ada", None, "cy"])
    flags: Array[bool | None, 1] = Array([True, None, False])
    print(names == "ada")
    print(names > "b")
    print(flags == False)
```

This prints `[True None False]`, `[False None True]` and `[False None True]`.

## The rules

- **The other side** is what [arithmetic](missing-values.md#arithmetic) takes: another
  array with missing values, an `Array` of numbers of one dimension, or a single number, on
  either side. Integers compare with floats as floats. The lengths must match.
- **Text compares with text and `bool`s with `bool`s:** the other side of an
  `Array[str | None, 1]` is another, an `Array[str, 1]` or one `str`, and that of an
  `Array[bool | None, 1]` is another, an `Array[bool, 1]` or one `bool`. `bool`s have no
  order, so `flags < flags` is an error, as it is for two plain `bool`s.
- **As a mask of a frame, `None` chooses nothing.** `quotes[quotes.bid > 2.0]` keeps the
  rows where the comparison is `True` and leaves out those where it is `False` or `None`,
  as pandas does.
- **As a mask of an array, the same:** `asks[bids > 2.0]` is the elements of the rank-1
  `Array` `asks` where the comparison is `True`, and `asks[bids > 2.0] = 0.0` sets those.
- **For a plain mask, fill it:** `(bids > 2.0).fillna(False)` is an `Array[bool, 1]`, which
  can be counted with `sum()`.
- **`==` is not a test for `None`.** `bids == None` is not written; ask `bids.isna()`.
- **Not built yet:** a chain `1.0 < bids < 2.0`, `in`, and such a mask choosing elements
  of an array of text, of an array of more than one dimension, or of an array with missing
  values.
- **Cost:** one loop over the stored values and one pass over the bitmaps. Text is one
  pass that compares each pair of texts, and one over the bitmaps. As a mask, two more
  passes make the plain mask of a byte for each element.

## Combining masks

`&`, `|`, `^` and `~` combine masks that may hold `None`. The result is known wherever one
side settles it, which is the rule pandas, Arrow and SQL share:

| `a` | `b` | `a & b` | `a \| b` | `a ^ b` |
| --- | --- | --- | --- | --- |
| `True` | `None` | `None` | `True` | `None` |
| `False` | `None` | `False` | `None` | `None` |
| `None` | `None` | `None` | `None` | `None` |

`False & None` is `False` because no value of the missing side could make it `True`, and
`True | None` is `True` for the same reason. `~None` is `None`. Where both sides are there,
the result is what two `bool`s give.

```python
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0])
    asks = Array([2.0, 3.0, 2.0, 9.0])
    cheap = (bids > 2.0) & (asks < 5.0)
    print(cheap)
    print((bids > 2.0) | (asks < 2.5))
    print(~(bids > 2.0))
    print(asks[cheap])
```

This prints `[False None True False]`, `[True None True True]`, `[True None False False]`
and `[2.]`.

- **Either side can be a plain mask**, an `Array[bool, 1]`, and the result may still hold
  `None`. The lengths must match.
- **Write the parentheses.** `&` binds tighter than `>`, as in Python, so `bids > 2.0 &
  flags` is not `(bids > 2.0) & flags`.
- **`~` and a `None`:** `quotes[~(quotes.bid > 2.0)]` does not choose the rows whose bid is
  `None`, and neither does `quotes[quotes.bid > 2.0]`. Ask for them with
  `quotes[quotes.bid.isna()]`.
- **Not built yet:** a single `True` or `False` as a side, `and` and `or` of whole masks,
  which Python does not define for arrays either, and `any()`, `all()` and `sum()` of a
  mask that may hold `None`: fill it first.
- **Cost:** one loop over the stored values, and for `&` and `|` one pass over both sides
  that makes the bitmap. A plain side costs one more bitmap of a bit for each element.
