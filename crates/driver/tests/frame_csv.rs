//! Checking `DataFrame.read_csv` and `to_csv` (decision 0045, roadmap 3.14.4), end to end:
//! programs that call them go through the parser, the syntax check, the resolver and the
//! checker, and the rendered diagnostics are compared in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
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

/// A call that is right checks with no diagnostic: the schema from an annotation, with every
/// type of column the reader has, in a function that lets both errors pass on, and in a `try`
/// that catches them.
#[test]
fn reading_a_file_checks() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None
    volume: i64
    note: str | None
    lots: i64 | None
    fee: f64
    live: bool
    filled: bool | None


def load(path: str) -> Raises[DataFrame[Trade], OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv(path)
    return trades


def main():
    try:
        trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
        print(len(trades))
    except OSError as error:
        print(error.filename)
    except ValueError as error:
        print(error.message)
"#;
    assert_eq!(check_one(program), "");
}

/// A keyword of pandas that is not built, no type from the context, a column type the reader
/// does not have, no path, a path that is not text, a function `DataFrame` does not have, and
/// errors that are not handled: each is named.
#[test]
fn what_is_not_built_is_named() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


class Wide(Struct):
    symbol: str
    price: complex128


def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep=";")
    untyped = DataFrame.read_csv("trades.csv")
    wide: DataFrame[Wide] = DataFrame.read_csv("trades.csv")
    none: DataFrame[Trade] = DataFrame.read_csv()
    number: DataFrame[Trade] = DataFrame.read_csv(3)
    other: DataFrame[Trade] = DataFrame.read_table("trades.csv")


def unhandled():
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
"#;
    let expected = r#"error: `DataFrame.read_csv` does not take `sep` yet
  --> main.byn:12:65
   |
12 |     trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep=";")
   |                                                                 ^^^ not built yet
   = note: this version reads a comma-separated file with a header row, given its path alone

error: `DataFrame.read_csv` needs the frame's type from the context
  --> main.byn:13:15
   |
13 |     untyped = DataFrame.read_csv("trades.csv")
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ which columns, of which types?
   = note: a compiled program cannot learn the columns from the file: write the schema as a struct and annotate the variable, `trades: DataFrame[Trade] = DataFrame.read_csv(path)`

error: `DataFrame.read_csv` cannot read the column `price` of type `complex128` yet
  --> main.byn:14:29
   |
14 |     wide: DataFrame[Wide] = DataFrame.read_csv("trades.csv")
   |                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built yet
   = note: a column of floats, of whole numbers, of `bool` or of `str` is read, and one of those that may be `None`

error: `DataFrame.read_csv` takes the path of the file
  --> main.byn:15:30
   |
15 |     none: DataFrame[Trade] = DataFrame.read_csv()
   |                              ^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")`

error: expected `str`, found `i64`
  --> main.byn:16:51
   |
16 |     number: DataFrame[Trade] = DataFrame.read_csv(3)
   |                                                   ^ a path is `str`

error: `DataFrame` has no function `read_table`
  --> main.byn:17:31
   |
17 |     other: DataFrame[Trade] = DataFrame.read_table("trades.csv")
   |                               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not defined
   = note: a frame is made with `DataFrame({...})`, `DataFrame(rows)` or `DataFrame.read_csv(path)`

error: `DataFrame.read_csv` can fail with `OSError`, which is not handled here
  --> main.byn:21:32
   |
21 |     trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
   |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `DataFrame.read_csv` can fail with `ValueError`, which is not handled here
  --> main.byn:21:32
   |
21 |     trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
   |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `ValueError`
   = note: catch it with `try:` and `except ValueError:`, or let it pass on by declaring `-> Raises[None, ValueError]`"#;
    assert_eq!(check_one(program), expected);
}

/// `to_csv` checks with a path alone, in a function that lets `OSError` pass on. A keyword of
/// pandas that is not built, no path, a path that is not text, a column type the writer does
/// not have and an `OSError` that is not handled are each named.
#[test]
fn writing_a_file_checks() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


class Sample(Struct):
    width: complex128
    count: i64


def save(trades: DataFrame[Trade], samples: DataFrame[Sample]) -> Raises[None, OSError]:
    trades.to_csv("trades.csv")
    trades.to_csv("trades.csv", index=False)
    trades.to_csv()
    trades.to_csv(3)
    samples.to_csv("samples.csv")


def main():
    trades = DataFrame({"symbol": Array(["A"]), "price": Array([1.5, None])})
    trades.to_csv("trades.csv")
"#;
    let expected = r#"error: `to_csv` does not take `index` yet
  --> main.byn:13:33
   |
13 |     trades.to_csv("trades.csv", index=False)
   |                                 ^^^^^ not built yet
   = note: this version writes a comma-separated file with a header row and no index column, given its path alone

error: `to_csv` takes the path of the file to write
  --> main.byn:14:5
   |
14 |     trades.to_csv()
   |     ^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `trades.to_csv("trades.csv")`

error: expected `str`, found `i64`
  --> main.byn:15:19
   |
15 |     trades.to_csv(3)
   |                   ^ a path is `str`

error: `to_csv` cannot write the column `width` of type `complex128` yet
  --> main.byn:16:5
   |
16 |     samples.to_csv("samples.csv")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built yet
   = note: a column of floats, of whole numbers, of `bool` or of `str` is written, and one of those that may be `None`

error: `trades.to_csv` can fail with `OSError`, which is not handled here
  --> main.byn:21:5
   |
21 |     trades.to_csv("trades.csv")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#;
    assert_eq!(check_one(program), expected);
}
