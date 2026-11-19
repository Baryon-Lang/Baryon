//! Left, right and outer joins (roadmap 3.13.8, decision 0045), built and run:
//! `left.merge(right, on=..., how="left")`, `"right"` and `"outer"`, whose rows without a
//! partner hold `None` in the other frame's columns.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// The three joins on a key of text, on two keys of which one may hold `None`, on one key
/// with a shared column, and with a frame that has no rows.
const PROGRAM: &str = r#"
class Trade(Struct):
    symbol: str
    size: i64


class Quote(Struct):
    symbol: str
    bid: f64


class Seen(Struct):
    day: i64 | None
    code: str
    count: i64 | None


class Note(Struct):
    day: i64 | None
    code: str
    text: str | None
    live: bool


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array(["b", "a", "c", "a", "z"]),
        "size": Array([1, 2, 3, 4, 9]),
    })
    quotes: DataFrame[Quote] = DataFrame({
        "symbol": Array(["a", "d", "b", "a", "c2"]),
        "bid": Array([10.5, 20.5, 30.5, 40.5, 1.0]),
    })
    print(trades.merge(quotes, on="symbol", how="left"))
    print(trades.merge(quotes, on="symbol", how="right"))
    print(trades.merge(quotes, on="symbol", how="outer"))
    seen: DataFrame[Seen] = DataFrame({
        "day": Array([2, None, 1, 1]),
        "code": Array(["x", "x", "y", "x"]),
        "count": Array([5, 6, None, 8]),
    })
    notes: DataFrame[Note] = DataFrame({
        "day": Array([None, 3, 2, 1]),
        "code": Array(["x", "x", "x", "z"]),
        "text": Array(["late", None, "ok", "odd"]),
        "live": Array([True, False, True, False]),
    })
    print(seen.merge(notes, on=["day", "code"], how="left"))
    print(seen.merge(notes, on=["day", "code"], how="right"))
    both = seen.merge(notes, on=["day", "code"], how="outer")
    print(both)
    print(both.count.count(), both.text.count(), both.live.count())
    print(seen.merge(notes, on="code", how="outer"))
    empty = trades[trades.size > 100]
    print(empty.merge(quotes, on="symbol", how="right"))
    print(trades.merge(quotes[quotes.bid > 100.0], on="symbol", how="left"))
"#;

/// What [`PROGRAM`] prints, which is what pandas 3.0.5 prints for the same frames with its
/// nullable column types.
const PRINTED: &str = r"
  symbol  size   bid
0      b     1  30.5
1      a     2  10.5
2      a     2  40.5
3      c     3  <NA>
4      a     4  10.5
5      a     4  40.5
6      z     9  <NA>
  symbol  size   bid
0      a     2  10.5
1      a     4  10.5
2      d  <NA>  20.5
3      b     1  30.5
4      a     2  40.5
5      a     4  40.5
6     c2  <NA>   1.0
  symbol  size   bid
0      a     2  10.5
1      a     2  40.5
2      a     4  10.5
3      a     4  40.5
4      b     1  30.5
5      c     3  <NA>
6     c2  <NA>   1.0
7      d  <NA>  20.5
8      z     9  <NA>
    day code  count  text  live
0     2    x      5    ok  True
1  <NA>    x      6  late  True
2     1    y   <NA>  <NA>  <NA>
3     1    x      8  <NA>  <NA>
    day code  count  text   live
0  <NA>    x      6  late   True
1     3    x   <NA>  <NA>  False
2     2    x      5    ok   True
3     1    z   <NA>   odd  False
    day code  count  text   live
0     1    x      8  <NA>   <NA>
1     1    y   <NA>  <NA>   <NA>
2     1    z   <NA>   odd  False
3     2    x      5    ok   True
4     3    x   <NA>  <NA>  False
5  <NA>    x      6  late   True
3 3 4
    day_x code  count  day_y  text   live
0       2    x      5   <NA>  late   True
1       2    x      5      3  <NA>  False
2       2    x      5      2    ok   True
3    <NA>    x      6   <NA>  late   True
4    <NA>    x      6      3  <NA>  False
5    <NA>    x      6      2    ok   True
6       1    x      8   <NA>  late   True
7       1    x      8      3  <NA>  False
8       1    x      8      2    ok   True
9       1    y   <NA>   <NA>  <NA>   <NA>
10   <NA>    z   <NA>      1   odd  False
  symbol  size   bid
0      a  <NA>  10.5
1      d  <NA>  20.5
2      b  <NA>  30.5
3      a  <NA>  40.5
4     c2  <NA>   1.0
  symbol  size   bid
0      b     1  <NA>
1      a     2  <NA>
2      c     3  <NA>
3      a     4  <NA>
4      z     9  <NA>
";

/// A left join follows the left frame's rows and a right join the right frame's, each with
/// `None` where the other frame has no partner; an outer join follows the keys in sorted
/// order with a `None` key last, and takes each key from the frame that has the row. A
/// `None` key joins a `None` key. Columns of numbers, `bool` and text all take the gaps.
#[test]
fn left_right_and_outer_joins_build_and_run() -> io::Result<()> {
    let directory = package("frame_outer", &[("main.byn", PROGRAM)])?;
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
