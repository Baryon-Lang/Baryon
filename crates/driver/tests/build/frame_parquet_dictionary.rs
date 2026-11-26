//! `df.to_parquet` with text columns (roadmap 3.14.6.4, decision 0045), built and run: a
//! column of text is written through a dictionary, across more than one data page, with long
//! runs of one value and with missing values, and a column of too many distinct values is
//! written plainly. What is read back is what was written.
//!
//! The layout of a dictionary page and of the pages that index it is pinned by
//! `types_written.parquet` in `frame_parquet.rs`, a file polars reads; this test is the
//! round trip at a size that small file does not reach.

use super::{baryon_build, package};
use std::fs;
use std::process::Command;

/// Makes 70,000 rows, more than the 65,536 of a data page: `runs`, five texts in runs of
/// 3,000 broken by short stretches that change every row; `mixed`, the same five in no
/// order, read from CSV so that the empty one is a missing value, 14,000 of them; and `wide`,
/// 70,000 distinct texts, whose dictionary would pass 1,048,576 bytes. Writes the frame as
/// CSV, then as Parquet, reads that back and writes it as CSV again.
const PROGRAM: &str = r#"
class Row(Struct):
    runs: str
    mixed: str | None
    wide: str


def main() -> Raises[None, OSError | ValueError]:
    words = ["AAPL", "MSFT", "BRK,B", "", "é"]
    runs: list[str] = []
    mixed: list[str] = []
    wide: list[str] = []
    for row in range(70000):
        if row % 4000 < 20:
            runs.append(words[row % 5].copy())
        else:
            runs.append(words[(row // 3000) % 5].copy())
        mixed.append(words[(row * 7 + row // 11) % 5].copy())
        wide.append(f"identifier-{row}")
    made = DataFrame({"runs": Array(runs), "mixed": Array(mixed), "wide": Array(wide)})
    made.to_csv("made.csv")
    frame: DataFrame[Row] = DataFrame.read_csv("made.csv")
    frame.to_csv("want.csv", na_rep="<none>")
    frame.to_parquet("texts.parquet")
    back: DataFrame[Row] = DataFrame.read_parquet("texts.parquet")
    back.to_csv("back.csv", na_rep="<none>")
    print(len(back))
"#;

/// The two CSV files are the same bytes, so every text and every missing value came back
/// from the dictionaries, from the runs and from the column written plainly.
#[test]
fn text_is_written_through_a_dictionary_and_read_back() {
    let directory = package("frame_parquet_dictionary", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let ran = Command::new(&executable)
        .current_dir(&*directory)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "70000\n");
    let want = fs::read(directory.join("want.csv")).unwrap();
    let back = fs::read(directory.join("back.csv")).unwrap();
    assert_eq!(String::from_utf8_lossy(&want).lines().count(), 70_001);
    assert!(back == want, "the file written was read back differently");
}
