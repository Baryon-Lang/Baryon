//! Variadic generics (the 0026 amendment, roadmap 3.19.2), built and run: a callback with a
//! variadic type parameter `*Ts` takes its extra arguments as a tuple and receives them with
//! `f(x, *args)`, as `SciPy`'s `solve_ivp(fun, ..., args=(...))` does.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    Ok((
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
    ))
}

/// The pack bound from `args`, from the callback alone when `args` is left out (then empty),
/// forwarded through a second generic function, and holding text and integers in any order.
#[test]
fn a_callback_receives_its_extra_arguments() {
    let program = "\
def apply[*Ts](f: Callable[[f64, *Ts], f64], x: f64, args: tuple[*Ts] = ()) -> f64:
    return f(x, *args)


def twice[*Ts](f: Callable[[f64, *Ts], f64], x: f64, args: tuple[*Ts] = ()) -> f64:
    return apply(f, apply(f, x, args), args)


def double(x: f64) -> f64:
    return 2.0 * x


def line(x: f64, slope: f64, offset: f64) -> f64:
    return slope * x + offset


def label(x: f64, name: str, n: i64) -> f64:
    print(name, n)
    return x + 1.0


def count(x: f64, n: i64, name: str) -> f64:
    print(n, name)
    return x


def main():
    print(apply(double, 2.0))
    print(apply(line, 2.0, (3.0, 1.0)))
    print(twice(line, 1.0, (2.0, 0.5)))
    print(twice(double, 1.5))
    print(apply(label, 0.5, (\"tag\", 7)))
    print(apply(count, 0.5, (7, \"tag\")))
";
    let (code, stdout, report) = run("variadic-callback", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, "4.0\n7.0\n5.5\n6.0\ntag 7\n1.5\n7 tag\n0.5\n");
}

/// `f(x, *t)` where `t` is a tuple of known length, with no type parameter involved.
#[test]
fn a_known_tuple_spreads_into_a_call() {
    let program = "\
def line(x: f64, slope: f64, offset: f64) -> f64:
    return slope * x + offset


def call(f: Callable[[f64, f64, f64], f64], t: tuple[f64, f64]) -> f64:
    return f(1.0, *t)


def main():
    print(call(line, (2.0, 0.5)))
";
    let (code, stdout, report) = run("variadic-known", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, "2.5\n");
}
