//! A `list` that chooses elements of a rank-1 array (roadmap 3.13.6, language reference,
//! "Choosing elements"), built and run: `a[[0, 2]]` is `a[Array([0, 2])]`, as in `NumPy`.

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

/// A list of positions and a list of `bool`s, written out or held in a name or a parameter,
/// choose elements of an array of numbers, of text and of either with missing values, and
/// say which elements an assignment sets. The list stays usable.
#[test]
fn lists_choose_elements() -> io::Result<()> {
    let program = r#"
def pick(a: Array[f64, 1], spots: list[int]) -> Array[f64, 1]:
    return a[spots]


def main():
    a = Array([1.0, 2.0, 3.0])
    print(a[[0, 2]], a[[2, 2, 0]])
    print(pick(a, [1]), pick(a, []))
    print(a[[True, False, True]])
    names = Array(["a", "b", "c"])
    print(names[[2, 0]])
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    print(bids[[1, 2]], bids[[False, True, True]])
    words: Array[str | None, 1] = Array(["x", None])
    print(words[[1, 0, 0]])
    a[[0, 1]] = 9.0
    print(a)
    spots = [0, 2]
    a[spots] = 7.0
    print(a, len(spots))
"#;
    let (code, output, report) = run("list_choosers_elements", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[1. 3.] [3. 3. 1.]\n\
         [2.] []\n\
         [1. 3.]\n\
         ['c' 'a']\n\
         [None 2.5] [None 2.5]\n\
         [None 'x' 'x']\n\
         [9. 9. 3.]\n\
         [7. 9. 7.] 2\n"
    );
    Ok(())
}

/// A position in a list is checked as one in an array of positions is, and a list of
/// `bool`s must be as long as the array.
#[test]
fn a_bad_list_panics() -> io::Result<()> {
    let past = "def main():\n    a = Array([1.0, 2.0, 3.0])\n    print(a[[3]])\n";
    let (code, _, report) = run("list_choosers_past", past)?;
    assert_eq!(code, Some(101));
    assert!(
        report.contains("panic: index 3 is out of range for dimension 0 of length 3"),
        "{report}"
    );
    let short = "def main():\n    a = Array([1.0, 2.0, 3.0])\n    print(a[[True]])\n";
    let (code, _, report) = run("list_choosers_short", short)?;
    assert_eq!(code, Some(101));
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 3 and 1"),
        "{report}"
    );
    Ok(())
}
