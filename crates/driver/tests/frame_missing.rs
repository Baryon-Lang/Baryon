//! Frame columns that may hold `None` (roadmap 3.13.6, decision 0045), through every stage
//! up to the type checker: the schema, building the frame from its columns, reading and
//! replacing a column, and the operations not built yet that it reports.
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

/// A program with one mistake, or one use of a frame with a column that may hold `None` that
/// is not built yet, on each line.
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
    made = DataFrame(rows)
    first = quotes[0]
    part = quotes.rename(columns={"bid": "b"})
    chosen = quotes.drop_duplicates()
    narrow = quotes.fillna({"price": 1.0})
    top = quotes.describe()
    full = quotes.dropna(how="all")
    order = quotes.assign(spread=quotes.price)
    groups = quotes.groupby("price").agg(count=("bid", "count"))
    joined = prices.merge(quotes, on="price")
    stacked = concat([quotes])
    for row in quotes.itertuples():
        pass
    print(quotes)
    text = f"{quotes}"
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

error: building it from a list of rows is not built for a frame with a column that may hold `None` yet
  --> main.byn:16:22
   |
16 |     made = DataFrame(rows)
   |                      ^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: reading one row is not built for a frame with a column that may hold `None` yet
  --> main.byn:17:20
   |
17 |     first = quotes[0]
   |                    ^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: the method `rename` is not built for a frame with a column that may hold `None` yet
  --> main.byn:18:12
   |
18 |     part = quotes.rename(columns={"bid": "b"})
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: the method `drop_duplicates` is not built for a frame with a column that may hold `None` yet
  --> main.byn:19:14
   |
19 |     chosen = quotes.drop_duplicates()
   |              ^^^^^^^^^^^^^^^^^^^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: the column `price` cannot hold `None`
  --> main.byn:20:29
   |
20 |     narrow = quotes.fillna({"price": 1.0})
   |                             ^^^^^^^ has nothing to fill

error: the method `describe` is not built for a frame with a column that may hold `None` yet
  --> main.byn:21:11
   |
21 |     top = quotes.describe()
   |           ^^^^^^^^^^^^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: `dropna` of a `DataFrame` takes no arguments yet
  --> main.byn:22:12
   |
22 |     full = quotes.dropna(how="all")
   |            ^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: `df.dropna()` drops each row with a `None` in any column; to look at some columns only, take them first: `df[["bid", "ask"]].dropna()`

error: the method `assign` is not built for a frame with a column that may hold `None` yet
  --> main.byn:23:13
   |
23 |     order = quotes.assign(spread=quotes.price)
   |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: `groupby` is not built for a frame with a column that may hold `None` yet
  --> main.byn:24:14
   |
24 |     groups = quotes.groupby("price").agg(count=("bid", "count"))
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: `merge` is not built for a frame with a column that may hold `None` yet
  --> main.byn:25:27
   |
25 |     joined = prices.merge(quotes, on="price")
   |                           ^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: `concat` is not built for a frame with a column that may hold `None` yet
  --> main.byn:26:23
   |
26 |     stacked = concat([quotes])
   |                       ^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: a loop over its rows is not built for a frame with a column that may hold `None` yet
  --> main.byn:27:16
   |
27 |     for row in quotes.itertuples():
   |                ^^^^^^^^^^^^^^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: showing it is not built for a frame with a column that may hold `None` yet
  --> main.byn:29:11
   |
29 |     print(quotes)
   |           ^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: showing it is not built for a frame with a column that may hold `None` yet
  --> main.byn:30:15
   |
30 |     text = f"{quotes}"
   |               ^^^^^^ a column may hold `None`
   = note: a frame with a column that may hold `None` is built from its columns, counted with `len`, copied with `copy`, narrowed by `dropna` and `fillna`, its rows chosen by a mask, a slice, `head` and `tail` and sorted with each `None` last, and its columns read, replaced and chosen by name; the rest comes with roadmap item 3.13.6

error: a column cannot be moved out of its `DataFrame`
  --> main.byn:31:12
   |
31 |     kept = quotes.bid
   |            ^^^^^^^^^^ moved out of the frame
   = note: a column can be read, passed to a call or assigned to; to keep it on its own, copy it: `quotes.bid.copy()`

error: list items must share one type, found `f64 | None` and `str`
  --> main.byn:32:79
   |
32 |     wrong: DataFrame[Quote] = DataFrame({"price": Array([1.0]), "bid": Array(["a"])})
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
