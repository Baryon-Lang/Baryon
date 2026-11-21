//! Rows compared in a frame with columns that may hold `None` (roadmap 3.13.6, decision 0045
//! part 3), built and run: `drop_duplicates` and `value_counts`, and such a frame summed up by
//! `describe`.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// A frame with a text column and columns of `f64`, `i64` and `bool` that may hold `None`,
/// with rows that repeat, some of them in their `None`s.
const PROGRAM: &str = r#"
class Quote(Struct):
    symbol: str
    bid: f64 | None
    size: i64 | None
    live: bool | None


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "symbol": Array(["a", "b", "a", "b", "a", "c", "c"]),
        "bid": Array([1.5, None, 1.5, None, None, 0.0, None]),
        "size": Array([1, None, 1, None, 2, 0, None]),
        "live": Array([True, None, True, None, False, False, None]),
    })
    print(quotes.drop_duplicates())
    print(quotes.drop_duplicates(subset=["bid"]))
    print(quotes.drop_duplicates(subset=["symbol", "live"]))
    print(quotes.value_counts("bid"))
    print(quotes.value_counts(["size", "live"]))
    print(quotes.value_counts(["symbol", "bid"]))
    print(quotes[["bid", "size"]].value_counts())
    counted = quotes.value_counts("size")
    print(counted.size.sum(), counted["count"].sum())
"#;

/// What [`PROGRAM`] prints, which is what pandas 3.0 prints for the same frame with nullable
/// columns, but that pandas keeps the row numbers of the first frame in `drop_duplicates`.
const PRINTED: &str = r"
  symbol   bid  size   live
0      a   1.5     1   True
1      b  <NA>  <NA>   <NA>
2      a  <NA>     2  False
3      c   0.0     0  False
4      c  <NA>  <NA>   <NA>
  symbol   bid  size   live
0      a   1.5     1   True
1      b  <NA>  <NA>   <NA>
2      c   0.0     0  False
  symbol   bid  size   live
0      a   1.5     1   True
1      b  <NA>  <NA>   <NA>
2      a  <NA>     2  False
3      c   0.0     0  False
4      c  <NA>  <NA>   <NA>
   bid  count
0  1.5      2
1  0.0      1
   size   live  count
0     1   True      2
1     2  False      1
2     0  False      1
  symbol  bid  count
0      a  1.5      2
1      c  0.0      1
   bid  size  count
0  1.5     1      2
1  0.0     0      1
3 4
";

/// `drop_duplicates` treats a `None` as equal to a `None` and as different from every value,
/// the filler stored under a `None` included: the row with a bid of 0.0 is not a repeat of a
/// row with no bid. It compares every column or those `subset` names, and each `None` stays
/// with its row. `value_counts` counts no row with a `None` in a column counted, by one
/// column, by several and by all, and its columns keep their types.
#[test]
fn rows_with_missing_values_are_compared() -> io::Result<()> {
    let directory = package("missing_distinct", &[("main.byn", PROGRAM)])?;
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

/// A frame with columns of numbers that may hold `None`, one of them all `None`, beside a
/// column of `f64` and columns `describe` leaves out.
const DESCRIBED: &str = r#"
class Quote(Struct):
    symbol: str
    bid: f64 | None
    n: i64 | None
    x: f64
    g: f64 | None
    live: bool | None


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "symbol": Array(["a", "b", "c", "d"]),
        "bid": Array([1.5, None, 2.5, None]),
        "n": Array([1, None, 3, 5]),
        "x": Array([1.0, 2.0, 3.0, 4.0]),
        "g": Array([None, None, None, None]),
        "live": Array([True, None, False, None]),
    })
    summary = quotes.describe()
    print(summary)
    print(quotes[["bid"]].describe())
"#;

/// What [`DESCRIBED`] prints: the numbers pandas 3.0 gives for the same frame with nullable
/// columns, but for the column with no values, where pandas gives `<NA>` under a count of 0.
const SUMMARY: &str = r"
  statistic       bid    n         x    g
0     count  2.000000  3.0  4.000000  0.0
1      mean  2.000000  3.0  2.500000  NaN
2       std  0.707107  2.0  1.290994  NaN
3       min  1.500000  1.0  1.000000  NaN
4       25%  1.750000  2.0  1.750000  NaN
5       50%  2.000000  3.0  2.500000  NaN
6       75%  2.250000  4.0  3.250000  NaN
7       max  2.500000  5.0  4.000000  NaN
  statistic       bid
0     count  2.000000
1      mean  2.000000
2       std  0.707107
3       min  1.500000
4       25%  1.750000
5       50%  2.000000
6       75%  2.250000
7       max  2.500000
";

/// `describe` sums up a column that may hold `None` by the values that are there: the count
/// is how many, and the mean, the deviation and the quartiles are theirs. A column with no
/// values has a count of 0 and NaN for the rest, as an empty column has. Text and `bool`
/// columns are left out as in any frame.
#[test]
fn a_frame_with_missing_values_is_described() -> io::Result<()> {
    let directory = package("missing_described", &[("main.byn", DESCRIBED)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        SUMMARY.strip_prefix('\n').unwrap_or(SUMMARY)
    );
    Ok(())
}
