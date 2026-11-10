//! Python's everyday built-ins built and run (roadmap 1.13.13): `enumerate`, `zip` and
//! `reversed` in loops and comprehensions. Each program prints what CPython prints for the same
//! program, with `Array(...)` read as `list(...)`.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// `enumerate` with and without a start, `zip` stopping at its shortest sequence, and
/// `reversed` of a list, an array, a tuple and `range` with positive and negative steps,
/// nested in each other, over `str`s that the loop borrows, and in comprehensions.
#[test]
fn enumerate_zip_and_reversed_build_and_run() {
    let program = r#"
def main():
    counts = [3, 1, 2]
    names = ["argon", "neon", "xenon"]
    masses = Array([39.948, 20.180, 131.29, 4.0026])
    for i, name in enumerate(names, start=1):
        print(i, name)
    for count, mass in zip(counts, masses):
        print(count, mass)
    for k in reversed(range(1, 10, 3)):
        print(k)
    for k in reversed(range(10, 0, -4)):
        print(k)
    for k in reversed(range(0)):
        print("never")
    for name in reversed(names):
        print(name)
    for i, (name, letter) in enumerate(zip(names, "xy")):
        print(i, name, letter)
    print([pair for pair in zip(counts, reversed(masses))])
    print([i * c for i, c in enumerate(counts, 10)])
    for item in reversed((1, 2, 3)):
        print(item)
    for e in enumerate(counts):
        print(e)
"#;
    let directory = package("builtins-iterators", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "1 argon\n2 neon\n3 xenon\n3 39.948\n1 20.18\n2 131.29\n7\n4\n1\n2\n6\n10\nxenon\nneon\nargon\n0 argon x\n1 neon y\n[(3, 4.0026), (1, 131.29), (2, 20.18)]\n[30, 11, 24]\n3\n2\n1\n(0, 3)\n(1, 1)\n(2, 2)\n"
    );
}
