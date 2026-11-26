//! `.copy()` on a value whose type is a type parameter, built and run (roadmap 1.9.60, decision
//! 0066): each type a generic function copies gives a value of its own, so changing the copy
//! leaves the original alone.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// Copies through `T` of numbers, `bool`s, `str`s, lists, an array, an array view and a struct
/// with its own `copy`, which runs that method; and through a bound `T`, in a loop variable.
#[test]
fn copies_through_a_type_parameter_build_and_run() {
    let program = r#"
class Point(Struct):
    x: f64
    y: f64

    def copy(self) -> Point:
        return Point(self.x + 100.0, self.y)


def last[T](xs: list[T]) -> T:
    return xs[len(xs) - 1].copy()


def twice[T](x: T) -> list[T]:
    return [x.copy(), x.copy()]


def scaled[T: Number](xs: list[T]) -> T:
    total = xs[0].copy()
    for x in xs:
        kept = x.copy()
        total = total + kept
    return total


def main():
    print(last([1.5, 2.5]), last(["a", "b"]), twice(7), twice(True), scaled([1, 2, 3]))
    names = ["x", "y"]
    pair = twice(names)
    pair[0].append("z")
    print(pair, names)
    values = Array([1.0, 2.0, 3.0, 4.0])
    copies = twice(values)
    copies[0][0] = 9.0
    print(copies[0], copies[1], values)
    print(twice(values[::2])[1])
    p = twice(Point(1.0, 2.0))
    print(p[0].x, p[1].y)
"#;
    let directory = package("parameter-copies", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "2.5 b [7, 7] [True, True] 7\n[['x', 'y', 'z'], ['x', 'y']] ['x', 'y']\n\
         [9. 2. 3. 4.] [1. 2. 3. 4.] [1. 2. 3. 4.]\n[1. 3.]\n101.0 2.0\n"
    );
}
