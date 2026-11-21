//! `df.dropna(how=, subset=)` (roadmap 3.13.6, decision 0045 part 3), built and run: the
//! rows kept match pandas's for the same frame, and the columns that lost their `None` read
//! as plain arrays.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// A frame with a column that cannot hold `None` and three that can, one of them text,
/// dropped by each rule and each choice of columns.
const PROGRAM: &str = r#"
def main():
    bid: Array[f64 | None, 1] = Array([1.5, None, None, 4.0, None])
    ask: Array[f64 | None, 1] = Array([2.0, 3.0, None, None, None])
    name: Array[str | None, 1] = Array(["a", None, None, "d", "e"])
    df = DataFrame({"id": Array([1, 2, 3, 4, 5]), "bid": bid, "ask": ask, "name": name})
    print(df.dropna(how="any"))
    print(df.dropna(how="all"))
    print(df.dropna(subset=["bid"]))
    print(df.dropna(subset="ask"))
    print(df.dropna(subset=["bid", "ask"]))
    print(df.dropna(how="all", subset=["bid", "ask"]))
    print(df.dropna(how="all", subset=["bid", "ask", "name"]))
    print(df.dropna(subset=["id"]))
    print(df.dropna(how="all", subset=["id", "bid"]))
    kept = df.dropna(subset=["bid"])
    doubled: Array[f64, 1] = kept.bid * 2.0
    print(doubled)
    print(kept.ask)
    print(df.dropna(subset=["bid", "ask"]).dropna(how="all", subset="name"))
"#;

/// `how="any"` drops a row with a `None` in any column looked at and `how="all"` one with
/// `None` in all of them; `subset=` takes one name or a list. A call that can drop nothing,
/// because it looks at a column that cannot hold `None`, gives every row. A column looked
/// at under `how="any"` reads as a plain array afterwards, and the others keep their gaps.
/// The rows are pandas's for the same frame.
#[test]
fn rows_are_dropped_by_rule_and_subset() -> io::Result<()> {
    let directory = package("missing_dropped", &[("main.byn", PROGRAM)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED);
    Ok(())
}

/// What [`PROGRAM`] prints.
const EXPECTED: &str = "\
\x20  id  bid  ask name\n\
0   1  1.5  2.0    a\n\
\x20  id   bid   ask  name\n\
0   1   1.5   2.0     a\n\
1   2  <NA>   3.0  <NA>\n\
2   3  <NA>  <NA>  <NA>\n\
3   4   4.0  <NA>     d\n\
4   5  <NA>  <NA>     e\n\
\x20  id  bid   ask name\n\
0   1  1.5   2.0    a\n\
1   4  4.0  <NA>    d\n\
\x20  id   bid  ask  name\n\
0   1   1.5  2.0     a\n\
1   2  <NA>  3.0  <NA>\n\
\x20  id  bid  ask name\n\
0   1  1.5  2.0    a\n\
\x20  id   bid   ask  name\n\
0   1   1.5   2.0     a\n\
1   2  <NA>   3.0  <NA>\n\
2   4   4.0  <NA>     d\n\
\x20  id   bid   ask  name\n\
0   1   1.5   2.0     a\n\
1   2  <NA>   3.0  <NA>\n\
2   4   4.0  <NA>     d\n\
3   5  <NA>  <NA>     e\n\
\x20  id   bid   ask  name\n\
0   1   1.5   2.0     a\n\
1   2  <NA>   3.0  <NA>\n\
2   3  <NA>  <NA>  <NA>\n\
3   4   4.0  <NA>     d\n\
4   5  <NA>  <NA>     e\n\
\x20  id   bid   ask  name\n\
0   1   1.5   2.0     a\n\
1   2  <NA>   3.0  <NA>\n\
2   3  <NA>  <NA>  <NA>\n\
3   4   4.0  <NA>     d\n\
4   5  <NA>  <NA>     e\n\
[3. 8.]\n\
[2.0 None]\n\
\x20  id  bid  ask name\n\
0   1  1.5  2.0    a\n\
";
