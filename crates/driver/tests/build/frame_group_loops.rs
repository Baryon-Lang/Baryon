//! The loop over a grouping (roadmap 3.13.25, decision 0045's amendment of 2026-11-19, point
//! 5), built and run: `for key, group in df.groupby("k"):`. Each pass builds a new frame
//! that the loop drops, so the second program runs under Valgrind where there is one.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// The program of `a_loop_sees_each_key_and_its_rows`.
const PROGRAM: &str = r#"
def main() -> None:
    maybe: list[f64 | None] = [1.5, None, 3.0, None, 5.0, 6.0]
    df = DataFrame(
        {
            "k": Array([2, 1, 2, 1, 3, 0]),
            "s": Array(["b", "c", "b", "a", "a", "c"]),
            "v": Array([1.5, 2.5, 3.0, 4.0, 5.0, 6.0]),
            "m": Array(maybe),
        }
    )
    for key, group in df.groupby("k"):
        print(key)
        print(group)
    for name, rows in df.groupby("s", sort=False):
        print(name, len(rows), rows.v.sum())
    for pair, part in df.groupby(["s", "k"]):
        print(pair, part.v.max())
    for one, part in df.groupby(["k"]):
        print(one, len(part))
    total = 0.0
    for key, group in df.groupby("k"):
        if key == 3:
            continue
        if key == 2:
            break
        total += group.v.sum()
    print(total)
    for key, group in df.head(0).groupby("k"):
        print(key)
"#;

/// What [`PROGRAM`] prints.
const EXPECTED: &str = r"0
   k  s    v    m
0  0  c  6.0  6.0
1
   k  s    v     m
0  1  c  2.5  <NA>
1  1  a  4.0  <NA>
2
   k  s    v    m
0  2  b  1.5  1.5
1  2  b  3.0  3.0
3
   k  s    v    m
0  3  a  5.0  5.0
b 2 4.5
c 2 8.5
a 2 9.0
('a', 1) 4.0
('a', 3) 5.0
('b', 2) 3.0
('c', 0) 6.0
('c', 1) 2.5
(0,) 1
(1,) 2
(2,) 2
(3,) 1
12.5
";

/// A loop over a grouping by one key, by a key of text with `sort=False`, by two keys and
/// by a list of one key, whose key is a tuple of one; `continue` and `break`; and a frame
/// of no rows, whose loop never runs. Each line is what pandas 3.0.5 prints for the same
/// loop, with the group's rows numbered from 0 and a missing float printed as `<NA>`.
#[test]
fn a_loop_sees_each_key_and_its_rows() {
    let directory = package("frame_group_loops", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), EXPECTED);
}

/// The program of `groups_are_dropped_once`.
const OWNED: &str = r#"
class Row(Struct):
    k: i64
    s: str
    v: f64


def biggest(frame: DataFrame[Row]) -> f64:
    return frame.v.max()


def first_big(df: DataFrame[Row]) -> str:
    for name, rows in df.groupby("s"):
        if biggest(rows) > 3.5:
            return name.copy()
    return "none"


def main() -> None:
    df: DataFrame[Row] = DataFrame(
        {
            "k": Array([2, 1, 2, 1, 3, 0]),
            "s": Array(["b", "c", "b", "a", "a", "c"]),
            "v": Array([1.5, 2.5, 3.0, 4.0, 5.0, 6.0]),
        }
    )
    print(first_big(df))
    kept: list[DataFrame[Row]] = []
    names: list[str] = []
    for name, rows in df.groupby("s"):
        kept.append(rows.copy())
        names.append(name.copy())
        for key, inner in rows.groupby("k"):
            print(name, key, biggest(inner))
        for row in rows.itertuples():
            print(row.s, row.v)
    print(names, len(kept), kept[1])
    sizes = [len(rows) for name, rows in df.groupby("s")]
    print(sizes)
    for name, rows in df.groupby("s"):
        summed = rows.groupby("k").sum(numeric_only=True)
        print(summed)
        wider = rows.copy().assign(w=rows.v * 2.0)
        print(wider.w.sum())
"#;

/// What [`OWNED`] prints.
const OWNED_OUTPUT: &str = r"a
a 1 4.0
a 3 5.0
a 4.0
a 5.0
b 2 3.0
b 1.5
b 3.0
c 0 6.0
c 1 2.5
c 2.5
c 6.0
['a', 'b', 'c'] 3    k  s    v
0  2  b  1.5
1  2  b  3.0
[2, 2, 2]
   k    v
0  1  4.0
1  3  5.0
18.0
   k    v
0  2  4.5
9.0
   k    v
0  0  6.0
1  1  2.5
17.0
";

/// Whether Valgrind can run here.
fn valgrind() -> bool {
    Command::new("valgrind")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Every way a group leaves a pass: a `return` from inside the loop, a copy kept in a list,
/// a group borrowed by a function, by a loop over its own groups and its rows, by a
/// reduction and by `assign` of a copy, and a comprehension. Under Valgrind, where there is
/// one, nothing is read after it is freed, freed twice or lost.
#[test]
fn groups_are_dropped_once() {
    let directory = package("frame_group_loops_owned", &[("main.byn", OWNED)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = if valgrind() {
        Command::new("valgrind")
            .args([
                "--quiet",
                "--error-exitcode=99",
                "--leak-check=full",
                "--errors-for-leak-kinds=definite",
                "--show-leak-kinds=definite",
            ])
            .arg(&executable)
            .output()
            .unwrap()
    } else {
        Command::new(&executable).output().unwrap()
    };
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&ran.stdout), OWNED_OUTPUT);
}

/// The program of `the_reference_shows_what_the_loop_prints`.
const SHOWN: &str = r#"
def main() -> None:
    trades = DataFrame(
        {
            "symbol": Array([7, 3, 7, 3, 9]),
            "venue": Array(["east", "west", "west", "west", "east"]),
            "price": Array([189.5, 410.0, 191.0, 405.5, 171.0]),
            "volume": Array([100, 250, 300, 50, 75]),
        }
    )
    for symbol, rows in trades.groupby("symbol"):
        print(symbol, len(rows), rows.price.mean())
    for key, rows in trades.groupby(["venue", "symbol"]):
        print(key, rows.volume.sum())
"#;

/// What [`SHOWN`] prints.
const SHOWN_OUTPUT: &str = r"3 2 407.75
7 2 190.25
9 1 171.0
('east', 7) 100
('east', 9) 75
('west', 3) 300
('west', 7) 300
";

/// The example of the reference's "Looping over groups" prints what the page shows.
#[test]
fn the_reference_shows_what_the_loop_prints() {
    let directory = package("frame_group_loops_shown", &[("main.byn", SHOWN)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), SHOWN_OUTPUT);
}
