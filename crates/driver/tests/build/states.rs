//! Reading a generic state by position (card 60, an amendment to decision 0065 point 1):
//! `len(y)` and `y[i]` in a body generic in `S: VectorSpace`, built at each state type.

use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// One `VectorSpace` function reads every element of its state, as an ODE solver's error norm
/// does, and builds at a `Fixed` vector, an `Array` and an `f64`, whose length is 1. Reading
/// `y[1]` of an `f64` panics as reading past the end of an `Array` of length 1 does.
#[test]
fn states_are_read_by_position_at_every_state_type() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def norm[S: VectorSpace](y: S) -> f64:
    total = 0.0
    for i in range(len(y)):
        total += y[i] * y[i]
    return total


def second[S: VectorSpace](y: S) -> f64:
    return y[1]


def main():
    print(norm(Vec3(1.0, 2.0, 2.0)), norm(Array([1.0, 2.0])), norm(3.0))
    print(second(Vec3(1.0, 2.0, 2.0)), second(Array([4.0, 5.0])))
    print(second(3.0))
";
    let directory = package("vector-space-states", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "9.0 5.0 9.0\n2.0 5.0\n"
    );
    assert_eq!(
        panic_report(&ran),
        "panic: index 1 is out of range for dimension 0 of length 1\n  --> main.byn:12:14\n"
    );
}

/// Only a `VectorSpace` type parameter is measured or indexed, one integer at a time, and its
/// elements are only read: a new state is built with `+`, `-` and `*`.
#[test]
fn misuses_of_a_state_are_reported() {
    let program = "\
def measure[T: Number](x: T) -> i64:
    return len(x)


def first[T: Number](x: T) -> f64:
    return x[0]


def block[S: VectorSpace](y: S) -> f64:
    return y[0:2]


def overwrite[S: VectorSpace](y: S) -> S:
    work = y + y
    work[0] = 1.0
    work[1] += 1.0
    a, work[2] = 1.0, 2.0
    return work


def main():
    print(measure(1), first(2), block(3.0), overwrite(4.0))
";
    let directory = package("vector-space-misuses", &[("main.byn", program)]).unwrap();
    let (output, _) = baryon_build(&directory).unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr(&output),
        "\
error: a value of type `T` has no length\n\
\x20--> main.byn:2:16\n\
\x20 |\n\
2 |     return len(x)\n\
\x20 |                ^ not a sequence\n\
\x20 = note: only a `VectorSpace` type parameter has a length, its number of elements\n\
\n\
error: a value of type `T` cannot be indexed\n\
\x20--> main.byn:6:12\n\
\x20 |\n\
6 |     return x[0]\n\
\x20 |            ^^^^ not a sequence\n\
\x20 = note: only a `VectorSpace` type parameter is read by position, as `y[i]`\n\
\n\
error: `S` takes one index\n\
\x20 --> main.byn:10:12\n\
\x20  |\n\
10 |     return y[0:2]\n\
\x20  |            ^^^^^^ one integer index\n\
\x20  = note: a `VectorSpace` state is read one element at a time, as `y[i]`\n\
\n\
error: the elements of a `VectorSpace` state cannot be assigned\n\
\x20 --> main.byn:15:5\n\
\x20  |\n\
15 |     work[0] = 1.0\n\
\x20  |     ^^^^^^^ assigned here\n\
\x20  = note: build a new state with `+`, `-` and `*`, as `y + h * k`\n\
\n\
error: the elements of a `VectorSpace` state cannot be assigned\n\
\x20 --> main.byn:16:5\n\
\x20  |\n\
16 |     work[1] += 1.0\n\
\x20  |     ^^^^^^^ assigned here\n\
\x20  = note: build a new state with `+`, `-` and `*`, as `y + h * k`\n\
\n\
error: the elements of a `VectorSpace` state cannot be assigned\n\
\x20 --> main.byn:17:8\n\
\x20  |\n\
17 |     a, work[2] = 1.0, 2.0\n\
\x20  |        ^^^^^^^ assigned here\n\
\x20  = note: build a new state with `+`, `-` and `*`, as `y + h * k`\n\
\n\
"
    );
}
