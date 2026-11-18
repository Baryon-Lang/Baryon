//! Choosing elements of an array with missing values (roadmap 3.13.6, decision 0045), built
//! and run: `bids[mask]`, `bids[positions]` and `bids[1:3]` give a new array with missing
//! values, a `None` staying a `None`, and a mask that may hold `None` chooses elements of a
//! text array as it does those of an array of numbers.

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

/// A plain mask, positions and a slice each choose elements of an array with missing
/// values, of numbers, of text and of `bool`s. Positions can repeat and come in any order,
/// and a slice follows Python: a step, a negative step and an empty result.
#[test]
fn masks_positions_and_slices_choose_elements() -> io::Result<()> {
    let program = r#"
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0, None])
    names: Array[str | None, 1] = Array(["ada", None, "cy", "", "bo"])
    keep = Array([True, True, False, True, False])
    order = Array([4, 0, 1, 1])
    print(bids[keep], names[keep])
    print(bids[order], names[order])
    print(bids[1:4], bids[::-1], bids[::2], bids[3:1])
    print(names[1:4], names[::-1], names[:0])
    part = bids[1:3]
    print(len(part), part.count(), part.sum(), part[0], part[1])
    flags = bids > 2.0
    print(flags[flags.notna()], flags[1:])
"#;
    let (code, output, report) = run("optional_selection_plain", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[1.5 None 4.0] ['ada' None '']\n\
         [None 1.5 None None] ['bo' 'ada' None None]\n\
         [None 2.5 4.0] [None 4.0 2.5 None 1.5] [1.5 2.5 None] []\n\
         [None 'cy' ''] ['bo' '' 'cy' None 'ada'] []\n\
         2 1 2.5 None 2.5\n\
         [False True True] [None True True None]\n"
    );
    Ok(())
}

/// A mask that may hold `None` chooses the elements where it is `True`, of an array with
/// missing values and of a text array, as it does those of an array of numbers.
#[test]
fn masks_with_gaps_choose_elements() -> io::Result<()> {
    let program = r#"
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0, None])
    names: Array[str | None, 1] = Array(["ada", None, "cy", "", "bo"])
    plain = Array(["a", "b", "c", "d", "e"])
    print(bids[bids > 2.0], bids[bids.isna()], names[names != "cy"])
    print(names[bids > 2.0], plain[bids > 2.0], plain[names == "ada"])
    bids = bids[bids > 1.0]
    print(bids)
"#;
    let (code, output, report) = run("optional_selection_gaps", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[2.5 4.0] [None None] ['ada' '' 'bo']\n\
         ['cy' ''] ['c' 'd'] ['a']\n\
         [1.5 2.5 4.0]\n"
    );
    Ok(())
}

/// Builds and runs `main`'s one statement `line` on an array of two elements and gives the
/// panic report.
fn panics(name: &str, line: &str) -> io::Result<String> {
    let program =
        format!("def main():\n    bids: Array[f64 | None, 1] = Array([1.5, None])\n    {line}\n");
    let (code, output, report) = run(name, &program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    Ok(report)
}

/// A position outside the array, a mask of another length and a slice with a step of 0
/// each stop the program, as they do for an array without missing values.
#[test]
fn bad_choices_panic() -> io::Result<()> {
    let report = panics("optional_selection_position", "print(bids[Array([0, 2])])")?;
    assert!(
        report.contains("panic: index 2 is out of range for dimension 0 of length 2"),
        "{report}"
    );
    let report = panics("optional_selection_mask", "print(bids[Array([True])])")?;
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 2 and 1"),
        "{report}"
    );
    let report = panics("optional_selection_step", "print(bids[::0])")?;
    assert!(
        report.contains("panic: slice step cannot be zero"),
        "{report}"
    );
    Ok(())
}
