//! Text columns of a `DataFrame` as keys (roadmap 3.13.3, decision 0045), built and run:
//! frames put in order by a column of `str`, their repeated rows left out, frames joined on
//! one, rows grouped by one, and one reduced in each group. The frames printed are compared
//! with what pandas 3 prints.

use super::text_columns::run;

/// A program that sorts frames by a text column: rising and falling, as the second key
/// after a column of numbers and as the first key before one, with texts that repeat, an
/// empty text, capitals, small letters and a letter outside ASCII, and a frame of no rows.
const SORTED: &str = r#"
def main():
    symbols = Array(["IBM", "", "AAPL", "βeta", "MSFT", "IBM", "x", "Zed", "aapl", "IB"])
    prices = Array([3.0, 1.0, 2.0, 4.0, 0.5, 9.0, 2.0, 2.0, 7.0, 3.0])
    sizes = Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
    trades = DataFrame({"symbol": symbols, "price": prices, "size": sizes})
    print(trades.sort_values("symbol"))
    print(trades.sort_values("symbol", ascending=False))
    print(trades.sort_values(["price", "symbol"]))
    print(trades.sort_values(["symbol", "price"], ascending=[True, False]))
    print(trades.sort_values(by=["price", "symbol"], ascending=[False, False]).symbol[0])
    alone = DataFrame({"tag": Array(["b", "a", "c", "a"])})
    print(alone.sort_values("tag"))
    none = alone.head(0).sort_values("tag")
    print(len(none))
    print(len(trades), trades.symbol[3])
"#;

