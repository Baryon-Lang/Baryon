//! The keyword arguments of `DataFrame.read_csv` and `df.to_csv` (roadmap 3.14.4.2), built
//! and run. `sep=` is the character between fields: a file split by `;` or a tab is read as
//! `pandas.read_csv(path, sep=...)` reads it, and written with the bytes
//! `pandas.DataFrame.to_csv(path, index=False, sep=...)` writes. `na_values=` names more cells
//! to read as missing, and `na_rep=` is the text written for a missing value. `skiprows=`
//! passes over lines before the header and `nrows=` stops after so many rows. `header=False`
//! leaves the header row out of a file written, and `columns=` chooses its columns.
//!
//! What each test expects was checked against pandas 3.0.5 on the same bytes.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// The program of `fields_are_split_by_the_separator`: a file split by `;`, with a comma in
/// one field and a quoted `;` in another, read and then written with four separators.
const SEPARATORS: &str = r#"
class Trade(Struct):
    symbol: str
    price: f64
    volume: i64 | None
    note: str | None


def main() -> Raises[None, OSError | ValueError]:
    with open("in.csv", "w") as out:
        out.write("symbol;price;volume;note\n")
        out.write("A,B;1.5;10;\"x;y\"\n")
        out.write("\"q\"\"r\";2.5;;plain, text\n")
    trades: DataFrame[Trade] = DataFrame.read_csv("in.csv", sep=";")
    print(trades)
    trades.to_csv("semi.csv", sep=";")
    trades.to_csv("tab.csv", sep="\t")
    trades.to_csv("comma.csv")
    trades.to_csv("pipe.csv", sep='|')
    again: DataFrame[Trade] = DataFrame.read_csv("tab.csv", sep="\t")
    print(again)
    back: DataFrame[Trade] = DataFrame.read_csv("comma.csv", sep=",")
    print(back.note)
    try:
        wrong: DataFrame[Trade] = DataFrame.read_csv("semi.csv")
        print(len(wrong))
    except ValueError as error:
        print(error)
"#;

/// A separator splits the fields of a file read and stands between those of a file written.
/// Only a field that holds the separator, a quote or a line end is quoted, so a comma is
/// plain text in a file split by `;`. A file read with the wrong separator has one column,
/// which is not one the schema names.
#[test]
fn fields_are_split_by_the_separator() {
    let directory = package("frame_csv_separators", &[("main.byn", SEPARATORS)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable)
        .current_dir(directory.join("package"))
        .output()
        .unwrap();
    assert_eq!(stderr(&ran), "");
    let table = "  symbol  price  volume         note\n\
                 0    A,B    1.5      10          x;y\n\
                 1    q\"r    2.5    <NA>  plain, text\n";
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        format!(
            "{table}{table}['x;y' 'plain, text']\n\
             ValueError(message=\"semi.csv: the header has no column 'symbol'\")\n"
        )
    );
    let written = |name: &str| std::fs::read_to_string(directory.join("package").join(name));
    assert_eq!(
        written("semi.csv").unwrap(),
        "symbol;price;volume;note\nA,B;1.5;10;\"x;y\"\n\"q\"\"r\";2.5;;plain, text\n"
    );
    assert_eq!(
        written("tab.csv").unwrap(),
        "symbol\tprice\tvolume\tnote\nA,B\t1.5\t10\tx;y\n\"q\"\"r\"\t2.5\t\tplain, text\n"
    );
    assert_eq!(
        written("comma.csv").unwrap(),
        "symbol,price,volume,note\n\"A,B\",1.5,10,x;y\n\"q\"\"r\",2.5,,\"plain, text\"\n"
    );
    assert_eq!(
        written("pipe.csv").unwrap(),
        "symbol|price|volume|note\nA,B|1.5|10|x;y\n\"q\"\"r\"|2.5||plain, text\n"
    );
}

/// The program of `missing_values_are_spelled_as_asked`: a file whose missing cells are
/// spelled `NA`, `-999` and `n/a`, read with those as `na_values=`, then written with four
/// values of `na_rep=`.
const MISSING: &str = r#"
class Reading(Struct):
    site: str
    note: str | None
    level: f64
    depth: f64 | None
    count: i64 | None
    small: f32
    ok: bool | None


def spellings() -> list[str]:
    return ["NA", "-999", "n/a"]


