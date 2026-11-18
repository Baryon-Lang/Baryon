//! `**` on whole arrays (roadmap 1.9.58), built and run: each element's power, as `**` on two
//! numbers gives it and as `NumPy` prints it, with integer powers checked for overflow and a
//! negative integer exponent a panic.

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

/// Float, integer and complex arrays to a number, a number to an array, an array to an array,
/// a power inside a sum, a broadcast matrix, `**=` on an array and on a slice, and a power in a
/// `prange` loop: every line is what `NumPy` prints. Before, lowering stopped on "`**` on
/// arrays" and "`**=`".
#[test]
fn powers_of_arrays_print_what_numpy_prints() {
    let program = "\
def main():
    x = Array([1.0, 2.0, 3.0])
    print(x ** 2, x ** 0.5, 2.0 ** x)
    n = Array([1, 2, 3, 4])
    print(n ** 2, n ** n, 2 ** n, n ** 0.5)
    print(((x - 2.0) ** 2).sum())
    m: Array[f64, 2] = Array.zeros((2, 2))
    m[0, 1] = 3.0
    print(m ** Array([1.0, 2.0]))
    x **= 2
    x[1:] **= 0.5
    n **= 2
    print(x, n)
    total = 0.0
    for i in prange(3):
        total += (x ** 2)[i]
    print(total)
    z = Array([1j, 2.0 + 0j])
    print(z ** 2)
";
    let (code, stdout, report) = run("array_powers", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
[1. 4. 9.] [1.         1.41421356 1.73205081] [2. 4. 8.]
[ 1  4  9 16] [  1   4  27 256] [ 2  4  8 16] [1.         1.41421356 1.73205081 2.        ]
2.0
[[0. 9.]
 [0. 0.]]
[1. 2. 3.] [ 1  4  9 16]
14.0
[-1.+0.j  4.+0.j]
"
    );
}

/// An integer power that overflows panics naming `**`, and a negative integer exponent panics
/// naming it, as `**` on two integers does.
#[test]
fn integer_powers_of_arrays_are_checked() {
    let overflow = "\
def main():
    big = Array([10, 4000000000])
    print(big ** 2)
";
    let (code, _, report) = run("array_power_overflow", overflow).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: integer overflow in `**`\n  --> main.byn:3:11\n"
    );
    let negative = "\
def main():
    n = Array([1, 2, 3])
    print(n ** Array([1, -1, 2]))
";
    let (code, _, report) = run("array_power_negative", negative).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: negative exponent -1 in integer `**`\n  --> main.byn:3:11\n"
    );
}
