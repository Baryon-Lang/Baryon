//! The lengths of arrays, `shape` and `size`, built and run.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// `a.size` and `a.shape[k]` passed straight to `print`, of an `Array` and of a
/// `Fixed` array. Before this, `print` looked for a place they name, found none, and reported
/// them as attributes lowering does not handle.
#[test]
fn shape_and_size_print_directly() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    a = Array.zeros((4, 5), dtype=f64)
    up = Vec3(0.0, 0.0, 1.0)
    print(a.size, a.shape[1], up.size, up.shape[0])
";
    let directory = package("shapes", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&run), "");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "20 5 3 3\n");
}
