//! Missing values shown (roadmap 3.13.6, decision 0045 part 3), built and run: `print` and
//! an f-string write a frame with a column that may hold `None` and an array with missing
//! values.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// A frame with columns that may hold `None`, of `f64`, `i64`, `bool` and `f32`.
const PROGRAM: &str = r#"
from math import nan


class Quote(Struct):
    bid: f64 | None
    n: i64 | None
    f: bool | None
    x: f64
    g: f64 | None
    s: f64 | None
    t: f32 | None


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "bid": Array([1.5, None, 2.25]),
        "n": Array([1, None, -3]),
        "f": Array([True, None, False]),
        "x": Array([1.0, nan, 2.5]),
        "g": Array([None, None, None]),
        "s": Array([1e-9, nan, 1e7]),
        "t": Array([1.0, 2.0, None]),
    })
    print(quotes)
    print(f"{quotes.dropna()}")
    bids = Array([1.5, None, 2.5])
    print(bids)
    print(quotes.n, quotes.f)
    text = f"bids: {bids}"
    print(text)
    empty: Array[f64 | None, 1] = Array([])
    print(empty)
    long: list[i64 | None] = []
    for at in range(1001):
        long.append(None)
    long[1] = 7
    print(Array(long))
    big = DataFrame({"a": Array(long)})
    print(big)
"#;

/// What [`PROGRAM`] prints. The first table is what pandas 3.0 prints for the same frame
/// with nullable columns, but for the `NaN` in the column `s`, which pandas shows as `<NA>`
/// and Baryon keeps apart from a missing value.
const PRINTED: &str = r"
    bid     n      f    x     g           s     t
0   1.5     1   True  1.0  <NA>         0.0   1.0
1  <NA>  <NA>   <NA>  NaN  <NA>         NaN   2.0
2  2.25    -3  False  2.5  <NA>  10000000.0  <NA>
Empty DataFrame
Columns: [bid, n, f, x, g, s, t]
Index: []
[1.5 None 2.5]
[1 None -3] [True None False]
bids: [1.5 None 2.5]
[]
[None 7 None ... None None None]
         a
0     <NA>
1        7
2     <NA>
3     <NA>
4     <NA>
...    ...
996   <NA>
997   <NA>
998   <NA>
999   <NA>
1000  <NA>

[1001 rows x 1 columns]
";

/// A missing cell of a frame prints as `<NA>`, each float of a column that may hold `None`
/// with its own decimals as pandas writes a nullable column, and a real `NaN` as `NaN`. A
/// frame with no rows left names its columns, and a long one shows its first and last five
/// rows. An array alone prints its elements with `None` for each gap, as `NumPy` prints an
/// array of objects, on its own, as a column and in an f-string, with `...` in the middle
/// of more than 1000 elements.
#[test]
fn missing_values_are_shown() -> io::Result<()> {
    let directory = package("missing_shown", &[("main.byn", PROGRAM)])?;
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
