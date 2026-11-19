//! CSV files read into a frame (decision 0045, roadmap 3.14.4), built and run:
//! `DataFrame.read_csv(path)` reads a file as `pandas.read_csv` reads it with its defaults,
//! into the columns the frame's type names, and raises `OSError` or `ValueError` when it
//! cannot.
//!
//! The values each test expects were checked against pandas 3.0.5 reading the same bytes
//! with `dtype=` set to the schema's types.

use super::{Scratch, baryon_build, package, stderr};
use std::io;
use std::process::{Command, Output};

/// The schema every test reads: text, a number that may be missing, a whole number, a `bool`
/// and text that may be missing.
const SCHEMA: &str = "\
class Trade(Struct):
    symbol: str
    price: f64 | None
    volume: i64
    live: bool
    note: str | None
";

/// Builds `SCHEMA` and `body` as the package `name` with `files` beside the program, runs it
/// in the package's directory, and returns what it printed and that directory, which is
/// removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, body: &str, files: &[(&str, &str)]) -> io::Result<(Output, Scratch)> {
    let program = format!("{SCHEMA}\n\n{body}");
    let mut written = vec![("main.byn", program.as_str())];
    written.extend_from_slice(files);
    let directory = package(name, &written)?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable)
        .current_dir(directory.join("package"))
        .output()?;
    Ok((run, directory))
}

/// A file with its columns in another order than the schema's and one the schema does not
/// name, a quoted field holding a comma, a doubled quote and a line end, a blank line, a
/// `"\r\n"` line end, spaces around numbers, empty cells and a row that ends early: each
/// column comes out as pandas reads it, an empty cell of an optional column being `None`.
#[test]
fn files_are_read_as_pandas_reads_them() {
    let body = "\
def main() -> Raises[None, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv(\"trades.csv\")
    print(trades.shape)
    print(trades.symbol)
    print(trades.price)
    print(trades.volume)
    print(trades.live)
    print(trades.note)
    print(trades.price.sum(), trades.volume.sum(), trades.note.count())
";
    let file = "\u{feff}volume,live,skipped,symbol,note,price\n\
                100,True,x,AAA,\"said \"\"hi\"\", twice\",1.5\n\
                \n\
                7,false,y,\"B,B\",,\r\n\
                -3,TRUE,z,CCC,\"two\nlines\", 2.5e3 \n\
                +4,False,,,,inf\n\
                0,False\n";
    let (ran, _directory) = run("csv_read", body, &[("trades.csv", file)]).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "(5, 5)\n\
         ['AAA' 'B,B' 'CCC' '' '']\n\
         [1.5 None 2500.0 inf None]\n\
         [100   7  -3   4   0]\n\
         [ True False  True False False]\n\
         ['said \"hi\", twice' None 'two\\nlines' None None]\n\
         inf 108 2\n"
    );
}

/// Each way a file can fail to fit the schema raises `ValueError` with a message that names
/// the file or the column and the data row, and a file that is not there raises `OSError`.
/// The columns read before the bad one are freed: under Valgrind nothing leaks.
#[test]
fn bad_files_raise() {
    let body = "\
def rows(path: str) -> Raises[int, OSError | ValueError]:
    trades: DataFrame[Trade] = DataFrame.read_csv(path)
    return len(trades)


def main():
    for name in [\"number\", \"whole\", \"flag\", \"empty\", \"header\", \"long\", \"nothing\"]:
        try:
            print(rows(name + \".csv\"))
        except ValueError as error:
            print(error.message)
        except OSError as error:
            print(\"OSError\", name)
    try:
        print(rows(\"absent.csv\"))
    except ValueError as error:
        print(error.message)
    except OSError as error:
        print(error.filename, error.errno)
";
    let header = "symbol,price,volume,live,note\n";
    let files = [
        (
            "number.csv",
            format!("{header}A,1.5,1,True,\nB,1_0,2,True,\n"),
        ),
        ("whole.csv", format!("{header}A,1.5,1e3,True,\n")),
        ("flag.csv", format!("{header}A,1.5,1,yes,\n")),
        ("empty.csv", format!("{header}A,1.5,1,True,\nB,,,True,\n")),
        (
            "header.csv",
            "symbol,price,live,note\nA,1.5,True,\n".to_owned(),
        ),
        (
            "long.csv",
            format!("{header}A,1.5,1,True,x\n\nB,2.5,2,True,y,z\n"),
        ),
        ("nothing.csv", "\n\n".to_owned()),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let (ran, _directory) = run("csv_errors", body, &files).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "column 'price', row 2: could not read '1_0' as f64\n\
         column 'volume', row 1: could not read '1e3' as i64\n\
         column 'live', row 1: could not read 'yes' as bool\n\
         column 'volume', row 2: the cell is empty, and the column's type has no None\n\
         header.csv: the header has no column 'volume'\n\
         long.csv: expected 5 fields in row 2, saw 6\n\
         nothing.csv: no columns to read, the file is empty\n\
         absent.csv 2\n"
    );
}

/// An empty cell of a column of `f64` whose type has no `None` is NaN, as pandas reads it
/// into a `float64` column, and the same cell of a column that may hold `None` is `None`.
#[test]
fn empty_cells_of_plain_floats_are_nan() {
    let body = "\
class Reading(Struct):
    plain: f64
    gaps: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    readings: DataFrame[Reading] = DataFrame.read_csv(\"readings.csv\")
    print(readings.plain)
    print(readings.gaps)
";
    let file = "plain,gaps\n1.5,2.5\n,\nNaN,nan\n";
    let (ran, _directory) = run("csv_nan", body, &[("readings.csv", file)]).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[1.5 nan nan]\n[2.5 None nan]\n"
    );
}
