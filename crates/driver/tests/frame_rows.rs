//! Choosing rows of a frame (roadmap 3.13.4, decision 0045), through every stage up to the
//! type checker: `df[mask]`, `df[i:j]`, `head` and `tail`, the type each gives, the mistakes
//! each reports, and the mistakes a loop over `itertuples` reports.
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

/// A mask, a slice, `head` and `tail` each give a frame of the type they read, which keeps
/// its struct for rows and can be assigned back to the frame's own variable.
#[test]
fn chosen_rows_have_the_type_of_the_frame() {
    let program = r"
class Trade(Struct):
    price: f64
    volume: i64


def chosen(trades: DataFrame[Trade], count: i64) -> DataFrame[Trade]:
    costly: DataFrame[Trade] = trades[trades.price > 100.0]
    middle: DataFrame[Trade] = costly[1:count:2]
    first: Trade = middle.head()[0]
    last: Trade = middle.tail(n=count)[-1]
    kept = trades.copy()
    kept = kept[kept.volume > 0]
    return kept.head(count).tail(3)
";
    assert_eq!(check_one(program), r"");
}

/// An index that chooses neither a column nor rows, `head` with the wrong arguments, and an
/// assignment to the rows a mask or a slice chooses are each reported.
#[test]
fn row_choosing_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def rows(trades: DataFrame[Trade], positions: Array[i64, 1], masks: Array[bool, 2]):
    a = trades[positions]
    b = trades[masks]
    c = trades[1.5:2]
    d = trades.head("three")
    e = trades.tail(1, 2)
    f = trades.head(rows=3)
    g = trades.sample(3)


def assigning(trades: InOut[DataFrame[Trade]], other: DataFrame[Trade]):
    trades[trades.price > 1.0] = other
    trades[1:2] = other
"#;
    assert_eq!(
        check_one(program),
        r#"error: a `DataFrame` is indexed by a column's name, a row's number, a mask or a slice, found `Array[i64, 1]`
 --> main.byn:7:16
  |
7 |     a = trades[positions]
  |                ^^^^^^^^^ does not choose a column or rows
  = note: write a column's name as a string literal, `df["price"]`, a row's number, `df[0]`, a mask, an `Array[bool, 1]` such as `df[df.price > 100.0]`, or a slice, `df[1:3]`

error: a `DataFrame` is indexed by a column's name, a row's number, a mask or a slice, found `Array[bool, 2]`
 --> main.byn:8:16
  |
8 |     b = trades[masks]
  |                ^^^^^ does not choose a column or rows
  = note: write a column's name as a string literal, `df["price"]`, a row's number, `df[0]`, a mask, an `Array[bool, 1]` such as `df[df.price > 100.0]`, or a slice, `df[1:3]`

error: an index must be an integer, found `f64`
 --> main.byn:9:16
  |
9 |     c = trades[1.5:2]
  |                ^^^ not an integer

error: `head` takes a number of rows, found `str`
  --> main.byn:10:21
   |
10 |     d = trades.head("three")
   |                     ^^^^^^^ not an integer

error: `tail` of a `DataFrame` takes the number of rows
  --> main.byn:11:9
   |
11 |     e = trades.tail(1, 2)
   |         ^^^^^^^^^^^^^^^^^ wrong arguments
   = note: write `df.tail()` for 5 rows, or `df.tail(10)`

error: `head` of a `DataFrame` takes the number of rows
  --> main.byn:12:9
   |
12 |     f = trades.head(rows=3)
   |         ^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: write `df.head()` for 5 rows, or `df.head(10)`

error: a `DataFrame` has no method `sample` yet
  --> main.byn:13:9
   |
13 |     g = trades.sample(3)
   |         ^^^^^^^^^^^^^^^^ not defined yet
   = note: a `DataFrame` has `copy`, `head`, `tail`, `itertuples`, `assign`, `drop`, `rename`, `sort_values`, `groupby(...).agg(...)`, `merge` and `drop_duplicates` today; the rest of decision 0045's methods come with roadmap items 3.13.6, 3.13.8 and 3.13.9

error: rows of a `DataFrame` cannot be assigned
  --> main.byn:17:5
   |
17 |     trades[trades.price > 1.0] = other
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^ whole rows
   = note: assign their values by column: `df.price[0] = 1.0`, or `df.price[mask] = 1.0` for the rows a mask chooses

error: rows of a `DataFrame` cannot be assigned
  --> main.byn:18:5
   |
18 |     trades[1:2] = other
   |     ^^^^^^^^^^^ whole rows
   = note: assign their values by column: `df.price[0] = 1.0`, or `df.price[mask] = 1.0` for the rows a mask chooses"#
    );
}

/// `itertuples` outside a `for` loop, with an argument, on a frame with no struct for its
/// rows, and a frame changed while a loop goes over its rows are each reported.
#[test]
fn row_loop_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def rows(trades: InOut[DataFrame[Trade]], prices: Array[f64, 1]):
    a = trades.itertuples()
    for row in trades.itertuples(index=False):
        print(row)
    plain = DataFrame({"price": prices.copy()})
    for row in plain.itertuples():
        print(row)
    for row in trades.itertuples():
        trades.price[0] = row.price
"#;
    assert_eq!(
        check_one(program),
        r"error: `itertuples` gives the rows of a `DataFrame` to a `for` loop
 --> main.byn:7:9
  |
7 |     a = trades.itertuples()
  |         ^^^^^^^^^^^^^^^^^^^ not looped over
  = note: write `for row in df.itertuples():`; to keep the rows, build a list, `[row for row in df.itertuples()]`

error: `itertuples` of a `DataFrame` takes no arguments
 --> main.byn:8:16
  |
8 |     for row in trades.itertuples(index=False):
  |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: each row is the struct of the frame's schema; a frame has no row index to leave out and its rows need no other name

error: no struct names the rows of `DataFrame[price: f64]`
  --> main.byn:11:16
   |
11 |     for row in plain.itertuples():
   |                ^^^^^^^^^^^^^^^^^^ a row needs a type
   = note: loop over the columns instead, `for price in df.price:`, or give the frame a schema: assign it to a variable declared `DataFrame[Trade]`, whose struct lists these columns

error: `trades` cannot be changed while a loop goes over it
  --> main.byn:14:9
   |
14 |         trades.price[0] = row.price
   |         ^^^^^^^^^^^^^^^ changed here
  ::: main.byn:13:16
   |
13 |     for row in trades.itertuples():
   |                ------ the loop goes over `trades` here
   = note: loop over the indices instead: `for i in range(len(trades)):`"
    );
}
