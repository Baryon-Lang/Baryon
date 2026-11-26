//! The keyword arguments of `DataFrame.read_csv` and `df.to_csv` (roadmap 3.14.4.2), built
//! and run. `sep=` is the character between fields: a file split by `;` or a tab is read as
//! `pandas.read_csv(path, sep=...)` reads it, and written with the bytes
//! `pandas.DataFrame.to_csv(path, index=False, sep=...)` writes. `na_values=` names more cells
//! to read as missing, and `na_rep=` is the text written for a missing value.
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
