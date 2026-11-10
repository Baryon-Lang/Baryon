//! The statements roadmap item 1.9.17 builds, run end to end: `for` over arrays and tuples, a
//! `range` step computed at run time, integer `**` and `**=`, and nested unpacking.

use super::{BARYON, baryon_build, build_runtime, package, panic_report, stderr};
use std::io;
use std::process::{Command, Output};

/// Builds and runs `main.byn` holding `program` as the package `name`, in a debug build, or a
/// release build when `release` is set.
fn run(name: &str, program: &str, release: bool) -> io::Result<Output> {
    let directory = package(name, &[("main.byn", program)])?;
    if release {
        build_runtime();
        return Command::new(BARYON)
            .args(["run", "--release"])
            .arg(directory.join("package"))
            .output();
    }
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    Command::new(&program).output()
}

/// The text of standard output.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `for` over an `Array`, a `Fixed` vector, an array of `Fixed` points unpacked in the loop
/// target, a tuple display and a tuple of tuples; `break` and `continue` behave as in a `range`
/// loop, and assigning the loop variable does not change the next element.
#[test]
fn for_loops_go_over_arrays_and_tuples() {
    let program = "\
Vec3 = Fixed[f64, (3,)]
Point = Fixed[f64, (2,)]


def total(values: Array[f64, 1]) -> f64:
    sum = 0.0
    for v in values:
        if v < 0.0:
            continue
        if v > 100.0:
            break
        sum += v
        v = 1000.0
    return sum


def main():
    a = Array.zeros(5, dtype=f64)
    for i in range(5):
        a[i] = f64(i) - 1.0
    a[4] = 500.0
    print(total(a))
    up = Vec3(1.0, 2.0, 3.0)
    product = 1.0
    for x in up:
        product *= x
    print(product)
    points = Array.fill(3, Point(1.0, 2.0))
    points[2] = Point(3.0, 4.0)
    for x, y in points:
        print(x, y)
    for n in (2, 3, 5):
        print(n * n)
    for name, count in ((1, 10), (2, 20)):
        print(name, count)
";
    let ran = run("for-collections", program, false).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        stdout(&ran),
        "3.0\n6.0\n1.0 2.0\n1.0 2.0\n3.0 4.0\n4\n9\n25\n1 10\n2 20\n"
    );
}

/// A `range` step that is a variable counts up or down by its sign, and a zero step panics,
/// where Python raises `ValueError`.
#[test]
fn a_range_step_may_be_computed() {
    let program = "\
def count(start: i64, stop: i64, step: i64) -> i64:
    n = 0
    for i in range(start, stop, step):
        n += 1
    return n


def main():
    print(count(0, 10, 3), count(10, 0, -3), count(0, 10, -1), count(5, 5, 2))
    print(count(0, 10, 0))
";
    let ran = run("range-step", program, false).unwrap();
    assert_eq!(stdout(&ran), "4 4 0 0\n");
    assert_eq!(ran.status.code(), Some(101), "{}", stderr(&ran));
    assert_eq!(
        panic_report(&ran),
        "panic: `range` step is zero\n  --> main.byn:3:33\n"
    );
}

/// Integer `**` and `**=` give exact results, `0 ** 0` is 1, a negative exponent panics naming
/// it, and a result that does not fit panics as `**` in a debug build.
#[test]
fn integer_power_is_exact_and_checked() {
    let program = "\
def power(base: i64, exponent: i64) -> i64:
    return base ** exponent


def main():
    x = 3
    x **= 4
    print(power(2, 10), power(-3, 3), power(0, 0), power(-2, 63), x, i32(7) ** i32(2))
    print(power(2, -1))
";
    let ran = run("integer-power", program, false).unwrap();
    assert_eq!(stdout(&ran), "1024 -27 1 -9223372036854775808 81 49\n");
    assert_eq!(ran.status.code(), Some(101), "{}", stderr(&ran));
    assert_eq!(
        panic_report(&ran),
        "panic: negative exponent -1 in integer `**`\n  --> main.byn:2:12\n"
    );

    let overflow = "\
def power(base: i64, exponent: i64) -> i64:
    return base ** exponent


def main():
    print(power(3, 40))
";
    let ran = run("integer-power-overflow", overflow, false).unwrap();
    assert_eq!(ran.status.code(), Some(101), "{}", stderr(&ran));
    assert_eq!(
        panic_report(&ran),
        "panic: integer overflow in `**`\n  --> main.byn:2:12\n"
    );
    // 3 ** 40 modulo 2 ** 64, as a signed `i64`.
    let ran = run("integer-power-release", overflow, true).unwrap();
    assert_eq!(ran.status.code(), Some(0), "{}", stderr(&ran));
    assert_eq!(stdout(&ran), "-6289078614652622815\n");
}

/// `a, (b, c) = e` unpacks the second part of `e` in turn, from a tuple or a `Fixed` vector.
#[test]
fn unpacking_nests() {
    let program = "\
Vec2 = Fixed[f64, (2,)]


def main():
    a, (b, c) = 1, (2.5, True)
    print(a, b, c)
    n, (x, y) = 7, Vec2(0.5, -0.5)
    print(n, x, y)
";
    let ran = run("nested-unpacking", program, false).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(stdout(&ran), "1 2.5 True\n7 0.5 -0.5\n");
}
