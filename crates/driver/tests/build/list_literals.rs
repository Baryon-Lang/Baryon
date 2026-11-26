//! List literals built and run: a literal of constants is copied from a table of them, any
//! other stores each element (roadmap 1.9.74). Either way the list owns a buffer of its own.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// Literals of integers, floats and `bool`s, which come from tables, and of conversions, `None`
/// and complex numbers, which are stored one by one, print what CPython prints, with `i32(n)`,
/// `u8(n)` and `f32(x)` read as `n` and `x`. A literal run twice, or written twice, makes a new
/// list each time, so changing one leaves the others alone.
#[test]
fn list_literals_build_and_run() {
    let program = r"
def main():
    for round in range(2):
        counts = [3, -1, 4, 1, 5]
        counts.append(9)
        counts[0] = 10 * round
        print(counts)
    print([i32(-7), i32(8)], [u8(255), u8(0)])
    print([0.5, -2.25, 1e300], [f32(0.5), f32(-1.5)])
    print([True, False, True])
    print([1, None, 3], [None, None, 2])
    print([1 + 2j, -0.5j])
    same = [1.5, 2.5]
    again = [1.5, 2.5]
    again[1] = 0.0
    print(same, again)
";
    let directory = package("list-literals", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[0, -1, 4, 1, 5, 9]\n[10, -1, 4, 1, 5, 9]\n[-7, 8] [255, 0]\n\
         [0.5, -2.25, 1e+300] [0.5, -1.5]\n[True, False, True]\n[1, None, 3] [None, None, 2]\n\
         [(1+2j), (-0-0.5j)]\n[1.5, 2.5] [1.5, 0.0]\n"
    );
}
