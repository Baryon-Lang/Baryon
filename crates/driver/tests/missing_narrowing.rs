//! `dropna` and `fillna` (roadmap 3.13.6, decision 0045), through every stage up to the type
//! checker: the narrowed types they give an array with missing values and a frame, and the
//! mistakes in their arguments that are reported.
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

/// A program with one mistake in a call of `dropna` or `fillna` on each line.
const MISTAKES: &str = r#"
def uses(bids: Array[f64 | None, 1], sizes: Array[i64 | None, 1], prices: Array[f64, 1]):
    kept = bids.dropna(0)
    filled = bids.fillna()
    wrong = bids.fillna("none")
    both = bids.fillna(0.0, 1.0)
    df = DataFrame({"bid": bids.copy(), "size": sizes.copy(), "price": prices.copy()})
    rows = df.dropna(subset=["bid"])
    mixed = df.fillna(0.0)
    plain = df.fillna({"price": 1.0})
    unknown = df.fillna({"ask": 1.0})
    text = df.fillna({"bid": "none"})
    none = df.fillna()
    head = df.fillna({"bid": 0.0}).describe()
"#;

/// What the checker reports for [`MISTAKES`], in source order.
const EXPECTED: &str = r#"error: `dropna` of an array takes no arguments
 --> main.byn:2:12
  |
2 |     kept = bids.dropna(0)
  |            ^^^^^^^^^^^^^^ wrong arguments
  = note: `values.dropna()` gives the elements that are not `None`

error: `fillna` takes the value that replaces each `None`
 --> main.byn:3:14
  |
3 |     filled = bids.fillna()
  |              ^^^^^^^^^^^^^ wrong arguments
  = note: for example `values.fillna(0.0)`

error: expected `f64`, found `str`
 --> main.byn:4:25
  |
4 |     wrong = bids.fillna("none")
  |                         ^^^^^^ the array holds `f64`

error: `fillna` takes the value that replaces each `None`
 --> main.byn:5:12
  |
5 |     both = bids.fillna(0.0, 1.0)
  |            ^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `values.fillna(0.0)`

error: `dropna` of a `DataFrame` takes no arguments yet
 --> main.byn:7:12
  |
7 |     rows = df.dropna(subset=["bid"])
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: `df.dropna()` drops each row with a `None` in any column; to look at some columns only, take them first: `df[["bid", "ask"]].dropna()`

error: expected `i64`, found `f64`
 --> main.byn:8:23
  |
8 |     mixed = df.fillna(0.0)
  |                       ^^^ a column holds `i64`
  = note: one value fills every column that may hold `None`; give each column its own with a dict, `df.fillna({"bid": 0.0, "size": 0})`

error: the column `price` cannot hold `None`
 --> main.byn:9:24
  |
9 |     plain = df.fillna({"price": 1.0})
  |                        ^^^^^^^ has nothing to fill

error: `DataFrame[bid: f64 | None, size: i64 | None, price: f64]` has no column `ask`
  --> main.byn:10:26
   |
10 |     unknown = df.fillna({"ask": 1.0})
   |                          ^^^^^ not a column
   = note: its columns are `bid`, `size`, `price`

error: expected `f64`, found `str`
  --> main.byn:11:30
   |
11 |     text = df.fillna({"bid": "none"})
   |                              ^^^^^^ the column holds `f64`

error: `fillna` takes the value that replaces each `None`
  --> main.byn:12:12
   |
12 |     none = df.fillna()
   |            ^^^^^^^^^^^ wrong arguments
   = note: for example `values.fillna(0.0)`"#;

/// A program that drops and fills missing values only in the ways that are built.
const BUILT: &str = r#"
def fine(bids: Array[f64 | None, 1], sizes: Array[i64 | None, 1], prices: Array[f64, 1]) -> f64:
    df = DataFrame({"bid": bids.copy(), "size": sizes.copy(), "price": prices.copy()})
    whole = df.dropna()
    filled = df.fillna({"bid": 0.0, "size": 0}).head(2)
    part = df[["bid", "price"]].fillna(value=0.0)
    same = whole.dropna().fillna(1)
    return bids.dropna().sum() + bids.fillna(0.0).max() + filled.bid.sum() + part.bid.mean()
"#;

/// Arguments `dropna` does not take, a missing or extra value for `fillna`, a value of
/// another type than the array or the column it fills, one value for columns of two types,
/// a dict that names a column that cannot hold `None` or is not in the frame, and a method
/// not built yet on a frame that still has a column that may hold `None` after a partial
/// fill are each reported once, where they are.
#[test]
fn mistakes_in_dropna_and_fillna_are_reported() {
    assert_eq!(check_one(MISTAKES), EXPECTED);
}

/// The results are narrowed: `dropna()` and `fillna(value)` of an array give an array of
/// numbers that sums, and of a frame a frame whose filled columns are numbers, so `head`
/// and the column reductions take it. Both leave a frame with no column to narrow as it is.
#[test]
fn dropna_and_fillna_narrow_the_type() {
    assert_eq!(check_one(BUILT), "");
}
