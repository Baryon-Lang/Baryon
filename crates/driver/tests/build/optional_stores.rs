//! Assigning elements of an array with missing values (roadmap 3.13.6, decision 0045), built
//! and run: `bids[i] = 2.0` puts a number where there was a number or a `None`,
//! `bids[i] = None` takes one out, and `bids[mask] = None` does either to every element a
//! mask, positions, a list or a slice chooses.

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

/// An element of floats, of integers and of `bool`s takes a value, a `None`, another
/// element and a `T | None`, at a position counted from either end and past the first
/// byte of the bitmap. The reductions and arithmetic see the change, and a function that
/// takes the array `InOut` changes the caller's.
#[test]
fn elements_are_assigned() -> io::Result<()> {
    let program = r"
def clear(values: InOut[Array[f64 | None, 1]], at: int):
    values[at] = None


def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0, None, 6.0, 7.0, 8.0, 9.0, 10.0])
    bids[1] = 2.0
    bids[0] = None
    bids[-1] = None
    bids[8] = bids[2]
    bids[3] = bids[4]
    print(bids, bids.count(), bids.sum())
    clear(bids, 5)
    maybe: f64 | None = 0.5
    bids[9] = maybe
    print(bids)
    counts: Array[i64 | None, 1] = Array([None, 2, 3])
    counts[0] = 7
    counts[2] = None
    print(counts, counts.sum(), counts + counts)
    flags: Array[bool | None, 1] = Array([True, None, False])
    flags[1] = True
    flags[0] = None
    print(flags, flags.sum(), flags.all())
    for at in range(len(counts)):
        counts[at] = at * at if at != 1 else None
    print(counts)
";
    let (code, output, report) = run("optional_stores_elements", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[None 2.0 2.5 None None 6.0 7.0 8.0 2.5 None] 6 28.0\n\
         [None 2.0 2.5 None None None 7.0 8.0 2.5 0.5]\n\
         [7 2 None] 9 [14 4 None]\n\
         [None True False] 1 False\n\
         [0 None 4]\n"
    );
    Ok(())
}

/// A position outside the array stops the program, and nothing is written.
#[test]
fn a_position_outside_panics() -> io::Result<()> {
    let program = r"
def main():
    bids: Array[f64 | None, 1] = Array([1.5, None])
    bids[2] = 1.0
";
    let (code, output, report) = run("optional_stores_range", program)?;
    assert_eq!(code, Some(101));
    assert_eq!(output, "");
    assert!(
        report.contains("panic: index 2 is out of range for dimension 0 of length 2"),
        "{report}"
    );
    Ok(())
}

/// A mask, a mask that may hold `None`, a slice with a step, an array of positions and a
/// list each choose the elements that take one value or `None`, in an array of floats,
/// `bool`s and integers and through an `InOut` parameter.
#[test]
fn chosen_elements_are_assigned() -> io::Result<()> {
    let program = r"
def clear(values: InOut[Array[f64 | None, 1]]):
    values[values > 2.0] = None


def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0, None, 6.0, 7.0, 8.0, 9.0, 10.0])
    bids[bids > 7.5] = 0.0
    print(bids)
    bids[bids.isna()] = -1.0
    print(bids)
    bids[1:4] = None
    print(bids)
    bids[::-3] = 5.0
    print(bids)
    bids[Array([0, 9])] = None
    bids[[2, 2]] = 3.0
    print(bids, bids.count())
    clear(bids)
    print(bids)
    flags: Array[bool | None, 1] = Array([True, None, False])
    flags[[True, True, False]] = None
    maybe: bool | None = True
    flags[2:] = maybe
    print(flags)
    counts: Array[i64 | None, 1] = Array([1, 2, 3])
    counts[counts == 2] = None
    print(counts)
    counts[:] = 7
    print(counts)
";
    let (code, output, report) = run("optional_stores_chosen", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "[1.5 None 2.5 4.0 None 6.0 7.0 0.0 0.0 0.0]\n\
         [1.5 -1.0 2.5 4.0 -1.0 6.0 7.0 0.0 0.0 0.0]\n\
         [1.5 None None None -1.0 6.0 7.0 0.0 0.0 0.0]\n\
         [5.0 None None 5.0 -1.0 6.0 5.0 0.0 0.0 5.0]\n\
         [None None 3.0 5.0 -1.0 6.0 5.0 0.0 0.0 None] 7\n\
         [None None None None -1.0 None None 0.0 0.0 None]\n\
         [None None True]\n\
         [1 None 3]\n\
         [7 7 7]\n"
    );
    Ok(())
}

/// A position outside the array and a mask of another length stop the program.
#[test]
fn a_bad_choice_panics() -> io::Result<()> {
    let start = "def main():\n    counts: Array[i64 | None, 1] = Array([1, 2, 3])\n";
    let past = format!("{start}    counts[[5]] = 1\n");
    let (code, _, report) = run("optional_stores_past", &past)?;
    assert_eq!(code, Some(101));
    assert!(
        report.contains("panic: index 5 is out of range for dimension 0 of length 3"),
        "{report}"
    );
    let short = format!("{start}    counts[[True]] = 1\n");
    let (code, _, report) = run("optional_stores_short", &short)?;
    assert_eq!(code, Some(101));
    assert!(
        report.contains("panic: array lengths differ in dimension 0: 3 and 1"),
        "{report}"
    );
    Ok(())
}
