//! `print` and an f-string of a tuple or a list that holds an array with missing values
//! (roadmap 3.24.4, decision 0045), built and run.

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

/// Tuples and lists that hold arrays with missing values, of numbers, text and `bool`s. Each
/// array is written as it prints alone, and the rest as Python writes a tuple or a list: a
/// comma after the one item of a tuple, a `str` and a `str | None` in quotes as `repr`
/// writes them, a plain array, a nested tuple and a list as they print elsewhere. A list of
/// tuples that hold lists goes through every level, an f-string gives the same text, and
/// an empty list is `[]`.
const PROGRAM: &str = r#"
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.25])
    names: Array[str | None, 1] = Array(["a", None])
    flags: Array[bool | None, 1] = Array([True, None])
    plain = Array([1.0, 2.0])
    word: str | None = "it's"
    gone: str | None = None
    print((bids.copy(), 1))
    print((bids.copy(),))
    print([bids.copy(), bids.copy()])
    print((plain, names.copy(), "x\ty", word, gone, 2.5, True, (1, "q"), [1.5]))
    print(f"{(bids.copy(), 2)} and {[names.copy()]}")
    nested = [(bids.copy(), [flags.copy(), flags.copy()]), (bids.copy(), [flags.copy()])]
    print(nested)
    print(nested, bids, sep=" | ")
    empty: list[Array[f64 | None, 1]] = []
    print(empty, (empty, 0))
"#;

/// What [`PROGRAM`] prints.
const EXPECTED: [&str; 8] = [
    "([1.5 None 2.25], 1)\n",
    "([1.5 None 2.25],)\n",
    "[[1.5 None 2.25], [1.5 None 2.25]]\n",
    "([1. 2.], ['a' None], 'x\\ty', \"it's\", None, 2.5, True, (1, 'q'), [1.5])\n",
    "([1.5 None 2.25], 2) and [['a' None]]\n",
    "[([1.5 None 2.25], [[True None], [True None]]), ([1.5 None 2.25], [[True None]])]\n",
    "[([1.5 None 2.25], [[True None], [True None]]), ([1.5 None 2.25], [[True None]])] | [1.5 None 2.25]\n",
    "[] ([], 0)\n",
];

#[test]
fn tuples_and_lists_show_the_arrays_they_hold() {
    let (code, stdout, report) = run("holders-of-gap-arrays-shown", PROGRAM).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, EXPECTED.concat());
}
