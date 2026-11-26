//! Checking `DataFrame.read_csv` and `to_csv` (decision 0045, roadmap 3.14.4 and 3.14.4.2), end
//! to end:
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
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", usecols=["price"])
    untyped = DataFrame.read_csv("trades.csv")
    wide: DataFrame[Wide] = DataFrame.read_csv("trades.csv")
    none: DataFrame[Trade] = DataFrame.read_csv()
    number: DataFrame[Trade] = DataFrame.read_csv(3)
    other: DataFrame[Trade] = DataFrame.read_table("trades.csv")


def unhandled():
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
"#;
    let expected = r#"error: `DataFrame.read_csv` does not take `usecols` yet
  --> main.byn:12:65
   |
12 |     trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", usecols=["price"])
   |                                                                 ^^^^^^^ not built yet
   = note: this version takes the path, `sep=`, `na_values=`, `nrows=` and `skiprows=`, for a file with a header row

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
   = note: a frame is made with `DataFrame({...})`, `DataFrame(rows)`, `DataFrame.read_csv(path)` or `DataFrame.read_parquet(path)`

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
   = note: this version takes the path, `sep=`, `na_rep=`, `header=` and `columns=`, and writes no index column

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

/// `sep=` (roadmap 3.14.4.2) checks as a string literal of one character, a tab included, on
/// both functions. A text of several characters, an empty one, a quote, a character outside
/// ASCII, a variable and a number are each reported at the value.
#[test]
fn the_separator_is_one_character_written_out() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep=";")
    tabbed: DataFrame[Trade] = DataFrame.read_csv("trades.tsv", sep="\t")
    trades.to_csv("out.csv", sep="|")
    trades.to_csv("out.tsv", sep='\t')
    semicolon = ";"
    two: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep="::")
    none: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep="")
    named: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep=semicolon)
    trades.to_csv("out.csv", sep='"')
    trades.to_csv("out.csv", sep="§")
    trades.to_csv("out.csv", sep=9)
"#;
    let expected = r#"error: `sep` of `DataFrame.read_csv` is one character, written out
  --> main.byn:12:66
   |
12 |     two: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep="::")
   |                                                                  ^^^^ not one character
   = note: for example `sep=";"` or `sep="\t"`; a quote, a backslash and a character outside ASCII cannot split fields

error: `sep` of `DataFrame.read_csv` is one character, written out
  --> main.byn:13:67
   |
13 |     none: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep="")
   |                                                                   ^^ not one character
   = note: for example `sep=";"` or `sep="\t"`; a quote, a backslash and a character outside ASCII cannot split fields

error: `sep` of `DataFrame.read_csv` is one character, written out
  --> main.byn:14:68
   |
14 |     named: DataFrame[Trade] = DataFrame.read_csv("trades.csv", sep=semicolon)
   |                                                                    ^^^^^^^^^ not one character
   = note: for example `sep=";"` or `sep="\t"`; a quote, a backslash and a character outside ASCII cannot split fields

error: `sep` of `to_csv` is one character, written out
  --> main.byn:15:34
   |
15 |     trades.to_csv("out.csv", sep='"')
   |                                  ^^^ not one character
   = note: for example `sep=";"` or `sep="\t"`; a quote, a backslash and a character outside ASCII cannot split fields

error: `sep` of `to_csv` is one character, written out
  --> main.byn:16:34
   |
16 |     trades.to_csv("out.csv", sep="§")
   |                                  ^^^ not one character
   = note: for example `sep=";"` or `sep="\t"`; a quote, a backslash and a character outside ASCII cannot split fields

error: `sep` of `to_csv` is one character, written out
  --> main.byn:17:34
   |
17 |     trades.to_csv("out.csv", sep=9)
   |                                  ^ not one character
   = note: for example `sep=";"` or `sep="\t"`; a quote, a backslash and a character outside ASCII cannot split fields"#;
    assert_eq!(check_one(program), expected);
}

/// `na_values=` of `read_csv` checks as a `str` or a `list[str]`, written out or worked out,
/// and `na_rep=` of `to_csv` as a `str` (roadmap 3.14.4.2). A number, a list of numbers and
/// a dict are reported at the value, and each keyword on the other function is not built.
#[test]
fn the_spellings_of_a_missing_value_are_text() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    words = ["NA", "-"]
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values="NA", sep=";")
    listed: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values=["NA", "-999"])
    named: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values=words)
    trades.to_csv("out.csv", na_rep="NA")
    trades.to_csv("out.csv", sep=";", na_rep=words[1])
    number: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values=-999)
    numbers: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values=[-999, 0])
    each: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values={"price": "NA"})
    trades.to_csv("out.csv", na_rep=0)
    trades.to_csv("out.csv", na_values="NA")
    wrong: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_rep="NA")
"#;
    let expected = r#"error: expected `str`, found `i64`
  --> main.byn:13:75
   |
13 |     number: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values=-999)
   |                                                                           ^^^^ `na_values` is a `list[str]` or a `str`

error: expected `str`, found `list[i64]`
  --> main.byn:14:76
   |
14 |     numbers: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values=[-999, 0])
   |                                                                            ^^^^^^^^^ `na_values` is a `list[str]` or a `str`

error: expected `str`, found `dict[str, str]`
  --> main.byn:15:73
   |
15 |     each: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_values={"price": "NA"})
   |                                                                         ^^^^^^^^^^^^^^^ `na_values` is a `list[str]` or a `str`

error: expected `str`, found `i64`
  --> main.byn:16:37
   |
16 |     trades.to_csv("out.csv", na_rep=0)
   |                                     ^ `na_rep` is `str`

error: `to_csv` does not take `na_values` yet
  --> main.byn:17:30
   |
