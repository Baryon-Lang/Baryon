//! `Array.interp` (decision 0069, roadmap 3.21.1), built and run: it prints what `NumPy` 2.5's
//! `numpy.interp` gives for the same points, bit for bit, and panics with `NumPy`'s message where
//! `NumPy` raises `ValueError`.

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

/// Points below, on, between and above the samples, and NaN; `left=` and `right=`; `period=`;
/// one sample point; and two infinite values, where the first formula gives NaN and `NumPy`
/// falls back to the value itself. Every line is what `numpy.interp` gives, printed in full.
#[test]
fn interp_prints_what_numpy_prints() {
    let program = "\
import math


def show(v: Array[f64, 1]):
    line = \"\"
    for i in range(len(v)):
        line += f\"{v[i]} \"
    print(line)


def main():
    xp = Array([0.0, 1.0, 2.5, 4.0, 7.0])
    fp = Array([1.0, 3.0, -2.0, 0.5, 10.0])
    x = Array([-1.0, 0.0, 0.3, 1.0, 2.0, 3.7, 4.0, 6.9, 7.0, 9.0, math.nan])
    show(Array.interp(x, xp, fp))
    show(Array.interp(x, xp, fp, left=-5.0, right=5.0))
    show(Array.interp(x, xp, fp, period=3.0))
    show(Array.interp(x, Array([2.0]), Array([8.0])))
    show(Array.interp(Array([0.1, 0.7]), Array([0.0, 1.0]), Array([math.inf, math.inf])))
";
    let (code, stdout, report) = run("interp-numpy", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
1.0 1.0 1.6 3.0 -0.3333333333333335 4.440892098500626e-16 0.5 9.683333333333334 10.0 10.0 nan \n\
-5.0 1.0 1.6 3.0 -0.3333333333333335 4.440892098500626e-16 0.5 9.683333333333334 10.0 5.0 nan \n\
2.0 1.0 1.6 10.0 2.0 2.4000000000000004 10.0 2.8000000000000007 10.0 1.0 nan \n\
8.0 8.0 8.0 8.0 8.0 8.0 8.0 8.0 8.0 8.0 8.0 \n\
inf inf \n"
    );
}

/// Sample points and values of different lengths panic with `NumPy`'s message.
#[test]
fn interp_panics_on_points_that_do_not_fit() {
    let program = "\
def main():
    print(Array.interp(Array([0.5]), Array([0.0, 1.0]), Array([1.0, 2.0, 3.0]))[0])
";
    let (code, stdout, report) = run("interp-lengths", program).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: assertion failed: len(xp) == len(fp), \"fp and xp are not of the same length.\"\n  \
         --> <std>/arrays.byn:246:5\n"
    );
}