def main() -> Raises[None, OSError | ValueError]:
    with open("in.csv", "w") as out:
        out.write("site,note,level,depth,count,small,ok\n")
        out.write("A,fine,1.5,2.5,3,0.5,True\n")
        out.write("NA,NA,NA,NA,NA,NA,NA\n")
        out.write("-999,\"n/a\",-999,-999.0,-999,-999,n/a\n")
        out.write("C, NA,2.5,,7,,False\n")
    plain: DataFrame[Reading] = DataFrame.read_csv("in.csv", na_values=["NA", "-999", "n/a"])
    print(plain)
    worked: DataFrame[Reading] = DataFrame.read_csv("in.csv", na_values=spellings())
    print(worked.count)
    try:
        one: DataFrame[Reading] = DataFrame.read_csv("in.csv", na_values="NA")
        print(len(one))
    except ValueError as error:
        print(error.message)
    plain.to_csv("default.csv")
    plain.to_csv("na.csv", na_rep="NA")
    word = "N;A"
    plain.to_csv("semi.csv", na_rep=word, sep=";")
    plain.to_csv("quote.csv", na_rep="q\"x,y")
    again: DataFrame[Reading] = DataFrame.read_csv("na.csv", na_values="NA")
    print(again.depth)
"#;

/// Valgrind set to fail on an error or a lost block, when it can run here.
fn valgrind() -> Option<Command> {
    let there = Command::new("valgrind")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    there.then(|| {
        let mut command = Command::new("valgrind");
        command.args([
            "--quiet",
            "--error-exitcode=99",
            "--leak-check=full",
            "--errors-for-leak-kinds=definite",
            "--show-leak-kinds=definite",
        ]);
        command
    })
}

