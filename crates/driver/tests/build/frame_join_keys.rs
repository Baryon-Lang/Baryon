//! `merge` with `left_on=`, `right_on=` and `suffixes=` (roadmap 3.13.15, decision 0045),
//! built and run, with the printed frames those pandas 3.0.5 prints for the same joins,
//! `<NA>` where pandas shows `NaN` for a missing value.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    Ok((
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
    ))
}

/// The frames a key with two names is joined into. Both key columns are kept, each among
/// its frame's columns, and in a left, right or outer join each may hold `None` like any
/// column that is no key. A key with one name in a list beside one with two stays one
/// column. `left_on=` and `right_on=` of one name are `on=`. `suffixes=` replaces `_x` and
/// `_y`, as a tuple or a list, and `""` or `None` leaves a name as it is.
const PROGRAM: &str = r#"
def main():
    l = DataFrame({
        "sym": Array(["a", "b", "c"]),
        "px": Array([1.0, 2.0, 3.0]),
        "n": Array([1, 2, 3]),
    })
    r = DataFrame({
        "ticker": Array(["b", "c", "d"]),
        "px": Array([20.0, 30.0, 40.0]),
        "n": Array([5, 6, 7]),
    })
    print(l.merge(r, left_on="sym", right_on="ticker"))
    print(l.merge(r, left_on="sym", right_on="ticker", how="left"))
    print(l.merge(r, left_on="sym", right_on="ticker", how="right"))
    print(l.merge(r, left_on="sym", right_on="ticker", how="outer"))
    print(l.merge(r, left_on="sym", right_on="ticker", suffixes=("_l", "_r")))
    print(l.merge(r, left_on=["sym", "n"], right_on=["ticker", "n"], how="outer"))
    r2 = r.rename(columns={"ticker": "sym"})
    print(l.merge(r2, left_on="sym", right_on="sym", how="outer"))
    print(l.merge(r2, on="sym", suffixes=("", "_r")))
    print(l.merge(r2, on="sym", suffixes=[None, "_r"]))
"#;

/// What [`PROGRAM`] prints.
const EXPECTED: &[&str] = &[
    "  sym  px_x  n_x ticker  px_y  n_y\n",
    "0   b   2.0    2      b  20.0    5\n",
    "1   c   3.0    3      c  30.0    6\n",
    "  sym  px_x  n_x ticker  px_y   n_y\n",
    "0   a   1.0    1   <NA>  <NA>  <NA>\n",
    "1   b   2.0    2      b  20.0     5\n",
    "2   c   3.0    3      c  30.0     6\n",
    "    sym  px_x   n_x ticker  px_y  n_y\n",
    "0     b   2.0     2      b  20.0    5\n",
    "1     c   3.0     3      c  30.0    6\n",
    "2  <NA>  <NA>  <NA>      d  40.0    7\n",
    "    sym  px_x   n_x ticker  px_y   n_y\n",
    "0     a   1.0     1   <NA>  <NA>  <NA>\n",
    "1     b   2.0     2      b  20.0     5\n",
    "2     c   3.0     3      c  30.0     6\n",
    "3  <NA>  <NA>  <NA>      d  40.0     7\n",
    "  sym  px_l  n_l ticker  px_r  n_r\n",
    "0   b   2.0    2      b  20.0    5\n",
    "1   c   3.0    3      c  30.0    6\n",
    "    sym  px_x  n ticker  px_y\n",
    "0     a   1.0  1   <NA>  <NA>\n",
    "1     b   2.0  2   <NA>  <NA>\n",
    "2  <NA>  <NA>  5      b  20.0\n",
    "3     c   3.0  3   <NA>  <NA>\n",
    "4  <NA>  <NA>  6      c  30.0\n",
    "5  <NA>  <NA>  7      d  40.0\n",
    "  sym  px_x   n_x  px_y   n_y\n",
    "0   a   1.0     1  <NA>  <NA>\n",
    "1   b   2.0     2  20.0     5\n",
    "2   c   3.0     3  30.0     6\n",
    "3   d  <NA>  <NA>  40.0     7\n",
    "  sym   px  n  px_r  n_r\n",
    "0   b  2.0  2  20.0    5\n",
    "1   c  3.0  3  30.0    6\n",
    "  sym   px  n  px_r  n_r\n",
    "0   b  2.0  2  20.0    5\n",
    "1   c  3.0  3  30.0    6\n",
];

#[test]
fn keys_with_two_names_are_joined() {
    let (code, stdout, report) = run("frames-joined-two-names", PROGRAM).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, EXPECTED.concat());
}
