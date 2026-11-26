//! The statistics `df.to_parquet` writes (roadmap 3.14.6.4, decision 0045): for each column
//! of a row group, how many values are missing and the smallest and the largest, which a
//! reader uses to pass over row groups a search cannot match.
//!
//! Baryon does not read statistics, so the test pins the bytes written against
//! `tests/fixtures/parquet/statistics.parquet`, whose statistics were compared, field by
//! field, with those polars writes for the same table. The fixtures' `README.md` says how.

use super::{baryon_build, package};
use std::fs;
use std::path::Path;
use std::process::Command;

/// The file the program must write, byte for byte.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/parquet/statistics.parquet"
);

/// Writes three rows whose statistics take every rule: floats that include a zero of each
/// sign, which must be written `-0.0` as the smallest and `0.0` as the largest; a column of
/// nothing but NaN, which has no smallest or largest; NaN beside numbers, which is left
/// out; missing values, which are counted; a whole number below zero; booleans; and text,
/// ordered by its bytes, so that `é` is after `ab`.
const PROGRAM: &str = r#"
from math import inf, nan


def main() -> Raises[None, OSError]:
    frame = DataFrame({
        "up": Array([0.0, 1.0, 2.0]),
        "down": Array([-3.0, -0.0, -1.0]),
        "nans": Array([nan, nan, nan]),
        "mixed": Array([nan, 2.0, -inf]),
        "some": Array([1.5, None, -2.5]),
        "count": Array([3, None, -7]),
        "flag": Array([True, False, True]),
        "name": Array(["b", None, "a"]),
        "word": Array(["ab", "a", "é"]),
    })
    frame.to_parquet("statistics.parquet")
    print(len(frame))
"#;

/// The file written is the fixture, so the missing counts, the smallest and the largest
/// values of its nine columns are those polars writes, and the note that each column is in
/// its type's order is there.
#[test]
fn statistics_are_those_polars_writes() {
    let directory = package("frame_parquet_statistics", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let ran = Command::new(&executable)
        .current_dir(&*directory)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "3\n");
    let written = fs::read(directory.join("statistics.parquet")).unwrap();
    let expected = fs::read(Path::new(FIXTURE)).unwrap();
    assert!(written == expected, "the bytes written have changed");
}
