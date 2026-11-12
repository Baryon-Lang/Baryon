//! Frames (roadmap 3.13.2, decision 0045), through every stage up to the type checker: the type
//! `DataFrame[S]`, building a frame from arrays, and reading its columns by names the compiler
//! checks.
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

/// A frame built from arrays is the declared frame type when the columns match, whatever
/// names its rows; a column is an array of rank 1 of its element type; `shape` and `columns`
/// are the frame's own; and a frame nests in lists, tuples and `T | None`.
#[test]
fn frames_and_their_columns_have_types() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


class Quote(Struct):
    price: f64
    volume: i64


def total(trades: DataFrame[Trade]) -> f64:
    prices: Array[f64, 1] = trades.price.copy()
    volumes: Array[i64, 1] = trades["volume"].copy()
    rows: i64 = len(trades)
    shape: tuple[i64, i64] = trades.shape
    names: list[str] = trades.columns
    return (prices * volumes).sum()


def build(prices: Owned[Array[f64, 1]], volumes: Owned[Array[i64, 1]]) -> DataFrame[Trade]:
    quotes: DataFrame[Quote] = DataFrame({"price": prices, "volume": volumes})
    print(total(quotes))
    literal: DataFrame[Trade] = DataFrame({"price": Array([1, 2]), "volume": Array([3, 4])})
    several: list[DataFrame[Trade]] = [literal.copy()]
    maybe: DataFrame[Trade] | None = None
    pair: tuple[DataFrame[Quote], i64] = (literal, 2)
    return quotes
"#;
    assert_eq!(check_one(program), "");
}

/// A schema that is not a struct, that has a text field or that has no fields, a frame type
/// with no schema, and a frame in a struct field are errors.
#[test]
fn frame_types_that_cannot_be_are_reported() {
    let program = r"
class Trade(Struct):
    price: f64
    volume: i64


class Named(Struct):
    name: str
    price: f64


class Empty(Struct):
    pass


class Book(Struct):
    trades: list[DataFrame[Trade]]


def types(a: DataFrame[f64], b: DataFrame[Named], c: DataFrame[Empty], d: DataFrame):
    pass
";
    assert_eq!(
        check_one(program),
        r"error: a struct field cannot hold a `DataFrame` yet
  --> main.byn:16:13
   |
16 |     trades: list[DataFrame[Trade]]
   |             ^^^^^^^^^^^^^^^^^^^^^^ holds a frame
   = note: keep the frame in a variable and pass it beside the struct; frames in struct fields are roadmap idea I.49

error: a `DataFrame`'s schema is a struct that lists its columns, found `f64`
  --> main.byn:19:24
   |
19 | def types(a: DataFrame[f64], b: DataFrame[Named], c: DataFrame[Empty], d: DataFrame):
   |                        ^^^ not a struct
   = note: declare the columns as the fields of a struct, `class Trade(Struct):` with `price: f64` and `volume: i64`, then write `DataFrame[Trade]`

error: a column of a `DataFrame` cannot hold `str` yet
  --> main.byn:19:43
   |
19 | def types(a: DataFrame[f64], b: DataFrame[Named], c: DataFrame[Empty], d: DataFrame):
   |                                           ^^^^^ the field `name` would be such a column
   = note: a column holds numbers or `bool` today; text columns come with roadmap item 3.13.3 and missing values with 3.13.6

error: `Empty` has no fields, so a `DataFrame` of it would have no columns
  --> main.byn:19:64
   |
19 | def types(a: DataFrame[f64], b: DataFrame[Named], c: DataFrame[Empty], d: DataFrame):
   |                                                                ^^^^^ no columns

error: `DataFrame` needs type arguments
  --> main.byn:19:75
   |
19 | def types(a: DataFrame[f64], b: DataFrame[Named], c: DataFrame[Empty], d: DataFrame):
   |                                                                           ^^^^^^^^^ no type arguments
   = note: for example `DataFrame[Trade]`"
    );
}

/// A column the frame does not have, a name that is not a literal, a row by number, a column
/// moved out, a method not built yet and a frame moved out of a borrow are errors.
#[test]
fn column_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def columns(trades: DataFrame[Trade], name: str):
    a = trades.cost
    b = trades["cost"]
    c = trades[name]
    d = trades[0]
    e = trades.price
    f = [trades["volume"]]
    g = trades.head(2)
    h = trades.copy(True)
    i = trades
