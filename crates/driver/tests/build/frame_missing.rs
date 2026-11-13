//! Frame columns that may hold `None` (roadmap 3.13.6, decision 0045), built and run: a
//! frame made from its columns, counted, passed, returned and copied, its columns read as
//! arrays with missing values and replaced.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    Ok((
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
    ))
}

/// A frame whose schema has fields that may be `None` is built from arrays with missing
/// values, the schema giving a list written with `None` its type. `len`, `shape` and
/// `columns` count it; a column reads as an array with missing values, by attribute and by
/// name, with `isna`, `notna` and elements that are the value or `None`. A function takes
/// the frame and returns one. A copy has its own columns: replacing them leaves the first
/// frame as it was.
#[test]
fn a_frame_holds_columns_with_missing_values() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    price: f64
    bid: f64 | None
    size: i64 | None


def gaps(quotes: DataFrame[Quote]) -> int:
    count = 0
    for at in range(len(quotes)):
        if quotes.bid[at] is None:
            count += 1
    return count


def load() -> DataFrame[Quote]:
    bids: Array[f64 | None, 1] = Array([1.0, None, 3.0])
    sizes: Array[i64 | None, 1] = Array([None, 2, 3])
    return DataFrame({"price": Array([1.5, 2.5, 3.5]), "bid": bids, "size": sizes})


def main():
    quotes = load()
    print(len(quotes), quotes.shape, quotes.columns, gaps(quotes))
    print(quotes.bid.isna(), quotes["size"].notna(), quotes.size[0], quotes.size[2])
    twin = quotes.copy()
    twin.bid = Array([None, None, 9.0])
    twin["size"] = Array([7, 8, None])
    print(twin.bid[2], twin.bid[0], twin.size[0], quotes.bid[0], gaps(twin))
    mixed = DataFrame({"flag": Array([True, False]), "seen": quotes.bid.isna()[0:2].copy()})
    print(len(mixed))
    worked: DataFrame[Quote] = DataFrame(
        {"price": Array([1.0]), "bid": Array([None]), "size": Array([4])}
    )
    print(worked.bid[0], worked.size[0])
"#;
    let (code, output, _) = run("frame_missing", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "3 (3, 3) ['price', 'bid', 'size'] 1\n\
         [False  True False] [False  True  True] None 3\n\
         9.0 None 7 1.0 2\n\
         2\n\
         None 4\n"
    );
    Ok(())
}

/// A column that may hold `None` of another length than the frame's first column panics when
/// the frame is built, as a column of numbers does.
#[test]
fn a_column_with_missing_values_of_another_length_panics() -> io::Result<()> {
    let program = r#"
def main():
    bids: Array[f64 | None, 1] = Array([1.0, None])
    quotes = DataFrame({"price": Array([1.5, 2.5, 3.5]), "bid": bids})
    print(len(quotes))
"#;
    let (code, output, report) = run("frame_missing_lengths", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 3 and 2"),
        "{report}"
    );
    Ok(())
}

/// `dropna()` of a frame gives the rows with no `None` in any column, its text and number
/// columns chosen with the others, and `fillna` every row with the value in place of each
/// `None`: one value for every column that may hold `None`, or a dict that fills the columns
/// it names and leaves the rest. The columns filled hold no `None` afterwards, so they sum
/// and sort as numbers and the frame prints. A list of names chooses columns of a frame with
/// missing values. A frame with no column that may hold `None` is copied by both.
#[test]
fn a_frame_drops_and_fills_missing_values() -> io::Result<()> {
    let program = r#"
class Quote(Struct):
    symbol: str
    bid: f64 | None
    size: i64 | None
    live: bool | None
    lot: i64


class Trade(Struct):
    symbol: str
    lot: i64


def load() -> DataFrame[Quote]:
    return DataFrame(
        {
            "symbol": Array(["a", "b", "c", "d"]),
            "bid": Array([1.5, None, 2.5, None]),
            "size": Array([10, 20, None, None]),
            "live": Array([True, None, False, None]),
            "lot": Array([1, 2, 3, 4]),
        }
    )


def spread(quotes: DataFrame[Quote]) -> f64:
    whole = quotes.dropna()
    return whole.bid.max() - whole.bid.min()


def main():
    quotes = load()
    whole = quotes.dropna()
    print(whole)
    print(len(whole), len(quotes), spread(quotes), whole.bid.sum())
    print(quotes[["bid", "lot"]].fillna(0.0))
    part = quotes.fillna({"bid": 9.5, "size": 7})
    print(len(part), part.bid, part["size"], part.live.isna())
    full = part.fillna(value=True)
    print(full)
    print(full.sort_values("bid").head(2))
    print(quotes.fillna({"size": 0, "live": False}).dropna())
    trades: DataFrame[Trade] = DataFrame({"symbol": Array(["x", "y"]), "lot": Array([5, 6])})
    print(trades.dropna())
    print(trades.fillna(0).lot, len(quotes[["bid"]].dropna()))
"#;
    let (code, output, _) = run("frame_missing_narrowed", program)?;
    assert_eq!(code, Some(0));
    assert_eq!(
        output,
        "  symbol  bid  size  live  lot\n\
         0      a  1.5    10  True    1\n\
         1 4 0.0 1.5\n\
         \x20  bid  lot\n\
         0  1.5    1\n\
         1  0.0    2\n\
         2  2.5    3\n\
         3  0.0    4\n\
         4 [1.5 9.5 2.5 9.5] [10 20  7  7] [False  True False  True]\n\
         \x20 symbol  bid  size   live  lot\n\
         0      a  1.5    10   True    1\n\
         1      b  9.5    20   True    2\n\
         2      c  2.5     7  False    3\n\
         3      d  9.5     7   True    4\n\
         \x20 symbol  bid  size   live  lot\n\
         0      a  1.5    10   True    1\n\
         1      c  2.5     7  False    3\n\
         \x20 symbol  bid  size   live  lot\n\
         0      a  1.5    10   True    1\n\
         1      c  2.5     0  False    3\n\
         \x20 symbol  lot\n\
         0      x    5\n\
         1      y    6\n\
         [5 6] 2\n"
    );
    Ok(())
}
