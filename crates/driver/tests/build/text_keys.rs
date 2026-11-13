//! Text columns of a `DataFrame` as keys (roadmap 3.13.3, decision 0045), built and run:
//! frames put in order by a column of `str`, and their repeated rows left out. The frames
//! printed are compared with what pandas 3 prints.

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
