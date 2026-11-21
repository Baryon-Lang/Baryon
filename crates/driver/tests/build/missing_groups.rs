//! Grouping a frame with columns that may hold `None` (roadmap 3.13.6, decision 0045 part
//! 3), built and run: `groupby` by such a column, and `agg` of one.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// A frame grouped by a text column and by columns that may hold `None`, with columns of
/// `f64`, `i64` and `bool` that may hold `None` reduced every way `agg` has.
const PROGRAM: &str = r#"
class Quote(Struct):
    symbol: str
    venue: i64 | None
    bid: f64 | None
    size: i64 | None
    live: bool | None
    last: f64


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "symbol": Array(["a", "b", "a", "b", "a", "c", "c", "d"]),
        "venue": Array([1, None, 1, 2, None, 2, 2, None]),
        "bid": Array([1.5, None, 2.5, None, 4.0, None, None, 9.0]),
        "size": Array([10, None, 30, None, 50, None, None, 7]),
        "live": Array([True, None, False, None, True, None, None, None]),
        "last": Array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
    })
    by_symbol = quotes.groupby("symbol").agg(
        rows=("last", "count"),
        bids=("bid", "count"),
        total=("bid", "sum"),
        mean=("bid", "mean"),
        low=("bid", "min"),
        high=("bid", "max"),
        first=("bid", "first"),
        last=("bid", "last"),
        spread=("bid", "std"),
        middle=("bid", "median"),
    )
    print(by_symbol[["symbol", "rows", "bids", "total", "mean", "low"]])
    print(by_symbol[["high", "first", "last", "spread", "middle"]])
    sizes = quotes.groupby("symbol").agg(
        sizes=("size", "count"),
        total=("size", "sum"),
        mean=("size", "mean"),
        low=("size", "min"),
        high=("size", "max"),
        first=("size", "first"),
        last=("size", "last"),
        lives=("live", "sum"),
        any=("live", "max"),
    )
    print(sizes)
    by_venue = quotes.groupby("venue").agg(
        rows=("last", "count"), total=("last", "sum"), bid=("bid", "max"), size=("size", "last")
    )
    print(by_venue)
    both = quotes.groupby(["venue", "live"]).agg(rows=("last", "count"))
    print(both)
    low = sizes.low.copy()
    print(low.count(), low[0], low[1])
"#;

/// What [`PROGRAM`] prints. pandas 3.0 gives the same for the same frame with nullable
/// columns, but that it shows `<NA>` where the mean, the spread and the median here are NaN.
const PRINTED: &str = r"
  symbol  rows  bids  total      mean   low
0      a     3     3    8.0  2.666667   1.5
1      b     2     0    0.0       NaN  <NA>
2      c     2     0    0.0       NaN  <NA>
3      d     1     1    9.0  9.000000   9.0
   high  first  last    spread  middle
0   4.0    1.5   4.0  1.258306     2.5
1  <NA>   <NA>  <NA>       NaN     NaN
2  <NA>   <NA>  <NA>       NaN     NaN
3   9.0    9.0   9.0       NaN     9.0
  symbol  sizes  total  mean   low  high  first  last  lives   any
0      a      3     90  30.0    10    50     10    50      2  True
1      b      0      0   NaN  <NA>  <NA>   <NA>  <NA>      0  <NA>
2      c      0      0   NaN  <NA>  <NA>   <NA>  <NA>      0  <NA>
3      d      1      7   7.0     7     7      7     7      0  <NA>
   venue  rows  total   bid  size
0      1     2    4.0   2.5    30
1      2     3   17.0  <NA>  <NA>
   venue   live  rows
0      1  False     1
1      1   True     1
2 10 None
";

/// Each group is reduced by the values that are there: `count` is how many, `sum` adds
/// them, and a group with none has a count and a sum of 0, a mean, spread and median of
/// NaN, and `None` for `min`, `max`, `first` and `last`, whose columns may hold `None`. A
/// `bool` column sums to how many are true. A row whose key is `None` is in no group, with
/// one key or two, and the key column keeps its type.
#[test]
fn a_frame_with_missing_values_is_grouped() -> io::Result<()> {
    let directory = package("missing_groups", &[("main.byn", PROGRAM)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        PRINTED.strip_prefix('\n').unwrap_or(PRINTED)
    );
    Ok(())
}