/// A cell that is exactly one of `na_values`, quoted or not, reads as an empty cell reads
/// in its column: `None` where the type has it, NaN in a column of floats and empty text
/// in a column of `str`. ` NA` with a space and `-999.0` are not the texts given, and a
/// spelling left out is an error in a column of `bool`. `na_rep=` is written for every
/// `None` and NaN, quoted where it holds the separator or a quote; the four files are the
/// bytes pandas 3.0.5 writes. The program runs under Valgrind where there is one, since
/// each call makes and drops the list of spellings or the cell of a missing value.
#[test]
fn missing_values_are_spelled_as_asked() {
    let directory = package("frame_csv_missing", &[("main.byn", MISSING)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let home = directory.join("package");
    let ran = match valgrind() {
        Some(mut valgrind) => valgrind.arg(&executable).current_dir(&home).output(),
        None => Command::new(&executable).current_dir(&home).output(),
    }
    .unwrap();
    assert_eq!(stderr(&ran), "");
    assert!(ran.status.success());
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "  site  note  level  depth  count  small     ok\n\
         0    A  fine    1.5    2.5      3    0.5   True\n\
         1       <NA>    NaN   <NA>   <NA>    NaN   <NA>\n\
         2       <NA>    NaN -999.0   <NA>    NaN   <NA>\n\
         3    C    NA    2.5   <NA>      7    NaN  False\n\
         [3 None None 7]\n\
         column 'ok', row 3: could not read 'n/a' as bool\n\
         [2.5 None -999.0 None]\n"
    );
    let written = |name: &str| std::fs::read_to_string(home.join(name));
    let header = "site,note,level,depth,count,small,ok\nA,fine,1.5,2.5,3,0.5,True\n";
    assert_eq!(
        written("default.csv").unwrap(),
        format!("{header},,,,,,\n,,,-999.0,,,\nC, NA,2.5,,7,,False\n")
    );
    assert_eq!(
        written("na.csv").unwrap(),
        format!("{header},NA,NA,NA,NA,NA,NA\n,NA,NA,-999.0,NA,NA,NA\nC, NA,2.5,NA,7,NA,False\n")
    );
    assert_eq!(
        written("semi.csv").unwrap(),
        "site;note;level;depth;count;small;ok\nA;fine;1.5;2.5;3;0.5;True\n\
         ;\"N;A\";\"N;A\";\"N;A\";\"N;A\";\"N;A\";\"N;A\"\n\
         ;\"N;A\";\"N;A\";-999.0;\"N;A\";\"N;A\";\"N;A\"\n\
         C; NA;2.5;\"N;A\";7;\"N;A\";False\n"
    );
    let cell = "\"q\"\"x,y\"";
    assert_eq!(
        written("quote.csv").unwrap(),
        format!(
            "{header},{cell},{cell},{cell},{cell},{cell},{cell}\n\
             ,{cell},{cell},-999.0,{cell},{cell},{cell}\n\
             C, NA,2.5,{cell},7,{cell},False\n"
        )
    );
}

/// The program of `a_part_of_a_file_is_read`: a file with two lines and a blank one before
/// its header and a row too long at its end, read with several `skiprows=` and `nrows=`,
/// and a file whose first record holds a line end in quotes.
const PART: &str = r##"
class Pair(Struct):
    a: i64
    b: i64 | None


def show(skip: int, rows: int) -> Raises[None, OSError]:
    try:
        frame: DataFrame[Pair] = DataFrame.read_csv("in.csv", skiprows=skip, nrows=rows)
        print(skip, rows, frame.a, frame.b)
    except ValueError as error:
        print(skip, rows, error.message)


def main() -> Raises[None, OSError | ValueError]:
    with open("in.csv", "w") as out:
        out.write("# made by x\n\nunits,m\na,b\n1,2\n\n3,\n5,6,7,8\n")
    with open("quoted.csv", "w") as out:
        out.write("\"x\ny\",z\r\na,b\r\n1,2\r\n")
    show(3, 2)
    show(3, 0)
    show(3, 3)
    show(3, 100)
    show(2, 2)
    show(-1, 2)
    show(3, -1)
    show(100, 1)
    show(7, 1)
    skipped: DataFrame[Pair] = DataFrame.read_csv("quoted.csv", skiprows=1)
    print(skipped)
    first: DataFrame[Pair] = DataFrame.read_csv("quoted.csv", skiprows=1, nrows=1, sep=",")
    print(len(first))
"##;

/// `skiprows=` counts every line before the header, a blank one too, and a record whose
/// quotes hold a line end as one; a negative count skips nothing. `nrows=` stops after so
/// many data rows, so a bad row after them is no error, 0 gives an empty frame, a count
/// past the end reads what there is, and a negative one is a `ValueError`. Each case is
/// what pandas 3.0.5 does, where it raises `ParserError` and `EmptyDataError` for the
/// `ValueError`s here.
#[test]
fn a_part_of_a_file_is_read() {
    let directory = package("frame_csv_part", &[("main.byn", PART)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable)
        .current_dir(directory.join("package"))
        .output()
        .unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "3 2 [1 3] [2 None]\n\
         3 0 [] []\n\
         3 3 in.csv: expected 2 fields in row 3, saw 4\n\
         3 100 in.csv: expected 2 fields in row 3, saw 4\n\
         2 2 in.csv: the header has no column 'a'\n\
         -1 2 in.csv: the header has no column 'a'\n\
         3 -1 in.csv: nrows is -1, and it cannot be less than 0\n\
         100 1 in.csv: no columns to read after skipping 100 lines\n\
         7 1 in.csv: the header has no column 'a'\n   \
         a  b\n\
         0  1  2\n\
         1\n"
    );
}

/// The program of `a_part_of_a_frame_is_written`.
const WRITTEN: &str = r#"
def main() -> Raises[None, OSError]:
    frame = DataFrame({
        "name": Array(["a", "b,c", ""]),
        "price": Array([1.5, None, 3.0]),
        "volume": Array([1, 2, 3]),
    })
    frame.to_csv("plain.csv", header=False)
    frame.to_csv("some.csv", columns=["volume", "name"])
    quiet = len(frame) > 5
    frame.to_csv("both.csv", columns=["price"], header=quiet, na_rep="NA", sep=";")
    frame.to_csv("one.csv", columns=["name"], header=False)
    frame.to_csv("all.csv", header=True, columns=["name", "price", "volume"])
"#;

/// `header=False` writes the rows alone, and `columns=` writes the columns it names in the
/// order it names them. A file of one column still writes an empty cell as `""`. Each file
/// is the bytes pandas 3.0.5 writes with the same keywords and `index=False`.
#[test]
fn a_part_of_a_frame_is_written() {
    let directory = package("frame_csv_written", &[("main.byn", WRITTEN)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let home = directory.join("package");
    let ran = match valgrind() {
        Some(mut valgrind) => valgrind.arg(&executable).current_dir(&home).output(),
        None => Command::new(&executable).current_dir(&home).output(),
    }
    .unwrap();
    assert_eq!(stderr(&ran), "");
    assert!(ran.status.success());
    let written = |name: &str| std::fs::read_to_string(home.join(name));
    assert_eq!(
        written("plain.csv").unwrap(),
        "a,1.5,1\n\"b,c\",,2\n,3.0,3\n"
    );
    assert_eq!(
        written("some.csv").unwrap(),
        "volume,name\n1,a\n2,\"b,c\"\n3,\n"
    );
    assert_eq!(written("both.csv").unwrap(), "1.5\nNA\n3.0\n");
    assert_eq!(written("one.csv").unwrap(), "a\n\"b,c\"\n\"\"\n");
    assert_eq!(
        written("all.csv").unwrap(),
        "name,price,volume\na,1.5,1\n\"b,c\",,2\n,3.0,3\n"
    );
}