/// A sort by a text column puts the rows in the order of Python's `<` on `str`, which is
/// the order of the code points, so capitals come before small letters: every frame printed
/// is what pandas 3 prints for a stable sort, numbered from 0 again.
#[test]
fn a_frame_is_sorted_by_a_text_column() {
    let expected = [
        "  symbol  price  size\n",
        "0           1.0     2\n",
        "1   AAPL    2.0     3\n",
        "2     IB    3.0    10\n",
        "3    IBM    3.0     1\n",
        "4    IBM    9.0     6\n",
        "5   MSFT    0.5     5\n",
        "6    Zed    2.0     8\n",
        "7   aapl    7.0     9\n",
        "8      x    2.0     7\n",
        "9   βeta    4.0     4\n",
        "  symbol  price  size\n",
        "0   βeta    4.0     4\n",
        "1      x    2.0     7\n",
        "2   aapl    7.0     9\n",
        "3    Zed    2.0     8\n",
        "4   MSFT    0.5     5\n",
        "5    IBM    3.0     1\n",
        "6    IBM    9.0     6\n",
        "7     IB    3.0    10\n",
        "8   AAPL    2.0     3\n",
        "9           1.0     2\n",
        "  symbol  price  size\n",
        "0   MSFT    0.5     5\n",
        "1           1.0     2\n",
        "2   AAPL    2.0     3\n",
        "3    Zed    2.0     8\n",
        "4      x    2.0     7\n",
        "5     IB    3.0    10\n",
        "6    IBM    3.0     1\n",
        "7   βeta    4.0     4\n",
        "8   aapl    7.0     9\n",
        "9    IBM    9.0     6\n",
        "  symbol  price  size\n",
        "0           1.0     2\n",
        "1   AAPL    2.0     3\n",
        "2     IB    3.0    10\n",
        "3    IBM    9.0     6\n",
        "4    IBM    3.0     1\n",
        "5   MSFT    0.5     5\n",
        "6    Zed    2.0     8\n",
        "7   aapl    7.0     9\n",
        "8      x    2.0     7\n",
        "9   βeta    4.0     4\n",
        "IBM\n",
        "  tag\n",
        "0   a\n",
        "1   a\n",
        "2   b\n",
        "3   c\n",
        "0\n",
        "10 βeta\n",
    ];
    let (code, stdout, report) = run("text-columns-sorted", SORTED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}
/// A program that leaves out the rows that repeat in frames with a text column: comparing
/// every column, the text column alone, the text column with a column of numbers in both
/// orders, and a column of numbers alone, with texts that differ only in case, an empty text
/// and a letter outside ASCII.
const DISTINCT: &str = r#"
def main():
    symbols = Array(["IBM", "", "AAPL", "IBM", "", "ibm", "AAPL", "IBM", "βeta", "βeta"])
    prices = Array([3.0, 1.0, 2.0, 3.0, 1.5, 3.0, 2.0, 4.0, 2.0, 2.0])
    live = Array([True, False, True, True, False, True, False, True, True, True])
    trades = DataFrame({"symbol": symbols, "price": prices, "live": live})
    print(trades.drop_duplicates())
    print(trades.drop_duplicates(subset="symbol"))
    print(trades.drop_duplicates(subset=["symbol", "price"]))
    print(trades.drop_duplicates(["price", "symbol"]))
    print(trades.drop_duplicates(subset=["price"]))
    print(trades.drop_duplicates("symbol").sort_values("symbol").symbol[0] == "")
    tags = DataFrame({"tag": Array(["b", "a", "b", "", "a", "c"])})
    print(tags.drop_duplicates())
    print(len(tags.head(0).drop_duplicates()), len(trades), trades.symbol[8])
"#;

/// `drop_duplicates` compares text as it compares numbers and keeps the first of the rows
/// that repeat, in the frame's order: every frame printed is what pandas 3 prints, numbered
/// from 0 again.
#[test]
fn rows_that_repeat_in_a_frame_with_a_text_column_are_left_out() {
    let expected = [
        "  symbol  price   live\n",
        "0    IBM    3.0   True\n",
        "1           1.0  False\n",
        "2   AAPL    2.0   True\n",
        "3           1.5  False\n",
        "4    ibm    3.0   True\n",
        "5   AAPL    2.0  False\n",
        "6    IBM    4.0   True\n",
        "7   βeta    2.0   True\n",
        "  symbol  price   live\n",
        "0    IBM    3.0   True\n",
        "1           1.0  False\n",
        "2   AAPL    2.0   True\n",
        "3    ibm    3.0   True\n",
        "4   βeta    2.0   True\n",
        "  symbol  price   live\n",
        "0    IBM    3.0   True\n",
        "1           1.0  False\n",
        "2   AAPL    2.0   True\n",
        "3           1.5  False\n",
        "4    ibm    3.0   True\n",
        "5    IBM    4.0   True\n",
        "6   βeta    2.0   True\n",
        "  symbol  price   live\n",
        "0    IBM    3.0   True\n",
        "1           1.0  False\n",
        "2   AAPL    2.0   True\n",
        "3           1.5  False\n",
        "4    ibm    3.0   True\n",
        "5    IBM    4.0   True\n",
        "6   βeta    2.0   True\n",
        "  symbol  price   live\n",
        "0    IBM    3.0   True\n",
        "1           1.0  False\n",
        "2   AAPL    2.0   True\n",
        "3           1.5  False\n",
        "4    IBM    4.0   True\n",
        "True\n",
        "  tag\n",
        "0   b\n",
        "1   a\n",
        "2    \n",
        "3   c\n",
        "0 10 βeta\n",
    ];
    let (code, stdout, report) = run("text-columns-distinct", DISTINCT).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that joins frames with text columns: on a text key from either side, with
/// keys that have several partners, none, an empty text and a letter outside ASCII; on a
/// text key and a key of numbers in both orders; and on a key of numbers with text columns
/// carried along from both frames.
const JOINED: &str = r#"
def main():
    trades = DataFrame({
        "symbol": Array(["IBM", "AAPL", "", "IBM", "βeta", "ibm", "AAPL"]),
        "price": Array([3.0, 2.0, 1.0, 4.0, 5.0, 6.0, 7.0]),
        "day": Array([1, 1, 2, 2, 1, 1, 2]),
    })
    names = DataFrame({
        "symbol": Array(["AAPL", "IBM", "βeta", "MSFT", "IBM"]),
        "name": Array(["Apple", "International Business Machines", "Beta", "Microsoft", "Big Blue"]),
    })
    print(trades.merge(names, on="symbol"))
    print(names.merge(trades, on="symbol"))
    limits = DataFrame({
        "day": Array([2, 1, 2]),
        "symbol": Array(["IBM", "AAPL", "AAPL"]),
        "limit": Array([10, 20, 30]),
    })
    print(trades.merge(limits, on=["symbol", "day"]))
    print(trades.merge(limits, on=["day", "symbol"], how="inner"))
    print(trades.merge(limits, on="day").head(4))
    sizes = DataFrame({"day": Array([1, 2]), "venue": Array(["NYSE", ""])})
    print(sizes.merge(limits, on="day"))
    print(len(names.head(0).merge(trades, on="symbol")), len(trades), names.name[4])
"#;

/// `merge` joins on text as on numbers and carries text columns into the result: every
/// frame printed holds the rows pandas 3 gives, in the order of the left frame's rows and,
/// for one of them, of its partners in the right frame.
#[test]
fn frames_with_a_text_column_are_joined() {
    let expected = [
        "  symbol  price  day                             name\n",
        "0    IBM    3.0    1  International Business Machines\n",
        "1    IBM    3.0    1                         Big Blue\n",
        "2   AAPL    2.0    1                            Apple\n",
        "3    IBM    4.0    2  International Business Machines\n",
        "4    IBM    4.0    2                         Big Blue\n",
        "5   βeta    5.0    1                             Beta\n",
        "6   AAPL    7.0    2                            Apple\n",
        "  symbol                             name  price  day\n",
        "0   AAPL                            Apple    2.0    1\n",
        "1   AAPL                            Apple    7.0    2\n",
        "2    IBM  International Business Machines    3.0    1\n",
        "3    IBM  International Business Machines    4.0    2\n",
        "4   βeta                             Beta    5.0    1\n",
        "5    IBM                         Big Blue    3.0    1\n",
        "6    IBM                         Big Blue    4.0    2\n",
        "  symbol  price  day  limit\n",
        "0   AAPL    2.0    1     20\n",
        "1    IBM    4.0    2     10\n",
        "2   AAPL    7.0    2     30\n",
        "  symbol  price  day  limit\n",
        "0   AAPL    2.0    1     20\n",
        "1    IBM    4.0    2     10\n",
        "2   AAPL    7.0    2     30\n",
        "  symbol_x  price  day symbol_y  limit\n",
        "0      IBM    3.0    1     AAPL     20\n",
        "1     AAPL    2.0    1     AAPL     20\n",
        "2             1.0    2      IBM     10\n",
        "3             1.0    2     AAPL     30\n",
        "   day venue symbol  limit\n",
        "0    1  NYSE   AAPL     20\n",
        "1    2          IBM     10\n",
        "2    2         AAPL     30\n",
        "0 7 Big Blue\n",
    ];
    let (code, stdout, report) = run("text-columns-joined", JOINED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that groups frames by text columns: by one, by a text key and a key of
/// numbers in both orders, by two text keys, and by a key of numbers with text columns
/// beside it, with texts that differ only in case, an empty text and a letter outside ASCII.
const GROUPED: &str = r#"
def main():
    trades = DataFrame({
        "symbol": Array(["IBM", "AAPL", "", "IBM", "βeta", "ibm", "AAPL", "IBM"]),
        "price": Array([3.0, 2.0, 1.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
        "day": Array([1, 1, 2, 2, 1, 1, 2, 1]),
        "venue": Array(["N", "Q", "N", "N", "Q", "N", "Q", "N"]),
    })
    print(trades.groupby("symbol").agg(total=("price", "sum"), trades=("day", "count")))
    print(trades.groupby(["symbol", "day"]).agg(high=("price", "max")))
    print(trades.groupby(["day", "symbol"]).agg(mean=("price", "mean"), first=("price", "first")))
    print(trades.groupby(["venue", "symbol"]).agg(low=("price", "min")))
    print(trades.groupby("day").agg(total=("price", "sum")))
    by = trades.groupby(by="venue").agg(days=("day", "sum"), spread=("price", "std"))
    print(by.venue[0], len(by))
    print(len(trades.head(0).groupby("symbol").agg(n=("day", "count"))), trades.symbol[4])
"#;

/// `groupby` groups rows by text as by numbers, the groups in the order of the text: every
/// frame printed is what pandas 3 prints with the keys as columns.
#[test]
fn a_frame_is_grouped_by_a_text_column() {
    let expected = [
        "  symbol  total  trades\n",
        "0           1.0       1\n",
        "1   AAPL    9.0       2\n",
        "2    IBM   15.0       3\n",
        "3    ibm    6.0       1\n",
        "4   βeta    5.0       1\n",
        "  symbol  day  high\n",
        "0           2   1.0\n",
        "1   AAPL    1   2.0\n",
        "2   AAPL    2   7.0\n",
        "3    IBM    1   8.0\n",
        "4    IBM    2   4.0\n",
        "5    ibm    1   6.0\n",
        "6   βeta    1   5.0\n",
        "   day symbol  mean  first\n",
        "0    1   AAPL   2.0    2.0\n",
        "1    1    IBM   5.5    3.0\n",
        "2    1    ibm   6.0    6.0\n",
        "3    1   βeta   5.0    5.0\n",
        "4    2          1.0    1.0\n",
        "5    2   AAPL   7.0    7.0\n",
        "6    2    IBM   4.0    4.0\n",
        "  venue symbol  low\n",
        "0     N         1.0\n",
        "1     N    IBM  3.0\n",
        "2     N    ibm  6.0\n",
        "3     Q   AAPL  2.0\n",
        "4     Q   βeta  5.0\n",
        "   day  total\n",
        "0    1   24.0\n",
        "1    2   12.0\n",
        "N 2\n",
        "0 βeta\n",
    ];
    let (code, stdout, report) = run("text-columns-grouped", GROUPED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that reduces text columns with `agg`: every reduction text has beside a sum
/// of numbers, a key column of text reduced when grouped by another, two keys, a key with
/// a NaN, whose row is in no group, an empty text, a letter outside ASCII, and no rows.
const REDUCED: &str = r#"
from math import nan


def main():
    trades = DataFrame({
        "symbol": Array(["b", "a", "b", "a", "c", "b", "d", "d"]),
        "venue": Array(["x", "q", "a", "", "m", "zz", "é", "e"]),
        "price": Array([2.0, 1.0, 2.5, 1.0, 5.0, nan, 7.0, 7.0]),
        "day": Array([1, 1, 1, 2, 2, 2, 3, 3]),
    })
    print(trades.groupby("symbol").agg(
        n=("venue", "count"),
        first=("venue", "first"),
        last=("venue", "last"),
        low=("venue", "min"),
        high=("venue", "max"),
        total=("price", "sum"),
    ))
    print(trades.groupby("day").agg(low=("symbol", "min"), high=("venue", "max")))
    print(trades.groupby(["day", "symbol"]).agg(v=("venue", "last"), n=("venue", "count")))
    print(trades.groupby("price").agg(n=("symbol", "count"), s=("symbol", "max")))
    empty = trades.head(0).groupby("symbol").agg(v=("venue", "min"), n=("venue", "count"))
    print(len(empty), len(trades))
"#;

/// `agg` reduces a column of `str` in each group: `count` counts its texts, `first` and
/// `last` give the group's first and last, and `min` and `max` the texts that come first
/// and last by `<`, the empty text before every other and `"é"` after `"zz"`, as pandas
/// gives them. The program leaves no memory behind (checked under Valgrind when written).
#[test]
fn text_columns_are_reduced_in_groups() {
    let expected = [
        "  symbol  n first last low high  total\n",
        "0      a  2     q             q    2.0\n",
        "1      b  3     x   zz   a   zz    4.5\n",
        "2      c  1     m    m   m    m    5.0\n",
        "3      d  2     é    e   e    é   14.0\n",
        "   day low high\n",
        "0    1   a    x\n",
        "1    2   a   zz\n",
        "2    3   d    é\n",
        "   day symbol   v  n\n",
        "0    1      a   q  1\n",
        "1    1      b   a  2\n",
        "2    2      a      1\n",
        "3    2      b  zz  1\n",
        "4    2      c   m  1\n",
        "5    3      d   e  2\n",
        "   price  n  s\n",
        "0    1.0  2  a\n",
        "1    2.0  1  b\n",
        "2    2.5  1  b\n",
        "3    5.0  1  c\n",
        "4    7.0  2  d\n",
        "0 8\n",
    ];
    let (code, stdout, report) = run("text-columns-reduced", REDUCED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}
