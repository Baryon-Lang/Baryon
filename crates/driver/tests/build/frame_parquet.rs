//! `DataFrame.read_parquet` and `to_parquet` (roadmap 3.14.6.3 and 3.14.6.4, decision 0045),
//! built and run. A frame written is read back as it was, and the bytes written are those of
//! a file polars reads as the same table. Reading: Parquet files
//! written by polars, and two written by hand with version 2 data pages, are read into frames
//! whose columns hold what polars reads from the same files, and a file unlike the schema, cut
//! short or not Parquet at all raises `ValueError` with a message that names the column.
//!
//! The files are in `tests/fixtures/parquet`; its `README.md` says what each holds and how it
//! was written.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of the Parquet files.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/parquet");

/// Builds `program` as the package `name` with the fixture `files` beside it, runs it there,
/// and returns what it printed and the directory it ran in, which is removed when dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str, files: &[&str]) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    for file in files {
        fs::copy(Path::new(FIXTURES).join(file), directory.join(file))?;
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let ran = Command::new(&executable)
        .current_dir(&*directory)
        .output()?;
    Ok((ran, directory))
}

/// The program of `every_type_and_codec_is_read`: a schema with a column of every type the
/// reader has, with and without `None`, read from one file for each codec and written as CSV.
const TYPES: &str = r#"
class All(Struct):
    f: f64 | None
    g: f32
    a: i8 | None
    b: i16
    c: i32 | None
    d: i64 | None
    p: u8
    q: u16 | None
    r: u32
    s: u64 | None
    t: bool | None
    u: bool
    w: str | None
    x: str
    z: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    for codec in ["uncompressed", "snappy", "gzip", "zstd", "lz4"]:
        frame: DataFrame[All] = DataFrame.read_parquet(f"types_{codec}.parquet")
        frame.to_csv(f"out_{codec}.csv", na_rep="<none>")
        print(codec, len(frame))
"#;

/// Each of the five files holds the same 120 rows in three row groups of small pages, plain
/// and through dictionaries: floats with infinities, signed zero and the smallest and largest
/// values, whole numbers at the ends of each width, booleans, text with commas, quotes and
/// line ends, a column of nothing but missing values, and one column the schema leaves out.
/// What is read from every codec, written as CSV, is byte for byte the CSV the script that
/// wrote the files made from the values it gave polars.
#[test]
fn every_type_and_codec_is_read() {
    let files = [
        "types_uncompressed.parquet",
        "types_snappy.parquet",
        "types_gzip.parquet",
        "types_zstd.parquet",
        "types_lz4.parquet",
    ];
    let (ran, directory) = run("frame_parquet_types", TYPES, &files).unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "uncompressed 120\nsnappy 120\ngzip 120\nzstd 120\nlz4 120\n"
    );
    let expected = fs::read(Path::new(FIXTURES).join("types.csv")).unwrap();
    for codec in ["uncompressed", "snappy", "gzip", "zstd", "lz4"] {
        let written = fs::read(directory.join(format!("out_{codec}.csv"))).unwrap();
        assert!(
            written == expected,
            "the file of {codec} was read differently"
        );
    }
}

/// The program of `version_two_pages_are_read`.
const PAGES: &str = r#"
class Row(Struct):
    n: i64 | None
    s: str
    b: bool | None
    x: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    for name in ["pages_v2.parquet", "pages_v2_gzip.parquet"]:
        frame: DataFrame[Row] = DataFrame.read_parquet(name)
        print(frame["n"])
        print(frame["s"])
        print(frame["b"])
        print(frame["x"])
"#;

/// A file whose data pages are of version 2 is read: the levels outside the compressed bytes,
/// a dictionary with runs bit-packed in one page and repeated in the next, booleans as runs,
/// and pages a compressed file marks as stored plainly. polars reads the same values from
/// both files.
#[test]
fn version_two_pages_are_read() {
    let files = ["pages_v2.parquet", "pages_v2_gzip.parquet"];
    let (ran, _directory) = run("frame_parquet_pages", PAGES, &files).unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    let once = "[10 None -20 9007199254740993 None 10 -20 9007199254740993 9007199254740993 None]\n\
                ['alpha' '' 'héllo' 'd,e' 'x' 'y' 'z' 'tail' '日本' 'end']\n\
                [True None False True True None False False True None]\n\
                [1.5 None -0.0 2.5e-300 None 1e+300 3.0 None 4.25 5.0]\n";
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        format!("{once}{once}")
    );
}

