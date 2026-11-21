//! The reductions of a grouping written as methods (roadmap 3.13.22, decision 0045's
//! amendment of 2026-11-19), built and run: `df.groupby(k).sum()`, a choice of columns by
//! `[...]` or by `.name`, `size()` and `numeric_only=True`.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// The program of `groups_are_reduced_by_methods`.
const PROGRAM: &str = r#"
def main() -> None:
    maybe: list[f64 | None] = [1.5, None, 3.0, None, 5.0]
    df = DataFrame(
        {
            "k": Array([2, 1, 2, 1, 3]),
            "s": Array(["b", "a", "b", "c", "a"]),
            "v": Array([1.5, 2.5, 3.0, 4.0, 5.0]),
            "n": Array([1, 2, 3, 4, 5]),
            "m": Array(maybe),
        }
    )
    print(df.groupby("k").sum(numeric_only=True))
    print(df.groupby("k").max())
    print(df.groupby("k")["v"].mean())
    print(df.groupby(["k", "s"])[["v", "n"]].min())
    print(df.groupby("s").v.std())
    print(df.groupby("k").size())
    print(df.groupby("k").count())
    print(df.groupby("s").nunique())
    print(df.groupby("k").first())
    print(df.groupby("s").median())
    print(df.groupby("s")[["k", "m"]].last(numeric_only=True))
    print(df.groupby("k")[["s", "v"]].var(numeric_only=True))
    print(df.groupby("s").sum(numeric_only=False))
    print(df.head(0).groupby("k").n.sum())
"#;

/// What [`PROGRAM`] prints.
const EXPECTED: &str = r"   k    v  n    m
0  1  6.5  6  0.0
1  2  4.5  4  4.5
2  3  5.0  5  5.0
   k  s    v  n     m
0  1  c  4.0  4  <NA>
1  2  b  3.0  3   3.0
2  3  a  5.0  5   5.0
   k     v
0  1  3.25
1  2  2.25
2  3  5.00
   k  s    v  n
0  1  a  2.5  2
1  1  c  4.0  4
2  2  b  1.5  1
3  3  a  5.0  5
   s         v
0  a  1.767767
1  b  1.060660
2  c       NaN
   k  size
0  1     2
1  2     2
2  3     1
   k  s  v  n  m
0  1  2  2  2  0
1  2  2  2  2  2
2  3  1  1  1  1
   s  k  v  n  m
0  a  2  2  2  1
1  b  1  2  2  2
2  c  1  1  1  0
   k  s    v  n     m
0  1  a  2.5  2  <NA>
1  2  b  1.5  1   1.5
2  3  a  5.0  5   5.0
   s    k     v    n     m
0  a  2.0  3.75  3.5  5.00
1  b  2.0  2.25  2.0  2.25
2  c  1.0  4.00  4.0   NaN
   s  k     m
0  a  3   5.0
1  b  2   3.0
2  c  1  <NA>
   k      v
0  1  1.125
1  2  1.125
2  3    NaN
   s  k    v  n    m
0  a  4  7.5  7  5.0
1  b  4  4.5  4  4.5
2  c  1  4.0  4  0.0
Empty DataFrame
Columns: [k, n]
Index: []
";

/// Every reduction as a method, over every column, one column, a list of columns and a
/// column by its name, with one key and two, a column of `str` and one that may hold
/// `None`, and a frame of no rows. Each frame is what pandas 3.0.5 prints for the same
/// line with `as_index=False`, except that a missing float prints as `<NA>` where pandas
/// prints `NaN`.
#[test]
fn groups_are_reduced_by_methods() {
    let directory = package("frame_group_methods", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), EXPECTED);
}

/// The program of `the_reference_shows_what_the_methods_print`.
const SHOWN: &str = r#"
def main() -> None:
    trades = DataFrame(
        {
            "symbol": Array([7, 3, 7, 3, 9]),
            "venue": Array(["east", "west", "west", "west", "east"]),
            "price": Array([189.5, 410.0, 191.0, 405.5, 171.0]),
            "volume": Array([100, 250, 300, 50, 75]),
        }
    )
    print(trades.groupby("symbol").sum(numeric_only=True))
    print(trades.groupby("symbol")["price"].mean())
    print(trades.groupby(["venue", "symbol"])[["price", "volume"]].max())
    print(trades.groupby("venue").volume.median())
    print(trades.groupby("symbol").size())
"#;

/// What [`SHOWN`] prints.
const SHOWN_OUTPUT: &str = r"   symbol  price  volume
0       3  815.5     300
1       7  380.5     400
2       9  171.0      75
   symbol   price
0       3  407.75
1       7  190.25
2       9  171.00
  venue  symbol  price  volume
0  east       7  189.5     100
1  east       9  171.0      75
2  west       3  410.0     250
3  west       7  191.0     300
  venue  volume
0  east    87.5
1  west   250.0
   symbol  size
0       3     2
1       7     2
2       9     1
";

/// The example of the reference's "Reductions as methods" prints what the page shows.
#[test]
fn the_reference_shows_what_the_methods_print() {
    let directory = package("frame_group_methods_shown", &[("main.byn", SHOWN)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), SHOWN_OUTPUT);
}
