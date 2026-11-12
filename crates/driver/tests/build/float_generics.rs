//! Functions generic over a float type, `[T: Float]`, built at `f32` and `f64` (roadmap
//! 3.10.1).

use super::{baryon_build, package, stderr};
use std::process::Command;

/// A function generic over `T: Float` builds at `f32` and at `f64` (roadmap 3.10.1): `math`
/// functions of `T`, `/`, `+=` and `-=` on `T`, `Array.zeros(n, T)` and a comparison of two
/// `T`s. The `f32` instance rounds to `f32`, as its last line shows.
#[test]
fn float_generic_functions_build_at_f32_and_f64() {
    let program = "\
from math import fabs, sqrt


def unit[T: Float](v: Array[T, 1]) -> Array[T, 1]:
    total = v[0] * v[0]
    for j in range(1, len(v)):
        total += v[j] * v[j]
    length = sqrt(total)
    result = Array.zeros(len(v), T)
    largest = fabs(v[0])
    for j in range(len(v)):
        result[j] = v[j] / length
        if fabs(v[j]) > largest:
            largest = fabs(v[j])
    result[0] -= result[0] * 0.0
    return result


def main():
    print(unit(Array([3.0, -4.0])))
    narrow = Array.zeros(3, f32)
    narrow[0] = f32(1.0)
    narrow[1] = f32(1.0)
    narrow[2] = f32(1.0)
    print(unit(narrow))
    print(unit(Array([1.0, 1.0, 1.0])))
";
    let directory = package("float-generics", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[ 0.6 -0.8]\n[0.57735026 0.57735026 0.57735026]\n[0.57735027 0.57735027 0.57735027]\n"
    );
}
