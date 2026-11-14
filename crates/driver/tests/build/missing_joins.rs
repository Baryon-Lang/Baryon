//! Joining frames with columns that may hold `None` (roadmap 3.13.6, decision 0045 part 3),
//! built and run: `merge` on such a column, and with such columns carried along.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Three frames joined on a key that may hold `None` and on a text key, with columns of
/// `f64`, `i64` and `bool` that may hold `None` on both sides.
const PROGRAM: &str = r#"
class Quote(Struct):
    symbol: str
    venue: i64 | None
    bid: f64 | None


class Venue(Struct):
    venue: i64 | None
    open: bool | None
    fee: f64


class Name(Struct):
    symbol: str
    lot: i64 | None


def main():
    quotes: DataFrame[Quote] = DataFrame({
        "symbol": Array(["a", "b", "a", "c", "d"]),
        "venue": Array([1, None, 0, 2, None]),
        "bid": Array([1.5, None, 2.5, None, 9.0]),
    })
    venues: DataFrame[Venue] = DataFrame({
        "venue": Array([None, 1, 0, 3]),
        "open": Array([True, None, False, True]),
        "fee": Array([0.1, 0.2, 0.3, 0.4]),
    })
    names: DataFrame[Name] = DataFrame({
        "symbol": Array(["a", "b", "e"]),
        "lot": Array([100, None, 5]),
    })
    print(quotes.merge(venues, on="venue"))
    print(quotes.merge(names, on="symbol"))
    print(names.merge(quotes, on="symbol").merge(venues, on="venue"))
    joined = quotes.merge(venues, on="venue")
    print(joined.bid.count(), joined.open.count(), len(joined))
"#;

/// What [`PROGRAM`] prints, which is what pandas 3.0 prints for the same frames with
/// nullable columns.
const PRINTED: &str = r"
  symbol  venue   bid   open  fee
0      a      1   1.5   <NA>  0.2
1      b   <NA>  <NA>   True  0.1
2      a      0   2.5  False  0.3
3      d   <NA>   9.0   True  0.1
  symbol  venue   bid   lot
0      a      1   1.5   100
1      b   <NA>  <NA>  <NA>
2      a      0   2.5   100
  symbol   lot  venue   bid   open  fee
0      a   100      1   1.5   <NA>  0.2
1      a   100      0   2.5  False  0.3
2      b  <NA>   <NA>  <NA>   True  0.1
3 3 4
";

/// A key that is `None` joins a key that is `None` and no number: the venue 0, which is
/// also the filler stored under a `None`, joins only the venue 0. Columns that may hold
/// `None` come through a join from either side with each `None` on its row, and the result
/// of one join is joined again.
#[test]
fn frames_with_missing_values_are_joined() -> io::Result<()> {
    let directory = package("missing_joins", &[("main.byn", PROGRAM)])?;
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
