//! Comparisons of text and of `bool`s on arrays with missing values (roadmap 3.13.6,
//! decision 0045), built and run: `==`, `!=`, `<`, `<=`, `>` and `>=` of an
//! `Array[str | None, 1]`, and `==` and `!=` of an `Array[bool | None, 1]`, give an array of
//! `bool`s that is `None` wherever a side is `None`, as pandas's nullable arrays do.

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

/// A text array with missing values compares with another, with a plain text array and
/// with one `str`, on either side, by the order Python gives `str`. The result is `None`
/// where a side is, a side can be a call, and an empty array gives an empty one.
#[test]
fn text_with_gaps_compares() -> io::Result<()> {
    let program = r#"
def names() -> Array[str | None, 1]:
    return Array(["ada", None, "émile", "bob"])


def main():
    a = names()
    b: Array[str | None, 1] = Array([None, "x", "émile", "al"])
    plain = Array(["ada", "b", "zed", "bob"])
    print(a == "ada", a != "ada")
    print(names() == names())
    print(a < b, a >= b)
    print(a > "b", "b" < a)
    print(a <= plain, plain > a)
    print(plain == a, plain != a)
    empty: Array[str | None, 1] = Array([])
    print(empty == "a", empty == empty)
"#;
    let (code, output, report) = run("optional_text_comparisons_text", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[True None False False] [False None True True]\n\
         [True None True True]\n\
         [None None False False] [None None True True]\n\
         [False None True True] [False None True True]\n\
         [True None False True] [False None False False]\n\
         [True None False True] [False None True False]\n\
         [] []\n"
    );
    Ok(())
}

/// An array of `bool`s with missing values compares by `==` and `!=` with another, with a
/// plain array and with one `bool`, and so does the result of a comparison.
#[test]
fn flags_with_gaps_compare() -> io::Result<()> {
    let program = r#"
def main():
    flags: Array[bool | None, 1] = Array([True, None, False, True])
    yes = Array([True, True, False, False])
    names: Array[str | None, 1] = Array(["ada", "bob", None, "cy"])
    print(flags == True, False == flags)
    print(flags != flags)
    print(flags == yes, yes != flags)
    print((names == "ada") == flags)
"#;
    let (code, output, report) = run("optional_text_comparisons_flags", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[True None False True] [False None True False]\n\
         [False None False False]\n\
         [True None True False] [False None False True]\n\
         [True None None False]\n"
    );
    Ok(())
}

/// A comparison of a text column that may hold `None` chooses the rows of a frame where it
/// is `True`, alone or combined with another mask, and leaves out the rows where the text
/// is `None`.
#[test]
fn text_comparisons_choose_rows() -> io::Result<()> {
    let program = r#"
def main():
    names: Array[str | None, 1] = Array(["ada", None, "émile", "bob"])
    frame = DataFrame({"name": names.copy(), "n": Array([1, 2, 3, 4])})
    print(frame[frame.name != "ada"])
    print(frame[(frame.name > "b") | (frame.n == 1)])
"#;
    let (code, output, report) = run("optional_text_comparisons_rows", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "    name  n\n\
         0  émile  3\n\
         1    bob  4\n\
        \x20   name  n\n\
         0    ada  1\n\
         1  émile  3\n\
         2    bob  4\n"
    );
    Ok(())
}

/// Two text arrays of different lengths panic when the program runs, as two arrays of
/// numbers do.
#[test]
fn text_lengths_must_match() -> io::Result<()> {
    let program = r#"
def main():
    a: Array[str | None, 1] = Array(["ada", None])
    b: Array[str | None, 1] = Array(["ada"])
    print(a == b)
"#;
    let (code, output, report) = run("optional_text_comparisons_length", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 2 and 1"),
        "{report}"
    );
    Ok(())
}
