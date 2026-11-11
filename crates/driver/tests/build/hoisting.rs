//! Bounds-check hoisting (roadmap 3.2), built and run: a loop whose index checks are made
//! before it gives the same results, and a loop with an index out of range panics at the same
//! element, with the same message and line, as without hoisting.

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

/// A stencil over the counter, a 2D loop over an invariant row, and a constant index all run
/// in the copy without checks, and compute what they compute with them.
#[test]
fn hoisted_loops_compute_the_same_results() {
    let program = "\
def smooth(u: Array[f64, 1], out: InOut[Array[f64, 1]]):
    for i in range(1, len(u) - 1):
        out[i] = u[i] + 0.25 * (u[i - 1] + u[i + 1] - 2.0 * u[i]) + u[0]


def scale(a: Array[f64, 2], b: InOut[Array[f64, 2]]):
    for i in range(a.shape[0]):
        for j in range(a.shape[1]):
            b[i, j] = 0.5 * b[i, j] + a[i, j]


def main():
    u = Array.zeros((9,))
    u[4] = 16.0
    out = u.copy()
    for _ in range(3):
        smooth(u, out)
        u, out = out, u
    a = Array.fill((3, 4), 1.0)
    b = Array.zeros((3, 4))
    for _ in range(2):
        scale(a, b)
    print(u)
    print(b.sum())
";
    let (code, stdout, report) = run("hoisting-results", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "[0.   0.25 1.5  3.75 5.   3.75 1.5  0.25 0.  ]\n18.0\n"
    );
}

/// A loop whose last pass reads past the end fails the checks before it, so it runs as written
/// and panics at that pass; so does a loop whose first pass reads before the start.
#[test]
fn an_index_out_of_range_panics_at_the_same_pass() {
    let past_end = "\
def main():
    a = Array.fill((5,), 1.0)
    total = 0.0
    for i in range(0, 6):
        total += a[i]
        print(total)
";
    let (code, stdout, report) = run("hoisting-past-end", past_end).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(stdout, "1.0\n2.0\n3.0\n4.0\n5.0\n");
    assert_eq!(
        report,
        "panic: index 5 is out of range for dimension 0 of length 5\n  --> main.byn:5:20\n"
    );

    let before_start = "\
def main():
    a = Array.fill((5,), 1.0)
    for i in range(1, 5):
        print(a[i - 2])
";
    let (code, stdout, report) = run("hoisting-before-start", before_start).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(stdout, "");
    assert_eq!(
        report,
        "panic: index -1 is out of range for dimension 0 of length 5\n  --> main.byn:4:17\n"
    );
}

/// A loop that runs no pass reads nothing, so an index out of range in it never panics, as it
/// never does without hoisting.
#[test]
fn a_loop_with_no_passes_never_panics() {
    let program = "\
def main():
    a = Array.fill((5,), 1.0)
    k = 10
    n = 0
    for i in range(n):
        print(a[k] + a[i])
    print(\"done\")
";
    let (code, stdout, report) = run("hoisting-no-passes", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, "done\n");
}
