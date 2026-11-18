//! `@` of matrices through the system BLAS, `--blas` (decision 0040, roadmap 3.10.2): the
//! program gives the same results as without the flag on matrices whose products are exact.
//! CI installs `OpenBLAS` on Linux; macOS has Accelerate.

use super::{BARYON, baryon_build, build_runtime, package, stderr};
use std::process::Command;

/// Matrices of small integers, so every product and sum is exact and BLAS must print the same
/// text as the loops: a rectangular product, views with steps and a reversed dimension, which
/// the runtime copies before calling BLAS, `f32` matrices, inner and outer lengths of zero, a
/// matrix times a vector, which stays a loop, and `x = m @ x`, which reads the old `x`.
const PROGRAM: &str = "\
def main():
    a = Array.zeros((4, 3))
    b = Array.zeros((3, 5))
    for i in range(4):
        for j in range(3):
            a[i, j] = f64(i * 3 + j) - 4.0
    for i in range(3):
        for j in range(5):
            b[i, j] = f64(i + 2 * j) - 3.0
    print(a @ b)
    print(a[::2, 1:] @ b[1:, ::-2])
    narrow = Array.zeros((2, 2), f32)
    narrow[0, 0] = f32(1.5)
    narrow[0, 1] = f32(2.0)
    narrow[1, 0] = f32(-1.0)
    narrow[1, 1] = f32(0.5)
    print(narrow @ narrow)
    print(Array.zeros((2, 0)) @ Array.zeros((0, 3)))
    print((Array.zeros((0, 2)) @ Array.zeros((2, 3))).shape)
    v = Array.zeros(3)
    v[0] = 1.0
    print(a @ v)
    m = Array.zeros((3, 3))
    x = Array.zeros((3, 3))
    for i in range(3):
        m[i, i] = 2.0
        for j in range(3):
            x[i, j] = f64(i - j)
    x = m @ x
    print(x)
";

/// What [`PROGRAM`] prints, with or without `--blas`.
const PRINTED: &str = "\
[[ 20.   2. -16. -34. -52.]
 [  2.   2.   2.   2.   2.]
 [-16.   2.  20.  38.  56.]
 [-34.   2.  38.  74. 110.]]
[[-32. -12.   8.]
 [ 46.  18. -10.]]
[[ 0.25  4.  ]
 [-2.   -1.75]]
[[0. 0. 0.]
 [0. 0. 0.]]
(0, 3)
[-4. -1.  2.  5.]
[[ 0. -2. -4.]
 [ 2.  0. -2.]
 [ 4.  2.  0.]]
";

/// The same program built with and without `--blas` prints the same exact results.
#[test]
fn blas_builds_print_what_loops_print() {
    let directory = package("blas", &[("main.byn", PROGRAM)]).unwrap();
    let (output, loops) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    build_runtime();
    let blas = directory.join("blas");
    let output = Command::new(BARYON)
        .args(["build", "--blas"])
        .arg(directory.join("package"))
        .arg(&blas)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    for program in [loops, blas] {
        let ran = Command::new(&program).output().unwrap();
        assert!(ran.status.success(), "{}", stderr(&ran));
        assert_eq!(String::from_utf8_lossy(&ran.stdout), PRINTED);
    }
}
