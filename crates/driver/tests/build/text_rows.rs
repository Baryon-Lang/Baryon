//! Rows of a `DataFrame` with a text column (roadmap 3.13.3, decision 0045), built and run:
//! one row read as the schema's struct, the rows walked by `itertuples`, and a frame made
//! from a list of rows. A field of `str` owns a copy of its text, so the programs keep rows
//! past a change of the frame.

use super::text_columns::run;

/// A program that reads rows of a frame with a text column: kept in variables and a list,
/// passed, returned, read from the end, and still whole after the frame's column is
/// replaced. Its last line reads a row past the end.
const ROWS: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64


def show(trade: Trade):
    print(trade.symbol, trade.price)


def last(trades: DataFrame[Trade]) -> Trade:
    return trades[-1]


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array(["IBM", "", "βeta"]),
        "price": Array([3.0, 1.0, 2.0]),
    })
    first = trades[0]
    print(first.symbol, first.price)
    print(trades[1].symbol == "", trades[2].symbol)
    show(trades[1])
    show(last(trades))
    kept = [trades[2], trades[0]]
    at = 1
    middle = trades[at]
    trades.symbol = Array(["a", "b", "c"])
    print(kept[0].symbol, kept[1].symbol, middle.price, first.symbol, trades[0].symbol)
    name = trades[2].symbol.copy()
    print(name, len(trades[-3].symbol))
    print(trades[3].price)
"#;

/// `df[i]` on a frame with a text column gives a struct whose `str` field is its own copy of
/// the text, and a row past the end panics as an array index does.
#[test]
fn rows_of_a_frame_with_a_text_column_are_read() {
    let expected = [
        "IBM 3.0\n",
        "True βeta\n",
        " 1.0\n",
        "βeta 2.0\n",
        "βeta IBM 1.0 IBM a\n",
        "c 1\n",
    ];
    let (code, stdout, report) = run("text-rows-read", ROWS).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: index 3 is out of range for dimension 0 of length 3\n  --> main.byn:32:18\n"
    );
}

/// A program that walks the rows of a frame with a text column: forward, with `enumerate`
/// and `break`, backward with `continue`, beside a list with `zip`, over no rows, one loop
/// inside another, and out of a function from inside the loop.
const WALKED: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64


def main():
    trades: DataFrame[Trade] = DataFrame({
        "symbol": Array(["IBM", "", "βeta", "AAPL"]),
        "price": Array([3.0, 1.0, 2.0, 4.0]),
    })
    for trade in trades.itertuples():
        print(trade.symbol, trade.price)
    kept: list[Trade] = []
    names: list[str] = []
    for at, trade in enumerate(trades.itertuples()):
        if trade.price > 1.5:
            names.append(trade.symbol.copy())
            kept.append(Trade(trade.symbol.copy(), trade.price))
        if at == 2:
            break
    print(names, len(kept), kept[0].symbol)
    for trade in reversed(trades.itertuples()):
        if trade.symbol == "":
            continue
        print(trade.symbol)
    total = 0
    for trade, weight in zip(trades.itertuples(), [1, 2, 3, 4]):
        total += len(trade.symbol) * weight
    print(total)
    for trade in trades.head(0).itertuples():
        print("never")
    for outer in trades.itertuples():
        for inner in trades.itertuples():
            if inner.symbol == outer.symbol:
                print(inner.symbol, outer.price)
                break
    print(first_big(trades))


def first_big(trades: DataFrame[Trade]) -> str:
    for trade in trades.itertuples():
        if trade.price > 2.5:
            return trade.symbol.copy()
    return "none"
"#;

/// `itertuples` on a frame with a text column gives each row as a new struct that owns its
/// text, whichever way the loop is left.
#[test]
fn rows_of_a_frame_with_a_text_column_are_walked() {
    let expected = [
        "IBM 3.0\n",
        " 1.0\n",
        "βeta 2.0\n",
        "AAPL 4.0\n",
        "['IBM', 'βeta'] 2 IBM\n",
        "AAPL\n",
        "βeta\n",
        "IBM\n",
        "31\n",
        "IBM 3.0\n",
        " 1.0\n",
        "βeta 2.0\n",
        "AAPL 4.0\n",
        "IBM\n",
    ];
    let (code, stdout, report) = run("text-rows-walked", WALKED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that makes frames from lists of rows with two fields of `str`: a list written
/// out, which then grows without changing the frame, a comprehension over `itertuples`, an
/// empty list, and the frames stacked, sorted and read by row.
const MADE: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64
    venue: str


def main():
    rows = [
        Trade("IBM", 3.0, "N"),
        Trade("", 1.0, ""),
        Trade("βeta", 2.0, "Q"),
    ]
    trades = DataFrame(rows)
    print(trades)
    rows.append(Trade("AAPL", 4.0, "Q"))
    print(len(trades), len(rows), rows[0].symbol, trades.venue[2])
    again = DataFrame([row for row in trades.itertuples() if row.price > 1.5])
    print(again)
    empty: list[Trade] = []
    none: DataFrame[Trade] = DataFrame(empty)
    print(len(none), len(none.symbol))
    more = concat([trades, DataFrame(rows)]).sort_values(["symbol", "price"])
    print(more.drop_duplicates())
    print(more[5].symbol, more[-1].venue)
"#;

/// `DataFrame(rows)` copies each `str` field of the rows into a text column, and leaves the
/// list as it was.
#[test]
fn a_frame_with_text_columns_is_made_from_rows() {
    let expected = [
        "  symbol  price venue\n",
        "0    IBM    3.0     N\n",
        "1           1.0      \n",
        "2   βeta    2.0     Q\n",
        "3 4 IBM Q\n",
        "  symbol  price venue\n",
        "0    IBM    3.0     N\n",
        "1   βeta    2.0     Q\n",
        "0 0\n",
        "  symbol  price venue\n",
        "0           1.0      \n",
        "1   AAPL    4.0     Q\n",
        "2    IBM    3.0     N\n",
        "3   βeta    2.0     Q\n",
        "βeta Q\n",
    ];
    let (code, stdout, report) = run("text-rows-made", MADE).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}
