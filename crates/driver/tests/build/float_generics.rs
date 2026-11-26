//! Functions generic over a float type, `[T: Float]`, built at `f32` and `f64` (roadmap
//! 3.10.1), conversions to a type parameter, `T(x)` (roadmap 1.9.59), and `**` on one (1.9.70).

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

/// A generic struct built with its type arguments written builds as one built from inferred
/// arguments (decision 0066, roadmap 1.9.61): `Box[f64]([])` starts an empty list of `f64`,
/// `Pair[f64](1, 2)` makes its integer literals `f64`, `Pair[i8](3, 4)` makes them `i8`, and a
/// generic function builds `Box[T]([])` at the `T` its argument fixes.
#[test]
fn structs_built_with_written_type_arguments_build() {
    let program = "\
class Pair[T: Number](Struct):
    a: T
    b: T


class Box[T](Struct):
    items: list[T]


def empty[T](like: T) -> Box[T]:
    return Box[T]([])


def main():
    b = Box[f64]([])
    b.items.append(2.5)
    p = Pair[f64](1, 2)
    q = Pair[i8](3, 4)
    e = empty(7)
    e.items.append(7)
    print(len(b.items), b.items, p.a + p.b, q.a + q.b, e.items)
";
    let directory = package("written_constructors", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "1 [2.5] 3.0 7 [7]\n");
}

/// `**` on a `T` under `Float` builds as the instance's `math.pow` (roadmap 1.9.70): a norm
/// raises a sum to `T(0.5)` at `f64` and `f32`, and a power of two `T`s and of a `T` and a
/// literal follow the float rules.
#[test]
fn powers_of_a_float_type_parameter_build() {
    let program = "\
def norm[T: Float](xs: Array[T, 1]) -> T:
    s = T(0)
    for x in xs:
        s += x * x
    return s ** T(0.5)


def cube[T: Float](x: T) -> T:
    return x ** 3.0


def main():
    print(norm(Array([3.0, 4.0])))
    narrow = Array.zeros(2, f32)
    narrow[0] = f32(6.0)
    narrow[1] = f32(8.0)
    print(norm(narrow))
    print(cube(2.0), cube(f32(1.5)), cube(-2.0) ** 2.0)
";
    let directory = package("float-generic-powers", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "5.0\n10.0\n8.0 3.375 64.0\n"
    );
}

/// `math.isnan`, `isinf` and `isfinite` of a `T: Float` value build at `f32` as well as `f64`:
/// the `f32` instance once compared in `f64` and failed to compile, which `eig` of an `f32`
/// matrix (roadmap 3.10.7) needs.
#[test]
fn float_classes_of_a_type_parameter_build_at_f32() {
    let program = "\
from math import isfinite, isinf, isnan


def classes[T: Float](x: T) -> tuple[bool, bool, bool]:
    return isnan(x), isinf(x), isfinite(x)


def main():
    narrow: f32 = 1.5
    print(classes(narrow), classes(f32(1e30) * f32(1e30)), classes(2.0))
";
    let directory = package("float-generic-classes", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "(False, False, True) (False, True, False) (False, False, True)\n"
    );
}
