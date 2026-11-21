//! The keyword arguments of `DataFrame.read_csv` and `df.to_csv` (roadmap 3.14.4.2), built
//! and run. `sep=` is the character between fields: a file split by `;` or a tab is read as
//! `pandas.read_csv(path, sep=...)` reads it, and written with the bytes
//! `pandas.DataFrame.to_csv(path, index=False, sep=...)` writes.
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
