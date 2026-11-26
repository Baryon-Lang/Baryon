//! The quick way `DataFrame.read_csv` reads a float (roadmap 3.14.4.4), built and run: a
//! cell that is a plain decimal is read from its bytes with no copy, and every other cell
//! through `float`, and both give the float nearest the text, which is what Python's
//! `float` gives for each cell here.

use super::{baryon_build, package, stderr};
use std::fs;
use std::process::Command;

/// Reads `cells.csv` and writes it back, prints the signs of its first two values, and
/// prints the rows or the error of each of five files with one cell that is no number.
const PROGRAM: &str = "\
from math import copysign


class Row(Struct):
    v: f64


def rows(path: str) -> Raises[int, OSError | ValueError]:
    cells: DataFrame[Row] = DataFrame.read_csv(path)
    return len(cells)


def main() -> Raises[None, OSError | ValueError]:
    cells: DataFrame[Row] = DataFrame.read_csv(\"cells.csv\")
    cells.to_csv(\"back.csv\")
    print(copysign(1.0, cells.v[0]), copysign(1.0, cells.v[1]))
    for name in [\"sign\", \"point\", \"both\", \"points\", \"middle\"]:
        try:
            print(rows(name + \".csv\"))
        except ValueError as error:
            print(error.message)
";

/// Cells at every edge of the quick way: a zero below zero with and without a point; no
/// digit before the point or after it; leading zeros; 2 to the 53, the greatest whole
/// number of digits it takes, and one more, which `float` rounds; decimals no float holds
/// exactly; an exponent, which it leaves to `float`; 22 places after the point, the most it
/// divides by, and 23; twenty digits; a plus sign; spaces; seventeen significant digits; and
/// the greatest float.
const CELLS: &str = "v\n-0\n-0.0\n.5\n5.\n-5.\n007.50\n9007199254740992\n9007199254740993\n\
    -9007199254740992.0\n0.1\n0.3\n123456.789\n1e22\n0.0000000000000000000001\n\
    0.00000000000000000000001\n-.25\n12345678901234567890\n+3.25\n 2.5 \n\
    0.30000000000000004\n1.7976931348623157e308\n";

/// What `to_csv` writes for the floats Python's `float` reads from `CELLS`, each in its
/// shortest text that reads back exactly.
const BACK: &str = "v\n-0.0\n-0.0\n0.5\n5.0\n-5.0\n7.5\n9007199254740992.0\n\
    9007199254740992.0\n-9007199254740992.0\n0.1\n0.3\n123456.789\n1e+22\n1e-22\n1e-23\n\
    -0.25\n1.2345678901234567e+19\n3.25\n2.5\n0.30000000000000004\n\
    1.7976931348623157e+308\n";

/// Every cell reads as the float nearest its text, the zeros below zero stay below it, and
/// a sign or a point with no digit, two points and a sign in the middle are each the same
/// `ValueError` as before, naming the cell.
#[test]
fn plain_decimals_read_as_float_reads_them() {
    let files = [
        ("main.byn", PROGRAM),
        ("cells.csv", CELLS),
        ("sign.csv", "v\n1\n-\n"),
        ("point.csv", "v\n.\n"),
        ("both.csv", "v\n-.\n"),
        ("points.csv", "v\n1.2.3\n"),
        ("middle.csv", "v\n1-2\n"),
    ];
    let directory = package("csv_decimals", &files).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable)
        .current_dir(directory.join("package"))
        .output()
        .unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "-1.0 -1.0\n\
         column 'v', row 2: could not read '-' as f64\n\
         column 'v', row 1: could not read '.' as f64\n\
         column 'v', row 1: could not read '-.' as f64\n\
         column 'v', row 1: could not read '1.2.3' as f64\n\
         column 'v', row 1: could not read '1-2' as f64\n"
    );
    let back = fs::read_to_string(directory.join("package").join("back.csv")).unwrap();
    assert_eq!(back, BACK);
}
