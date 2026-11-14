//! The rows of a frame with columns that may hold `None` (roadmap 3.13.6, decision 0045 part
//! 3), built and run: `DataFrame(rows)`, one row `df[i]`, and a loop over `itertuples()`.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// A frame built from a list of structs with fields that may be `None`, read back a row at
/// a time.
const PROGRAM: &str = r#"
class Quote(Struct):
    symbol: str
    bid: f64 | None
    size: i64 | None
    live: bool | None


def best(quote: Quote) -> f64:
    bid = quote.bid
    if bid is None:
        return -1.0
    return bid


def main():
    rows = [
        Quote("a", 1.5, 10, True),
        Quote("b", None, None, None),
        Quote("c", 0.0, None, False),
    ]
    quotes = DataFrame(rows)
    print(quotes)
    first = quotes[0]
    second = quotes[1]
    print(first.symbol, first.bid, first.size, first.live)
    print(second.symbol, second.bid, second.size, second.live)
    print(best(quotes[-1]), best(second))
    total = 0.0
    gaps = 0
    for quote in quotes.itertuples():
        bid = quote.bid
        if bid is None:
            gaps += 1
        else:
            total += bid
        size = quote.size
        if size is not None:
            print(quote.symbol, size)
    print(total, gaps)
    kept = [quote for quote in quotes.itertuples() if quote.live is not None]
    again = DataFrame(kept)
    print(again)
    none: list[Quote] = []
    empty = DataFrame(none)
    print(len(empty), empty.bid.count())
    print(len(rows), rows[1].bid)
"#;

/// What [`PROGRAM`] prints.
const PRINTED: &str = r"
  symbol   bid  size   live
0      a   1.5    10   True
1      b  <NA>  <NA>   <NA>
2      c   0.0  <NA>  False
a 1.5 10 True
b None None None
0.0 -1.0
a 10
1.5 1
  symbol  bid  size   live
0      a  1.5    10   True
1      c  0.0  <NA>  False
0 0
3 None
";

/// `DataFrame(rows)` puts each field that may be `None` in a column that may hold it, the
/// value 0.0 and `None` staying apart, and leaves the list usable; an empty list gives a
/// frame of no rows. `df[i]` and each row of `itertuples()` hold a `T | None` for such a
/// column, which the program checks before it uses the number, and such rows build a
/// frame again.
#[test]
fn rows_with_missing_values_are_read_and_built() -> io::Result<()> {
    let directory = package("missing_rows", &[("main.byn", PROGRAM)])?;
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
