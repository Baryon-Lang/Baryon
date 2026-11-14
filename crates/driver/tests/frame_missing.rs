//! Frame columns that may hold `None` (roadmap 3.13.6, decision 0045), through every stage
//! up to the type checker: the schema, building the frame from its columns, reading and
//! replacing a column, and the mistakes it reports.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A program with one mistake in the use of a frame with a column that may hold `None` on
/// each line, or one use of such a column that is not built yet.
const MISTAKES: &str = r#"
class Quote(Struct):
    price: f64
    bid: f64 | None


class Named(Struct):
    name: str | None


class Price(Struct):
    price: f64


def uses(quotes: DataFrame[Quote], rows: list[Quote], prices: DataFrame[Price]):
    names: DataFrame[Named] = DataFrame({"name": Array(["a"])})
    first: f64 = quotes[0].bid
    narrow = quotes.fillna({"price": 1.0})
    full = quotes.dropna(how="all")
    order = quotes.bid.argsort()
    groups = quotes.groupby("price").agg(top=("bid", "top"))
    joined = prices.merge(quotes, on="bid")
    stacked = quotes.bid.std()
    for row in quotes.itertuples():
        total = row.bid + 1.0
    print((quotes.copy(), 1))
    kept = quotes.bid
    wrong: DataFrame[Quote] = DataFrame({"price": Array([1.0]), "bid": Array(["a"])})
"#;

/// What the checker reports for [`MISTAKES`], in source order.
const EXPECTED: &str = r#"error: a column of a `DataFrame` cannot hold `str | None` yet
  --> main.byn:15:22
   |
15 |     names: DataFrame[Named] = DataFrame({"name": Array(["a"])})
   |                      ^^^^^ the field `name` would be such a column
   = note: a column holds numbers, `bool` or `str` today, and numbers or `bool`s that may be `None`

error: this value may be `None`
  --> main.byn:16:18
   |
16 |     first: f64 = quotes[0].bid
   |                  ^^^^^^^^^^^^^ has type `f64 | None`
   = note: give it a name and check it first: `if value is not None:`

error: the column `price` cannot hold `None`
  --> main.byn:17:29
   |
17 |     narrow = quotes.fillna({"price": 1.0})
   |                             ^^^^^^^ has nothing to fill

error: `dropna` of a `DataFrame` takes no arguments yet
  --> main.byn:18:12
   |
18 |     full = quotes.dropna(how="all")
   |            ^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: `df.dropna()` drops each row with a `None` in any column; to look at some columns only, take them first: `df[["bid", "ask"]].dropna()`

error: the method `argsort` is not built for an array with missing values yet
  --> main.byn:19:13
   |
19 |     order = quotes.bid.argsort()
   |             ^^^^^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: `agg` has no reduction `top`
  --> main.byn:20:54
   |
20 |     groups = quotes.groupby("price").agg(top=("bid", "top"))
   |                                                      ^^^^^ not a reduction
   = note: it has `sum`, `min`, `max`, `first`, `last`, `count`, `mean`, `var`, `std`, `median`

error: `DataFrame[Price]` has no column `bid`
  --> main.byn:21:38
   |
21 |     joined = prices.merge(quotes, on="bid")
   |                                      ^^^^^ not a column
   = note: its columns are `price`

error: the method `std` is not built for an array with missing values yet
  --> main.byn:22:15
   |
22 |     stacked = quotes.bid.std()
   |               ^^^^^^^^^^^^^^^^ an array that may hold `None`
   = note: an array with missing values has `len`, `print`, `values[i]`, which gives a `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna`, `copy`, `count` and, for numbers, `sum`, `min`, `max` and `mean`; the rest comes with roadmap item 3.13.6

error: this value may be `None`
  --> main.byn:24:17
   |
24 |         total = row.bid + 1.0
   |                 ^^^^^^^ has type `f64 | None`
   = note: give it a name and check it first: `if value is not None:`

error: a value that holds an array with missing values cannot be shown yet
  --> main.byn:25:11
   |
25 |     print((quotes.copy(), 1))
   |           ^^^^^^^^^^^^^^^^^^ holds an array that may hold `None`
   = note: show the array or the frame alone: `print(values)`

error: a column cannot be moved out of its `DataFrame`
  --> main.byn:26:12
   |
26 |     kept = quotes.bid
   |            ^^^^^^^^^^ moved out of the frame
   = note: a column can be read, passed to a call or assigned to; to keep it on its own, copy it: `quotes.bid.copy()`

error: list items must share one type, found `f64 | None` and `str`
  --> main.byn:27:79
   |
27 |     wrong: DataFrame[Quote] = DataFrame({"price": Array([1.0]), "bid": Array(["a"])})
   |                                                                               ^^^ a different type"#;

/// A program that uses a frame with a column that may hold `None` only in the ways that are
/// built.
const BUILT: &str = r#"
class Quote(Struct):
    price: f64
    bid: f64 | None


def fine(quotes: DataFrame[Quote]) -> DataFrame[Quote]:
    count = len(quotes) + quotes.shape[1] + len(quotes.columns)
    bid = quotes.bid[0]
    there = quotes.bid.notna() | quotes["bid"].isna()
    twin = quotes.copy()
    twin.bid = Array([None, 2.0])
    narrow = quotes[["bid"]]
    rows = quotes[quotes.bid.notna()][1:].head(2).tail(1).sort_values("price")
    both = concat([rows, quotes.copy().assign(bid=Array([None, 1.0]))])
    wider = both.assign(ask=quotes.bid.copy()).drop(columns=["price"]).rename(columns={"ask": "a"})
    total = quotes.dropna().bid.sum() + quotes.fillna(0.0).bid.sum()
    return twin
"#;

/// Each mistake and each operation that is not built yet is reported once, where it is, with
/// what a frame with a column that may hold `None` has today.
#[test]
fn unbuilt_uses_of_a_frame_with_missing_values_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}

/// The schema, `len`, `shape`, `columns`, a column read as an array with missing values,
/// `copy`, a column replaced, columns chosen by name, rows chosen by a mask, a slice, `head`
/// and `tail` and sorted by a column of numbers, and `dropna` and `fillna`, whose columns are
/// numbers, check clean.
#[test]
fn the_built_uses_of_a_frame_with_missing_values_check_clean() {
    assert_eq!(check_one(BUILT), "");
}
