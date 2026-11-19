//! Functions generic over a float type, `[T: Float]`, built at `f32` and `f64` (roadmap
//! 3.10.1), and conversions to a type parameter, `T(x)` (roadmap 1.9.59).

use super::{baryon_build, package, panic_report, stderr};
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

/// `T(x)` under `Number` or `Float` builds as the concrete conversion for each type argument
/// (decision 0066, roadmap 1.9.59): a mean divides by `T(len(xs))` at `f64` and `f32`, an
/// integer converts to `i8`, `f32` and `complex128`, a float to `i32` rounding toward zero, and
/// `T(300)` at `i8` panics as `i8(300)` does.
#[test]
fn conversions_to_a_type_parameter_build_as_concrete_conversions() {
    let program = "\
def mean[T: Float](xs: Array[T, 1]) -> T:
    s = T(0)
    for x in xs:
        s += x
    return s / T(len(xs))


def shifted[T: Number](like: T, n: i64) -> T:
    return like + T(n)


def halved[T: Number](like: T, x: f64) -> T:
    return like + T(x / 2.0)


def main():
    a = Array([1.0, 2.0, 4.0])
    b: Array[f32, 1] = Array.zeros((3,))
    b[0] = f32(1.5)
    print(mean(a), mean(b))
    print(shifted(i8(0), 100), shifted(f32(0), 3), shifted(1j, 2), halved(i32(0), 7.0))
    print(shifted(i8(0), 300))
";
    let directory = package("parameter_conversions", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "2.3333333333333335 0.5\n100 3.0 (2+1j) 3\n"
    );
    assert_eq!(
        panic_report(&ran),
        "panic: the value does not fit in `i8`\n  --> main.byn:9:19\n"
    );
}