17 |     trades.to_csv("out.csv", na_values="NA")
   |                              ^^^^^^^^^ not built yet
   = note: this version takes the path, `sep=`, `na_rep=`, `header=` and `columns=`, and writes no index column

error: `DataFrame.read_csv` does not take `na_rep` yet
  --> main.byn:18:64
   |
18 |     wrong: DataFrame[Trade] = DataFrame.read_csv("trades.csv", na_rep="NA")
   |                                                                ^^^^^^ not built yet
   = note: this version takes the path, `sep=`, `na_values=`, `nrows=` and `skiprows=`, for a file with a header row"#;
    assert_eq!(check_one(program), expected);
}

/// `nrows=` and `skiprows=` of `read_csv` each check as an `int`, written out or worked out
/// (roadmap 3.14.4.2). A float, a list of lines to skip and a `bool` are reported at the
/// value, and `to_csv` has neither.
#[test]
fn the_rows_to_read_are_whole_numbers() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    count = 10
    trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv", nrows=100, skiprows=2)
    some: DataFrame[Trade] = DataFrame.read_csv("trades.csv", skiprows=count - 1, nrows=count)
    half: DataFrame[Trade] = DataFrame.read_csv("trades.csv", nrows=1.5)
    lines: DataFrame[Trade] = DataFrame.read_csv("trades.csv", skiprows=[0, 2])
    flag: DataFrame[Trade] = DataFrame.read_csv("trades.csv", nrows=True)
    trades.to_csv("out.csv", nrows=5)
"#;
    let expected = r#"error: expected `i64`, found `f64`
  --> main.byn:10:69
   |
10 |     half: DataFrame[Trade] = DataFrame.read_csv("trades.csv", nrows=1.5)
   |                                                                     ^^^ `nrows` is `i64`

error: expected `i64`, found `list[i64]`
  --> main.byn:11:73
   |
11 |     lines: DataFrame[Trade] = DataFrame.read_csv("trades.csv", skiprows=[0, 2])
   |                                                                         ^^^^^^ `skiprows` is `i64`

error: expected `i64`, found `bool`
  --> main.byn:12:69
   |
12 |     flag: DataFrame[Trade] = DataFrame.read_csv("trades.csv", nrows=True)
   |                                                                     ^^^^ `nrows` is `i64`

error: `to_csv` does not take `nrows` yet
  --> main.byn:13:30
   |
13 |     trades.to_csv("out.csv", nrows=5)
   |                              ^^^^^ not built yet
   = note: this version takes the path, `sep=`, `na_rep=`, `header=` and `columns=`, and writes no index column"#;
    assert_eq!(check_one(program), expected);
}

/// `header=` of `to_csv` checks as a `bool` and `columns=` as a list, written out, of the
/// frame's column names (roadmap 3.14.4.2). A list of new names for the header, one name
/// alone, an empty list, a name the frame lacks, one named twice and a list worked out are
/// each reported, and `read_csv` has no `header=` yet.
#[test]
fn the_part_of_a_frame_to_write_is_named() {
    let program = r#"
class Trade(Struct):
    symbol: str
    price: f64 | None


def save(trades: DataFrame[Trade], quiet: bool) -> Raises[None, OSError | ValueError]:
    names = ["price"]
    trades.to_csv("out.csv", header=False, columns=["price", "symbol"])
    trades.to_csv("out.csv", header=not quiet, columns=["price"])
    trades.to_csv("out.csv", header=["a", "b"])
    trades.to_csv("out.csv", columns="price")
    trades.to_csv("out.csv", columns=[])
    trades.to_csv("out.csv", columns=["price", "volume"])
    trades.to_csv("out.csv", columns=["price", "price"])
    trades.to_csv("out.csv", columns=names)
    again: DataFrame[Trade] = DataFrame.read_csv("out.csv", header=False)
"#;
    let expected = r#"error: expected `bool`, found `list[str]`
  --> main.byn:10:37
   |
10 |     trades.to_csv("out.csv", header=["a", "b"])
   |                                     ^^^^^^^^^^ `header` is `bool`

error: `columns` of `to_csv` is a list of column names, written out
  --> main.byn:11:38
   |
11 |     trades.to_csv("out.csv", columns="price")
   |                                      ^^^^^^^ not a list of names
   = note: for example `columns=["symbol", "price"]`

error: `columns` of `to_csv` is a list of column names, written out
  --> main.byn:12:38
   |
12 |     trades.to_csv("out.csv", columns=[])
   |                                      ^^ not a list of names
   = note: for example `columns=["symbol", "price"]`

error: `DataFrame[Trade]` has no column `volume`
  --> main.byn:13:48
   |
13 |     trades.to_csv("out.csv", columns=["price", "volume"])
   |                                                ^^^^^^^^ not a column
   = note: its columns are `symbol`, `price`

error: the column `price` is named twice
  --> main.byn:14:48
   |
14 |     trades.to_csv("out.csv", columns=["price", "price"])
   |                                                ^^^^^^^ a second time

error: `columns` of `to_csv` is a list of column names, written out
  --> main.byn:15:38
   |
15 |     trades.to_csv("out.csv", columns=names)
   |                                      ^^^^^ not a list of names
   = note: for example `columns=["symbol", "price"]`

error: `DataFrame.read_csv` does not take `header` yet
  --> main.byn:16:61
   |
16 |     again: DataFrame[Trade] = DataFrame.read_csv("out.csv", header=False)
   |                                                             ^^^^^^ not built yet
   = note: this version takes the path, `sep=`, `na_values=`, `nrows=` and `skiprows=`, for a file with a header row"#;
    assert_eq!(check_one(program), expected);
}
