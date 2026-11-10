//! Assigning into a list slice, `xs[i:j] = ys` (roadmap 1.13.13), built and run: the list grows
//! or shrinks as Python's does, and a range past the end panics as a slice read does.

use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// Replacing a range with more, fewer or no elements, inserting with an empty range, assigning
/// a list into a slice of itself, and a list of `str`s whose values stay usable. Each line is
/// what CPython prints for the same program.
#[test]
fn list_slice_assignment_builds_and_runs() {
    let program = r#"
def main():
    xs = [1, 2, 3, 4, 5]
    xs[1:3] = [9, 8, 7]
    print(xs)
    xs[2:2] = [0]
    print(xs)
    xs[:2] = []
    print(xs)
    xs[-2:] = [6, 6, 6]
    print(xs)
    xs[1:2] = xs
    print(xs)
    names = ["neon", "argon", "xenon"]
    extra = ["helium"]
    names[0:1] = extra
    names[3:] = ["radon", "krypton"]
    print(names, extra)
    weights = [1.5]
    weights[0:0] = [0.5, 1.0]
    print(weights)
"#;
    let directory = package("list-slice-assignment", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[1, 9, 8, 7, 4, 5]\n[1, 9, 0, 8, 7, 4, 5]\n[0, 8, 7, 4, 5]\n[0, 8, 7, 6, 6, 6]\n\
         [0, 0, 8, 7, 6, 6, 6, 7, 6, 6, 6]\n\
         ['helium', 'argon', 'xenon', 'radon', 'krypton'] ['helium']\n[0.5, 1.0, 1.5]\n"
    );
}

/// A range past the end panics, where Python would shorten it, as for a slice read.
#[test]
fn a_list_slice_past_the_end_panics() {
    let program = "def main():\n    xs = [1, 2, 3]\n    xs[2:9] = [1]\n";
    let directory = package("list-slice-assignment-panic", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: slice 2:9 is out of range for dimension 0 of length 3\n  --> main.byn:3:8\n"
    );
}
