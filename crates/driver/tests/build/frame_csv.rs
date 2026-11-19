//! CSV files read into a frame and written from one (decision 0045, roadmap 3.14.4), built
//! and run: `DataFrame.read_csv(path)` reads a file as `pandas.read_csv` reads it with its
//! defaults, into the columns the frame's type names, and raises `OSError` or `ValueError`
//! when it cannot. `df.to_csv(path)` writes the bytes
//! `pandas.DataFrame.to_csv(path, index=False)` writes.
//!
//! The values each test expects were checked against pandas 3.0.5 reading the same bytes
//! with `dtype=` set to the schema's types, and writing the same columns.

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

/// `to_csv` writes the bytes `pandas.DataFrame.to_csv(path, index=False)` writes for the same
/// columns: text quoted only where it holds a comma, a double quote or a line end, a float
/// as its shortest text, NaN and `None` as empty cells, whole numbers of any width and
/// `True` and `False`. A frame of one column writes an empty cell as `""`, a frame of no
/// rows writes its header alone, and the file reads back with `read_csv`. A path that cannot
/// be written raises `OSError`.
#[test]
fn frames_are_written_as_pandas_writes_them() {
    let body = "\
class Row(Struct):
    name: str
    bid: f64 | None
    volume: i64


def main() -> Raises[None, OSError | ValueError]:
    small: Array[i32, 1] = Array([1, -2, 3, 4])
    byte: Array[u8, 1] = Array([255, 0, 7, 9])
    frame = DataFrame({
        \"name\": Array([\"AAPL\", \"BRK,B\", \"say \\\"hi\\\"\", \"\"]),
        \"note\": Array([\"a\", None, \"two\\nlines\", \"\"]),
        \"price\": Array([189.5, float(\"nan\"), 1e-7, 100.0]),
        \"bid\": Array([1.0, None, 2.5e20, -0.0]),
        \"volume\": Array([100, -3, 0, 9007199254740993]),
        \"lots\": Array([1, None, 3, 4]),
        \"small\": small,
        \"byte\": byte,
        \"live\": Array([True, False, True, True]),
        \"seen\": Array([True, None, False, None]),
    })
    frame.to_csv(\"out.csv\")
    back: DataFrame[Row] = DataFrame.read_csv(\"out.csv\")
    print(back.name)
    print(back.bid)
    print(back.volume)
    one = DataFrame({\"only\": Array([\"x\", \"\", \"z\"])})
    one.to_csv(\"one.csv\")
    none = frame[frame.price > 1e300]
    none.to_csv(\"none.csv\")
    try:
        frame.to_csv(\"absent/out.csv\")
    except OSError as error:
        print(error.filename, error.errno)
";
    let (ran, directory) = run("csv_write", body, &[]).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "['AAPL' 'BRK,B' 'say \"hi\"' '']\n\
         [1.0 None 2.5e+20 -0.0]\n\
         [             100               -3                0 9007199254740993]\n\
         absent/out.csv 2\n"
    );
    let written = |name: &str| std::fs::read_to_string(directory.join("package").join(name));
    assert_eq!(
        written("out.csv").unwrap(),
        "name,note,price,bid,volume,lots,small,byte,live,seen\n\
         AAPL,a,189.5,1.0,100,1,1,255,True,True\n\
         \"BRK,B\",,,,-3,,-2,0,False,\n\
         \"say \"\"hi\"\"\",\"two\nlines\",1e-07,2.5e+20,0,3,3,7,True,False\n\
         ,,100.0,-0.0,9007199254740993,4,4,9,True,\n"
    );
    assert_eq!(written("one.csv").unwrap(), "only\nx\n\"\"\nz\n");
    assert_eq!(
        written("none.csv").unwrap(),
        "name,note,price,bid,volume,lots,small,byte,live,seen\n"
    );
}

/// Columns of the narrower number types (roadmap 3.14.4.1): every whole number type reads
/// its least and greatest value, `u64` past the range of `i64`, and `f32` reads each cell
/// rounded to an `f32`. A cell outside its type's range raises `ValueError` where pandas
/// keeps the low bits. `to_csv` writes the bytes pandas writes for the same columns, an
/// `f32` in the form pandas writes: scientific from a million up.
#[test]
fn narrow_number_columns_are_read_and_written() {
    let body = "\
class Row(Struct):
    g: u64
    c: i32
    f: u32
    h: f32
    o: i16 | None
    b: i16
    e: u16
    a: i8
    d: u8


class Small(Struct):
    a: i8


class Byte(Struct):
    d: u8


class Large(Struct):
    g: u64


class Single(Struct):
    h: f32 | None


def small(path: str) -> Raises[int, OSError | ValueError]:
    rows: DataFrame[Small] = DataFrame.read_csv(path)
    return len(rows)


def byte(path: str) -> Raises[int, OSError | ValueError]:
    rows: DataFrame[Byte] = DataFrame.read_csv(path)
    return len(rows)


def large(path: str) -> Raises[int, OSError | ValueError]:
    rows: DataFrame[Large] = DataFrame.read_csv(path)
    return len(rows)


def single(path: str) -> Raises[int, OSError | ValueError]:
    rows: DataFrame[Single] = DataFrame.read_csv(path)
    return len(rows)


def main() -> Raises[None, OSError]:
    try:
        rows: DataFrame[Row] = DataFrame.read_csv(\"all.csv\")
        print(rows.g)
        print(rows.o)
        print(rows.a, rows.d)
        rows.to_csv(\"back.csv\")
    except ValueError as error:
        print(error.message)
    try:
        print(small(\"small.csv\"))
    except ValueError as error:
        print(error.message)
    try:
        print(byte(\"byte.csv\"))
    except ValueError as error:
        print(error.message)
    try:
        print(large(\"large.csv\"))
    except ValueError as error:
        print(error.message)
    try:
        print(single(\"single.csv\"))
    except ValueError as error:
        print(error.message)
";
    let all = "a,b,c,d,e,f,g,h,o\n\
               -128,-32768,-2147483648,255,65535,4294967295,18446744073709551615,0.1,\n\
               127,32767,2147483647,0, +7 ,0,0,16777217,5\n\
               1,2,3,4,5,6, +42 ,1234567.5,\n\
               0,0,0,0,0,0,0,nan,-3\n\
               0,0,0,0,0,0,0,0.0001,1\n\
               0,0,0,0,0,0,0,1e-5,1\n\
               0,0,0,0,0,0,0,-2.5e6,1\n\
               0,0,0,0,0,0,0,inf,1\n\
               0,0,0,0,0,0,0,123456.7,1\n";
    let files = [
        ("all.csv", all),
        ("small.csv", "a\n127\n128\n"),
        ("byte.csv", "d\n-1\n"),
        ("large.csv", "g\n18446744073709551616\n"),
        ("single.csv", "h\n1.5\n\n1_0\n"),
    ];
    let (ran, directory) = run("csv_narrow", body, &files).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[18446744073709551615                    0                   42\n                    \
         0                    0                    0\n                    \
         0                    0                    0]\n\
         [None 5 None -3 1 1 1 1 1]\n\
         [-128  127    1    0    0    0    0    0    0] [255   0   4   0   0   0   0   0   0]\n\
         column 'a', row 2: could not read '128' as i8\n\
         column 'd', row 1: could not read '-1' as u8\n\
         column 'g', row 1: could not read '18446744073709551616' as u64\n\
         column 'h', row 2: could not read '1_0' as f32\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("package").join("back.csv")).unwrap(),
        "g,c,f,h,o,b,e,a,d\n\
         18446744073709551615,-2147483648,4294967295,0.1,,-32768,65535,-128,255\n\
         0,2147483647,0,1.6777216e+07,5,32767,7,127,0\n\
         42,3,6,1.2345675e+06,,2,5,1,4\n\
         0,0,0,,-3,0,0,0,0\n\
         0,0,0,1e-04,1,0,0,0,0\n\
         0,0,0,1e-05,1,0,0,0,0\n\
         0,0,0,-2.5e+06,1,0,0,0,0\n\
         0,0,0,inf,1,0,0,0,0\n\
         0,0,0,123456.7,1,0,0,0,0\n"
    );
}