/// The program of `a_file_unlike_the_schema_is_an_error`: one schema for each way a file can
/// differ from what a program names, each read in its own function so that every error is
/// caught and printed.
const UNLIKE: &str = r#"
class Good(Struct):
    ratio: f64
    count: i64 | None
    label: str
    big: i16


class When(Struct):
    when: i32


class Stamp(Struct):
    stamp: i64 | None


class Amount(Struct):
    amount: f64


class Items(Struct):
    items: i64


class Count(Struct):
    count: i64


class Label(Struct):
    label: f64


class Ratio(Struct):
    ratio: i64 | None


class Small(Struct):
    big: u8


class Flag(Struct):
    count: bool


class Text(Struct):
    count: str


class Absent(Struct):
    label: str
    nope: f64


def when() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[When] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def stamp() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Stamp] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def amount() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Amount] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def items() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Items] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def count() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Count] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def label() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Label] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def ratio() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Ratio] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def small() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Small] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def flag() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Flag] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def text() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Text] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def absent() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Absent] = DataFrame.read_parquet("other.parquet")
    print(len(frame))


def file(path: str) -> Raises[None, OSError | ValueError]:
    frame: DataFrame[Good] = DataFrame.read_parquet(path)
    print(frame)


def show(index: int) -> Raises[None, OSError | ValueError]:
    if index == 0:
        when()
    elif index == 1:
        stamp()
    elif index == 2:
        amount()
    elif index == 3:
        items()
    elif index == 4:
        count()
    elif index == 5:
        label()
    elif index == 6:
        ratio()
    elif index == 7:
        small()
    elif index == 8:
        flag()
    elif index == 9:
        text()
    elif index == 10:
        absent()
    elif index == 11:
        file("text.parquet")
    elif index == 12:
        file("cut.parquet")
    elif index == 13:
        file("none.parquet")
    else:
        file("other.parquet")


def main() -> None:
    for index in range(15):
        try:
            show(index)
        except ValueError as error:
            print(error.message)
        except OSError as error:
            print(error)
"#;

/// A column that holds a date, a timestamp or a decimal, a nested column, a missing value
/// where the type has no `None`, a column of another kind than its field, a number outside
/// the field's range and a column the file lacks are each a `ValueError` that names the
/// column, and so are a file that is not Parquet and one cut short. A file that is not there
/// is the `OSError` of opening it. A missing float where the type has no `None` is NaN, as
/// `read_csv` reads an empty cell.
#[test]
fn a_file_unlike_the_schema_is_an_error() {
    let directory = package("frame_parquet_unlike", &[("main.byn", UNLIKE)]).unwrap();
    let other = fs::read(Path::new(FIXTURES).join("other.parquet")).unwrap();
    fs::write(directory.join("other.parquet"), &other).unwrap();
    fs::write(directory.join("cut.parquet"), &other[..700]).unwrap();
    fs::write(directory.join("text.parquet"), "not parquet at all").unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let ran = Command::new(&executable)
        .current_dir(&*directory)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    let unread = "holds a date, a time, a decimal or another kind of value that is not read yet";
    let expected = format!(
        "other.parquet: the column 'when' {unread}\n\
         other.parquet: the column 'stamp' {unread}\n\
         other.parquet: the column 'amount' {unread}\n\
         other.parquet: the column 'items' is nested, which is not read yet\n\
         other.parquet: column 'count', row 2: the value is missing, and the column's type \
         has no None\n\
         other.parquet: the column 'label' holds text, which is not read into a column of \
         f64\n\
         other.parquet: the column 'ratio' holds floats (DOUBLE), which is not read into a \
         column of i64\n\
         other.parquet: column 'big', row 2: could not read 300 as u8\n\
         other.parquet: the column 'count' holds whole numbers (INT64), which is not read \
         into a column of bool\n\
         other.parquet: the column 'count' holds whole numbers (INT64), which is not read \
         into a column of str\n\
         other.parquet: the file has no column 'nope'\n\
         text.parquet: not a Parquet file\n\
         cut.parquet: the file is cut short, with no metadata at its end\n\
         OSError(kind=ErrorKind.FILE_NOT_FOUND, errno=2, strerror='No such file or \
         directory', filename='none.parquet')\n   \
            ratio  count label  big\n\
         0    0.5      1     a    1\n\
         1    NaN   <NA>     b  300\n\
         2    1.5      3     c   -1\n"
    );
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected);
}

