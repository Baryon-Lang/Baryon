//! Choosing and changing the columns of a frame (roadmap 3.13.4, decision 0045), built and
//! run: `df[["a", "b"]]`, `assign`, `drop` and `rename`, with the printed frames those
//! pandas prints for the same steps, and the panic for a new column of another length.

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

/// Columns picked from a borrowed frame, in another order; `assign` adding one column and
/// replacing another in one call, reading the frame it takes; a frame `assign` gives
/// returned as a declared frame type; `drop` by a list and by one name; `rename`, also
/// swapping two names; and the three chained on a frame that has no name.
#[test]
fn columns_are_chosen_added_dropped_and_renamed() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64

class Priced(Struct):
    price: f64
    volume: i64
    notional: f64

def notional(trades: DataFrame[Trade]) -> f64:
    pair = trades[["volume", "price"]]
    return (pair.price * pair.volume).sum()

def valued(trades: DataFrame[Trade]) -> DataFrame[Priced]:
    return trades.copy().assign(notional=trades.price * trades.volume)

def main():
    trades: DataFrame[Trade] = DataFrame({
        "price": Array([189.5, 410.0, 171.0]),
        "volume": Array([100, 250, 50]),
    })
    print(notional(trades))
    print(trades[["volume"]])
    priced = valued(trades)
    print(priced[1])
    wide = trades.assign(notional=trades.price * trades.volume, price=trades.price * 2.0)
    print(wide)
    print(wide.columns)
    narrow = wide.drop(columns=["volume"])
    print(narrow)
    named = narrow.rename(columns={"price": "cost", "notional": "value"})
    print(named)
    print(named.cost.sum(), named.shape)
    one = named.drop(columns="cost")
    print(one)
    chained = (
        DataFrame({"a": Array([1, 2]), "b": Array([3, 4])})
        .assign(c=Array([5, 6]))
        .drop(columns=["a"])
        .rename(columns={"b": "x"})
    )
    print(chained[["c", "x"]].head(1))
    swapped = chained.rename(columns={"x": "c", "c": "x"})
    print(swapped)
"#;
    let expected = [
        "130000.0\n",
        "   volume\n",
        "0     100\n",
        "1     250\n",
        "2      50\n",
        "Priced(price=410.0, volume=250, notional=102500.0)\n",
        "   price  volume  notional\n",
        "0  379.0     100   18950.0\n",
        "1  820.0     250  102500.0\n",
        "2  342.0      50    8550.0\n",
        "['price', 'volume', 'notional']\n",
        "   price  notional\n",
        "0  379.0   18950.0\n",
        "1  820.0  102500.0\n",
        "2  342.0    8550.0\n",
        "    cost     value\n",
        "0  379.0   18950.0\n",
        "1  820.0  102500.0\n",
        "2  342.0    8550.0\n",
        "1541.0 (3, 2)\n",
        "      value\n",
        "0   18950.0\n",
        "1  102500.0\n",
        "2    8550.0\n",
        "   c  x\n",
        "0  5  3\n",
        "   c  x\n",
        "0  3  5\n",
        "1  4  6\n",
    ];
    let (code, stdout, report) = run("frames-schema", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}

/// A column `assign` gives must have the frame's number of rows: another length panics at
/// the argument, before the frame changes hands.
#[test]
fn a_new_column_of_another_length_panics() {
    let program = r#"
def main():
    df = DataFrame({"a": Array([1, 2]), "b": Array([3, 4])})
    wide = df.assign(c=Array([5, 6, 7]))
    print(wide)
"#;
    let (code, stdout, report) = run("frames-schema-length", program).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert!(
        report.starts_with(
            "panic: array lengths differ in dimension 0: 2 and 3\n  --> main.byn:4:24"
        ),
        "{report}"
    );
}

/// One value in `assign` fills every row (roadmap 3.13.18): a number, a `bool` or a `str`,
/// a literal or computed, added at the end or replacing a column, also on a frame with no
/// rows. `drop` and `rename` take pandas's `axis=1` and `axis="columns"` forms. The
/// expected text is what pandas 3.0.5 prints for the same program.
#[test]
fn one_value_fills_a_column_and_axis_names_the_columns() {
    let program = r#"
def main():
    venue = "NY" + "SE"
    rate = 0.25
    trades = DataFrame({
        "symbol": Array(["b", "a", "c"]),
        "price": Array([2.5, 4.0, 1.5]),
        "volume": Array([10, 30, 5]),
    })
    print(trades.copy().assign(fee=0.5))
    print(trades.copy().assign(lot=100, live=True, venue=venue, note="x", cost=trades.price * rate))
    print(trades.copy().assign(price=1.0, half=rate / 2.0))
    print(trades.copy().drop("volume", axis=1))
    print(trades.copy().drop(["price", "volume"], axis="columns"))
    print(trades.copy().drop(labels="symbol", axis=1))
    print(trades.copy().rename({"price": "cost"}, axis=1))
    print(trades.copy().rename(mapper={"symbol": "ticker"}, axis="columns"))
    none = trades[trades.price > 9.0]
    print(none.assign(fee=0.5, venue="x"))
"#;
    let expected = [
        "  symbol  price  volume  fee\n",
        "0      b    2.5      10  0.5\n",
        "1      a    4.0      30  0.5\n",
        "2      c    1.5       5  0.5\n",
        "  symbol  price  volume  lot  live venue note   cost\n",
        "0      b    2.5      10  100  True  NYSE    x  0.625\n",
        "1      a    4.0      30  100  True  NYSE    x  1.000\n",
        "2      c    1.5       5  100  True  NYSE    x  0.375\n",
        "  symbol  price  volume   half\n",
        "0      b    1.0      10  0.125\n",
        "1      a    1.0      30  0.125\n",
        "2      c    1.0       5  0.125\n",
        "  symbol  price\n",
        "0      b    2.5\n",
        "1      a    4.0\n",
        "2      c    1.5\n",
        "  symbol\n",
        "0      b\n",
        "1      a\n",
        "2      c\n",
        "   price  volume\n",
        "0    2.5      10\n",
        "1    4.0      30\n",
        "2    1.5       5\n",
        "  symbol  cost  volume\n",
        "0      b   2.5      10\n",
        "1      a   4.0      30\n",
        "2      c   1.5       5\n",
        "  ticker  price  volume\n",
        "0      b    2.5      10\n",
        "1      a    4.0      30\n",
        "2      c    1.5       5\n",
        "Empty DataFrame\n",
        "Columns: [symbol, price, volume, fee, venue]\n",
        "Index: []\n",
    ];
    let (code, stdout, report) = run("frames-schema-scalar", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
