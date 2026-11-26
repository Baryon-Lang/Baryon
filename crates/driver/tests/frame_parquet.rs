//! Checking `DataFrame.read_parquet` and `to_parquet` (decision 0045, roadmap 3.14.6.3 and
//! 3.14.6.4), end to end: programs that call them go through the parser, the syntax check,
//! the resolver and the checker, and the rendered diagnostics are compared in full.

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

/// A call that is right checks with no diagnostic: the schema from an annotation, a
/// parameter's type or a return type, with every type of column the reader has, in a function
/// that lets both errors pass on, and in a `try` that catches them.
#[test]
fn reading_a_file_checks() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None
    volume: i64
    note: str | None
    lots: u16 | None
    fee: f32
    live: bool
    filled: bool | None


def load(path: str) -> Raises[DataFrame[Trade], OSError | ValueError]:
    return DataFrame.read_parquet(path)


def count(trades: DataFrame[Trade]) -> int:
    return len(trades)


def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")
    print(count(DataFrame.read_parquet("more.parquet")))
    try:
        again: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")
        print(len(again), len(trades))
    except OSError as error:
        print(error)
    except ValueError as error:
        print(error)
"#;
    assert_eq!(check_one(program), "");
}

/// A keyword, a call with no schema, a column type the reader does not have, a call with no
/// path, a path that is not text and a call whose errors are not handled are each one
/// diagnostic that says what to write.
#[test]
fn a_wrong_call_is_an_error() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64


class Wide(Struct):
    symbol: str
    price: complex128


def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet", columns=["price"])
    untyped = DataFrame.read_parquet("trades.parquet")
    wide: DataFrame[Wide] = DataFrame.read_parquet("trades.parquet")
    none: DataFrame[Trade] = DataFrame.read_parquet()
    number: DataFrame[Trade] = DataFrame.read_parquet(3)


def unhandled():
    trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")
"#;
    let expected = r#"error: `DataFrame.read_parquet` does not take `columns` yet
  --> main.byn:12:73
   |
12 |     trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet", columns=["price"])
   |                                                                         ^^^^^^^ not built yet
   = note: this version takes the path alone, and the schema chooses the columns

error: `DataFrame.read_parquet` needs the frame's type from the context
  --> main.byn:13:15
   |
13 |     untyped = DataFrame.read_parquet("trades.parquet")
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ which columns, of which types?
   = note: a compiled program must know the columns before it reads the file: write the schema as a struct and annotate the variable, `trades: DataFrame[Trade] = DataFrame.read_parquet(path)`

error: `DataFrame.read_parquet` cannot read the column `price` of type `complex128` yet
  --> main.byn:14:29
   |
14 |     wide: DataFrame[Wide] = DataFrame.read_parquet("trades.parquet")
   |                             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built yet
   = note: a column of floats, of whole numbers, of `bool` or of `str` is read, and one of those that may be `None`

error: `DataFrame.read_parquet` takes the path of the file
  --> main.byn:15:30
   |
15 |     none: DataFrame[Trade] = DataFrame.read_parquet()
   |                              ^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")`

error: expected `str`, found `i64`
  --> main.byn:16:55
   |
16 |     number: DataFrame[Trade] = DataFrame.read_parquet(3)
   |                                                       ^ a path is `str`

error: `DataFrame.read_parquet` can fail with `OSError`, which is not handled here
  --> main.byn:20:32
   |
20 |     trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")
   |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`

error: `DataFrame.read_parquet` can fail with `ValueError`, which is not handled here
  --> main.byn:20:32
   |
20 |     trades: DataFrame[Trade] = DataFrame.read_parquet("trades.parquet")
   |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `ValueError`
   = note: catch it with `try:` and `except ValueError:`, or let it pass on by declaring `-> Raises[None, ValueError]`"#;
    assert_eq!(check_one(program), expected);
}

/// A frame is written in a function that lets `OSError` pass on and in a `try` that catches
/// it, with no diagnostic.
#[test]
fn writing_a_file_checks() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


def save(trades: DataFrame[Trade]) -> Raises[None, OSError]:
    trades.to_parquet("trades.parquet")


def main():
    trades = DataFrame({"symbol": Array(["A", "B"]), "price": Array([1.5, None])})
    try:
        trades.to_parquet("trades.parquet")
        save(trades)
    except OSError as error:
        print(error)
"#;
    assert_eq!(check_one(program), "");
}

/// A keyword, a call with no path, a path that is not text, a column type the writer does
/// not have and an `OSError` that is not handled are each one diagnostic.
#[test]
fn a_wrong_write_is_an_error() {
    let program = r#"
def main() -> Raises[None, OSError]:
    trades = DataFrame({"symbol": Array(["A", "B"]), "price": Array([1.5, 2.5])})
    trades.to_parquet("trades.parquet", compression="gzip")
    trades.to_parquet()
    trades.to_parquet(3)
    waves = DataFrame({"z": Array([1.0 + 2.0j])})
    waves.to_parquet("waves.parquet")


def unhandled():
    trades = DataFrame({"price": Array([1.5, 2.5])})
    trades.to_parquet("trades.parquet")
"#;
    let expected = r#"error: `to_parquet` does not take `compression` yet
 --> main.byn:3:41
  |
3 |     trades.to_parquet("trades.parquet", compression="gzip")
  |                                         ^^^^^^^^^^^ not built yet
  = note: this version takes the path alone, and compresses with Snappy

error: `to_parquet` takes the path of the file to write
 --> main.byn:4:5
  |
4 |     trades.to_parquet()
  |     ^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `trades.to_parquet("trades.parquet")`

error: expected `str`, found `i64`
 --> main.byn:5:23
  |
5 |     trades.to_parquet(3)
  |                       ^ a path is `str`

error: `to_parquet` cannot write the column `z` of type `complex128` yet
 --> main.byn:7:5
  |
7 |     waves.to_parquet("waves.parquet")
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built yet
  = note: a column of floats, of whole numbers, of `bool` or of `str` is written, and one of those that may be `None`

error: `trades.to_parquet` can fail with `OSError`, which is not handled here
  --> main.byn:12:5
   |
12 |     trades.to_parquet("trades.parquet")
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#;
    assert_eq!(check_one(program), expected);
}