"#;
    assert_eq!(
        check_one(program),
        r#"error: `DataFrame[Trade]` has no column `cost`
 --> main.byn:7:9
  |
7 |     a = trades.cost
  |         ^^^^^^^^^^^ not a column
  = note: its columns are `price`, `volume`

error: `DataFrame[Trade]` has no column `cost`
 --> main.byn:8:16
  |
8 |     b = trades["cost"]
  |                ^^^^^^ not a column
  = note: its columns are `price`, `volume`

error: a `DataFrame` is indexed by a column's name, found `str`
 --> main.byn:9:16
  |
9 |     c = trades[name]
  |                ^^^^ not a column's name
  = note: write the name as a string literal, `df["price"]`; rows by number, masks and slices come with roadmap items 3.13.2 and 3.13.4

error: a `DataFrame` is indexed by a column's name, found `i64`
  --> main.byn:10:16
   |
10 |     d = trades[0]
   |                ^ not a column's name
   = note: write the name as a string literal, `df["price"]`; rows by number, masks and slices come with roadmap items 3.13.2 and 3.13.4

error: a column cannot be moved out of its `DataFrame`
  --> main.byn:11:9
   |
11 |     e = trades.price
   |         ^^^^^^^^^^^^ moved out of the frame
   = note: a column can be read, passed to a call or assigned to; to keep it on its own, copy it: `trades.price.copy()`

error: a column cannot be moved out of its `DataFrame`
  --> main.byn:12:10
   |
12 |     f = [trades["volume"]]
   |          ^^^^^^^^^^^^^^^^ moved out of the frame
   = note: a column can be read, passed to a call or assigned to; to keep it on its own, copy it: `trades["volume"].copy()`

error: a `DataFrame` has no method `head` yet
  --> main.byn:13:9
   |
13 |     g = trades.head(2)
   |         ^^^^^^^^^^^^^^ not defined yet
   = note: a `DataFrame` has `copy()` today; the rest of decision 0045's methods come with roadmap items 3.13.4 to 3.13.9

error: `copy` of a `DataFrame` takes no arguments
  --> main.byn:14:9
   |
14 |     h = trades.copy(True)
   |         ^^^^^^^^^^^^^^^^^ wrong arguments

error: `trades` is borrowed from the caller, so it cannot be moved
  --> main.byn:15:9
   |
15 |     i = trades
   |         ^^^^^^ moved here
   = note: move a copy instead: `trades.copy()`, or take the parameter as `Owned[...]`"#
    );
}

/// Assigning a column the frame does not have, an array of another element type, and `shape`
/// are errors: a frame's type never changes.
#[test]
fn assignments_that_would_change_a_frames_type_are_reported() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def assigning(trades: InOut[DataFrame[Trade]], flags: Owned[Array[bool, 1]]):
    trades["cost"] = trades.price * 2.0
    trades.price = flags
    trades.shape = (1, 2)
"#;
    assert_eq!(
        check_one(program),
        r#"error: `DataFrame[Trade]` has no column `cost`
 --> main.byn:7:12
  |
7 |     trades["cost"] = trades.price * 2.0
  |            ^^^^^^ not a column
  = note: its columns are `price`, `volume`

error: expected `Array[f64, 1]`, found `Array[bool, 1]`
 --> main.byn:8:20
  |
8 |     trades.price = flags
  |                    ^^^^^ the target holds `Array[f64, 1]`

error: `shape` of a `DataFrame` cannot be assigned
 --> main.byn:9:5
  |
9 |     trades.shape = (1, 2)
  |     ^^^^^^^^^^^^ computed from the frame
  = note: a frame's column names and their number are part of its type; `rename`, `assign` and `drop` give a new frame (roadmap 3.13.4)"#
    );
}

/// A frame built from something other than a dict of columns, with a name that is not a
/// literal, a column that is not an array of rank 1, a name twice or no columns, one that
/// does not match the declared type, and one that would move a borrowed array are errors.
#[test]
fn building_mistakes_are_reported() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def building(name: str, grid: Array[f64, 2], column: Array[f64, 1]):
    a = DataFrame(column.copy())
    b = DataFrame({name: column.copy()})
    c = DataFrame({"a": grid.copy(), "b": [1.0]})
    d = DataFrame({"a": column.copy(), "a": column.copy()})
    e = DataFrame({})
    f: DataFrame[Trade] = DataFrame({"price": column.copy()})
    g = DataFrame({"a": column})
"#;
    assert_eq!(
        check_one(program),
        r#"error: `DataFrame(...)` takes a dict of its columns
 --> main.byn:7:9
  |
7 |     a = DataFrame(column.copy())
  |         ^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `DataFrame({"price": prices, "volume": volumes})`

error: a column's name is written as a string literal
 --> main.byn:8:20
  |
8 |     b = DataFrame({name: column.copy()})
  |                    ^^^^ not a string literal
  = note: a `DataFrame`'s type lists its columns, so each name must be known when compiling

error: a column is an array of rank 1, found `Array[f64, 2]`
 --> main.byn:9:25
  |
9 |     c = DataFrame({"a": grid.copy(), "b": [1.0]})
  |                         ^^^^^^^^^^^ not a column

error: a column is an array of rank 1, found `list[f64]`
 --> main.byn:9:43
  |
9 |     c = DataFrame({"a": grid.copy(), "b": [1.0]})
  |                                           ^^^^^ not a column

error: the column `a` is given twice
  --> main.byn:10:40
   |
10 |     d = DataFrame({"a": column.copy(), "a": column.copy()})
   |                                        ^^^^^^^^^^^^^^^^^^ a second time

error: a `DataFrame` needs at least one column
  --> main.byn:11:19
   |
11 |     e = DataFrame({})
   |                   ^^ no columns

error: expected `DataFrame[Trade]`, found `DataFrame[price: f64]`
  --> main.byn:12:27
   |
12 |     f: DataFrame[Trade] = DataFrame({"price": column.copy()})
   |                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the annotation says `DataFrame[Trade]`

error: `column` is borrowed from the caller, so it cannot be moved
  --> main.byn:13:25
   |
13 |     g = DataFrame({"a": column})
   |                         ^^^^^^ moved here
   = note: move a copy instead: `column.copy()`, or take the parameter as `Owned[...]`"#
    );
}
