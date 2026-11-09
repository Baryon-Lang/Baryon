//! Defaults that are not a literal or a constant, built and run: each evaluated at every call
//! that leaves its argument out, in the callee's module (decision 0027, roadmap 1.9.13).

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Builds the package `name` from `files`, runs it, and returns what it printed, after checking
/// that the build succeeded and the program printed nothing to standard error.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
fn output(name: &str, files: &[(&str, &str)]) -> io::Result<String> {
    let directory = package(name, files)?;
    let (built, program) = baryon_build(&directory)?;
    assert!(built.status.success(), "{}", stderr(&built));
    let ran = Command::new(&program).output()?;
    assert_eq!(stderr(&ran), "");
    Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
}

/// A default array is fresh at each call, so the second `histogram(v)` starts from zeros, not
/// from what the first call added, as it would in Python. A default that calls a function
/// calls it again at each call that leaves the argument out, after the arguments passed, and
/// not at all when the argument is passed. Owned, borrowed and method parameters all work.
#[test]
fn computed_defaults_are_evaluated_at_each_call() {
    let program = "\
from math import sqrt

SCALE: Final = 3.0


def width() -> f64:
    print(\"width\")
    return 2.0


def histogram(values: Array[f64, 1], bins: Owned[Array[f64, 1]] = Array.zeros(4)) -> f64:
    bins[0] = bins[0] + values[0]
    return bins[0]


def drift(x: f64, dt: f64 = SCALE * width(), steps: i64 = 2) -> f64:
    return x + dt * f64(steps)


def total(buffer: Array[f64, 1] = Array.fill(3, 1.5)) -> f64:
    return buffer.sum()


class Grid(Struct):
    n: i64

    def spacing(self, length: f64 = sqrt(16.0)) -> f64:
        return length / f64(self.n)


def main():
    v = Array.fill(2, 5.0)
    print(histogram(v), histogram(v))
    print(drift(1.0, steps=1))
    print(drift(1.0, 0.5))
    print(total(), Grid(2).spacing())
";
    assert_eq!(
        output("computed-defaults", &[("main.byn", program)]).unwrap(),
        "5.0 5.0\nwidth\n7.0\n2.0\n4.5 2.0\n"
    );
}

/// A default names what its own module can see, a private constant and function included, even
/// when the caller is in another module. A default that raises raises from the call, and the
/// caller catches it.
#[test]
fn defaults_resolve_in_the_callee_module_and_may_raise() {
    let physics = "\
_BINS: Final = 3


def _fresh() -> Array[f64, 1]:
    return Array.zeros(_BINS)


def count(work: Owned[Array[f64, 1]] = _fresh()) -> i64:
    return len(work)
";
    let program = "\
import physics


class Negative(Exception):
    value: f64


def checked(x: f64) -> Raises[f64, Negative]:
    if x < 0.0:
        raise Negative(x)
    return x


def shifted(x: f64, y: f64 = checked(-1.0)) -> Raises[f64, Negative]:
    return x + y


def main():
    print(physics.count())
    try:
        print(shifted(1.0))
    except Negative as error:
        print(error.value)
    try:
        print(shifted(1.0, 2.0))
    except Negative:
        print(\"unreachable\")
";
    assert_eq!(
        output(
            "defaults-across-modules",
            &[("main.byn", program), ("physics.byn", physics)]
        )
        .unwrap(),
        "3\n-1.0\n3.0\n"
    );
}