/// The program of `a_frame_written_is_the_file_polars_reads`: every type of column read from
/// a file polars wrote, written, read back and written as CSV; then a frame of no rows.
const WRITTEN: &str = r#"
class All(Struct):
    f: f64 | None
    g: f32
    a: i8 | None
    b: i16
    c: i32 | None
    d: i64 | None
    p: u8
    q: u16 | None
    r: u32
    s: u64 | None
    t: bool | None
    u: bool
    w: str | None
    x: str
    z: f64 | None


def main() -> Raises[None, OSError | ValueError]:
    frame: DataFrame[All] = DataFrame.read_parquet("types_snappy.parquet")
    frame.to_parquet("back.parquet")
    again: DataFrame[All] = DataFrame.read_parquet("back.parquet")
    again.to_csv("back.csv", na_rep="<none>")
    frame.head(0).to_parquet("empty.parquet")
    empty: DataFrame[All] = DataFrame.read_parquet("empty.parquet")
    print(len(again), len(empty))
"#;

/// A frame with a column of every type the writer has, with and without `None`, is written
/// byte for byte as `types_written.parquet`, a file polars reads as the table it wrote into
/// `types_snappy.parquet`, with the same types. Read back here, it holds what the first file
/// held. A frame of no rows writes a file that reads as no rows.
#[test]
fn a_frame_written_is_the_file_polars_reads() {
    let files = ["types_snappy.parquet"];
    let (ran, directory) = run("frame_parquet_written", WRITTEN, &files).unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "120 0\n");
    let fixture = |name: &str| fs::read(Path::new(FIXTURES).join(name)).unwrap();
    let written = fs::read(directory.join("back.parquet")).unwrap();
    assert!(
        written == fixture("types_written.parquet"),
        "the bytes written have changed"
    );
    let back = fs::read(directory.join("back.csv")).unwrap();
    assert!(
        back == fixture("types.csv"),
        "the file written was read back differently"
    );
}

/// The program of `special_values_are_written`.
const SPECIAL: &str = r#"
from math import inf, nan


class Row(Struct):
    x: f64
    y: f64 | None
    name: str | None
    flag: bool | None


def main() -> Raises[None, OSError | ValueError]:
    frame = DataFrame({
        "x": Array([nan, inf, -inf, -0.0, 5e-324, 1.7976931348623157e308]),
        "y": Array([None, nan, 1.5, None, None, -2.5]),
        "name": Array(["", None, "日本語", "a\nb", None, "x"]),
        "flag": Array([True, None, False, None, None, True]),
    })
    frame.to_parquet("special.parquet")
    back: DataFrame[Row] = DataFrame.read_parquet("special.parquet")
    print(back["x"])
    print(back["y"])
    print(back["name"])
    print(back["flag"])
    try:
        frame.to_parquet("no/such/directory/special.parquet")
    except OSError as error:
        print(error.filename)
"#;

/// NaN, the infinities, signed zero and the smallest and largest floats are written and read
/// back, a NaN that is there stays apart from a value that is missing, an empty text stays
/// apart from a missing one. A path that cannot be written is an `OSError`.
#[test]
fn special_values_are_written() {
    let (ran, _directory) = run("frame_parquet_special", SPECIAL, &[]).unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[             nan              inf             -inf -0.00000000e+000\n  \
         5.00000000e-324  1.79769313e+308]\n\
         [None nan 1.5 None None -2.5]\n\
         ['' None '日本語' 'a\\nb' None 'x']\n\
         [True None False None None True]\n\
         no/such/directory/special.parquet\n"
    );
}
