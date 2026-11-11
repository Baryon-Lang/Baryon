//! Text tables of numbers (decision 0043), built and run: `Array.loadtxt` reads tables as `NumPy`
//! reads them, `Array.savetxt` writes the bytes `NumPy` writes, and bad data raises `ValueError`
//! with `NumPy`'s message.
//!
//! The files in `tests/fixtures/text_tables` were written by hand or by `NumPy`; its `README.md`
//! says which and how.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of tables and of the files `NumPy` wrote from them.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/text_tables"
    ))
}

/// Copies every table into `directory`, where the program under test runs.
fn copy_fixtures(directory: &Path) -> io::Result<()> {
    for entry in fs::read_dir(fixtures())? {
        let path = entry?.path();
        if let Some(name) = path.file_name().filter(|name| *name != "README.md") {
            fs::copy(&path, directory.join(name))?;
        }
    }
    Ok(())
}

/// Builds `program` as the package `name` with the tables beside it, runs it there, and returns
/// what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    copy_fixtures(&directory)?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// A program reads a table with a comment, a header line, a blank line, spaces around fields
/// and an end-of-line comment, a column into a rank-1 array, and a square table, and prints
/// what `NumPy` holds.
#[test]
fn tables_are_read_as_numpy_reads_them() {
    let program = "\
def main() -> Raises[None, OSError | ValueError]:
    run = Array.loadtxt(\"run.csv\", delimiter=\",\", skiprows=2)
    print(run.shape)
    print(run[1, 1], run[0, 2], run[2, 1], run[2, 2])
    print(Array.loadtxt(\"run.csv\", f64, delimiter=\",\", skiprows=2, max_rows=1).shape)
    column: Array[f64, 1] = Array.loadtxt(\"column.txt\")
    print(column)
    print(Array.loadtxt(\"square.txt\", dtype=f64))
    print(Array.loadtxt(\"square.txt\", comments=\"\", skiprows=2).shape)
    row: Array[f64, 1] = Array.loadtxt(\"square.txt\", max_rows=1)
    print(row)
";
    let (output, _) = run("text_tables_read", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "(3, 3)\n\
         -0.00125 101325.0 inf nan\n\
         (1, 3)\n\
         [ 1.50e+000 -2.25e+000  1.00e+300]\n\
         [[1. 2.]\n \
         [3. 4.]]\n\
         (0, 1)\n\
         [1. 2.]\n"
    );
}

/// A program writes tables and a column with each of `savetxt`'s keywords, and each file holds
/// the bytes `NumPy` wrote for the same array. A table written and read again is unchanged.
#[test]
fn written_tables_match_numpy() {
    let program = "\
def main() -> Raises[None, OSError | ValueError]:
    run = Array.loadtxt(\"run.csv\", delimiter=\",\", skiprows=2)
    Array.savetxt(\"out_run.txt\", run)
    Array.savetxt(
        \"out_run.csv\",
        run,
        delimiter=\",\",
        header=\"time,energy\\npressure\",
        footer=\"end\",
        comments=\"% \",
    )
    column: Array[f64, 1] = Array.loadtxt(\"column.txt\")
    Array.savetxt(\"out_column.txt\", X=column)
    first = Array.loadtxt(\"run.csv\", delimiter=\",\", skiprows=2, max_rows=2)
    Array.savetxt(\"out_first_rows.txt\", first, newline=\"\\r\\n\")
    again = Array.loadtxt(\"out_run.csv\", delimiter=\",\", comments=\"%\")
    print(again.shape, again[1, 1] == run[1, 1], again[2, 1], again[2, 2])
";
    let (output, directory) = run("text_tables_written", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "(3, 3) True inf nan\n"
    );
    for name in ["run.txt", "run.csv", "column.txt"] {
        let written = fs::read(directory.join(format!("out_{name}"))).unwrap();
        let numpy = fs::read(fixtures().join(format!("numpy_{name}"))).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&written),
            String::from_utf8_lossy(&numpy),
            "{name}"
        );
    }
    let written = fs::read_to_string(directory.join("out_first_rows.txt")).unwrap();
    let numpy = fs::read_to_string(fixtures().join("numpy_first_rows.txt")).unwrap();
    assert_eq!(written, numpy.replace('\n', "\r\n"));
}

/// Bad data raises `ValueError` with `NumPy`'s message, word for word, a table read where a
/// rank-1 array is needed raises one too, and a missing file raises `OSError`.
#[test]
fn bad_tables_raise_numpys_errors() {
    let program = "\
def read(path: str) -> None:
    try:
        table = Array.loadtxt(path)
        print(table.shape)
    except ValueError as error:
        print(error.message)
    except OSError as failure:
        print(failure.strerror)


def main() -> None:
    for path in [\"bad_value.txt\", \"ragged.txt\", \"hex.txt\", \"underscore.txt\", \"none.txt\"]:
        read(path)
    try:
        column: Array[f64, 1] = Array.loadtxt(\"square.txt\")
        print(column)
    except ValueError as error:
        print(error.message)
    except OSError as failure:
        print(failure.strerror)
";
    let (output, _) = run("text_tables_failing", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "could not convert string 'x' to float64 at row 1, column 2.\n\
         the number of columns changed from 3 to 2 at row 2; use `usecols` to select a subset \
         and avoid this error\n\
         could not convert string '0x10' to float64 at row 0, column 1.\n\
         could not convert string '1_000' to float64 at row 0, column 1.\n\
         No such file or directory\n\
         `Array.loadtxt` read 2 rows of 2 columns, which need an `Array[f64, 2]`, not a rank-1 \
         array\n"
    );
}
