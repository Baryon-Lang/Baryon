//! `print` and an f-string of a tuple, a list, a dict or a `T | None` that holds an array
//! with missing values or a frame (roadmap 3.24.4 and 3.24.5, decision 0045), built and run.

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

/// A dict writes each entry as `key: value` with the array as it prints alone, a `T | None`
/// writes the array or `None`, and a frame inside a tuple, a list or a dict writes its
/// table where Python writes it, with or without missing values (roadmap 3.24.5). An empty
/// dict is `{}`.
#[test]
fn dicts_optionals_and_held_frames_are_shown() {
    let program = r#"
def pick(flag: bool) -> Array[f64 | None, 1] | None:
    if flag:
        return Array([1.5, None])
    return None


def main():
    bids: Array[f64 | None, 1] = Array([1.0, None])
    names: Array[str | None, 1] = Array(["a", None])
    df = DataFrame({"a": Array([1, 2]), "s": Array(["x", "y"])})
    gaps = DataFrame({"bids": bids.copy(), "names": names.copy()})
    print({1: bids.copy(), 2: bids.copy()})
    print({"k": (names.copy(), "t"), "j": (names.copy(), "u")})
    print({(1, "a"): [bids.copy()], (2, "b"): []})
    print(pick(True), pick(False))
    print((pick(True), 1, pick(False)))
    print([pick(False), pick(True)])
    print(f"{pick(True)} and {pick(False)}")
    print((df.copy(), 1))
    print([gaps.copy(), gaps.copy()])
    print({"f": gaps.copy()})
    print({True: {"in": bids.copy()}})
    empty: dict[str, Array[f64 | None, 1]] = {}
    print(empty, f"{empty}")
"#;
    let expected = [
        "{1: [1.0 None], 2: [1.0 None]}\n",
        "{'k': (['a' None], 't'), 'j': (['a' None], 'u')}\n",
        "{(1, 'a'): [[1.0 None]], (2, 'b'): []}\n",
        "[1.5 None] None\n",
        "([1.5 None], 1, None)\n",
        "[None, [1.5 None]]\n",
        "[1.5 None] and None\n",
        "(   a  s\n",
        "0  1  x\n",
        "1  2  y, 1)\n",
        "[   bids names\n",
        "0   1.0     a\n",
        "1  <NA>  <NA>,    bids names\n",
        "0   1.0     a\n",
        "1  <NA>  <NA>]\n",
        "{'f':    bids names\n",
        "0   1.0     a\n",
        "1  <NA>  <NA>}\n",
        "{True: {'in': [1.0 None]}}\n",
        "{} {}\n",
    ];
    let (code, stdout, report) = run("shown-entries", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}

/// The same texts for a dict and a frame that may be `None` borrowed as parameters, in
/// `print` and in an f-string, and for a list built again in each turn of a loop, where
/// every turn frees the texts it made.
#[test]
fn held_values_are_shown_from_parameters_and_in_loops() {
    let program = r#"
class Quote(Struct):
    bid: f64 | None
    size: i64


def load(flag: bool) -> DataFrame[Quote] | None:
    if flag:
        bids: Array[f64 | None, 1] = Array([1.0, None])
        return DataFrame({"bid": bids, "size": Array([3, 4])})
    return None


def show(held: dict[str, Array[i64 | None, 1]], maybe: DataFrame[Quote] | None) -> str:
    print(held, maybe)
    return f"{held}|{maybe}"


def main():
    sizes: Array[i64 | None, 1] = Array([None, 7])
    held = {"sizes": sizes}
    print(load(True))
    print(load(False))
    print(show(held, load(True)))
    print(show(held, None))
    for turn in range(2):
        print([(load(turn == 0), turn)])
"#;
    let expected = [
        "    bid  size\n",
        "0   1.0     3\n",
        "1  <NA>     4\n",
        "None\n",
        "{'sizes': [None 7]}     bid  size\n",
        "0   1.0     3\n",
        "1  <NA>     4\n",
        "{'sizes': [None 7]}|    bid  size\n",
        "0   1.0     3\n",
        "1  <NA>     4\n",
        "{'sizes': [None 7]} None\n",
        "{'sizes': [None 7]}|None\n",
        "[(    bid  size\n",
        "0   1.0     3\n",
        "1  <NA>     4, 0)]\n",
        "[(None, 1)]\n",
    ];
    let (code, stdout, report) = run("shown-entries-borrowed", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}
