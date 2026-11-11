//! Array fusion (roadmap 3.1), built and run: a program written in steps gives the same results
//! and the same panics as the program it is fused into.

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

/// `t`, `u` and the update of `c` fuse into one loop, inside a `for` loop; the sums are the same
/// to the last digit as the one-expression form's.
#[test]
fn statements_in_steps_compute_what_one_expression_does() {
    let steps = "\
def main():
    a = Array.zeros((1001,))
    for i in range(1001):
        a[i] = i / 1000
    b = Array.fill((1001,), 3.0)
    d = Array.fill((1001,), 0.5)
    c = Array.zeros((1001,))
    for _ in range(10):
        t = a * b
        u = t + d
        c = u * 0.5 + c * 0.25
    print(c.sum())
";
    let expression = steps.replace(
        "        t = a * b\n        u = t + d\n        c = u * 0.5 + c * 0.25\n",
        "        c = (a * b + d) * 0.5 + c * 0.25\n",
    );
    let fused = run("fusion-steps", steps).unwrap();
    let written = run("fusion-expression", &expression).unwrap();
    assert_eq!(fused, written);
    assert_eq!(fused.0, Some(0));
}

/// `t = a * b` with `b` shorter than `a` panics at `t`'s line, before `c = t + d` runs, as it
/// does when nothing is fused; with `d` the short one instead, the panic is at `c`'s line.
#[test]
fn a_fused_statement_panics_at_its_own_line() {
    let program = "\
def main():
    a = Array.zeros((4,))
    b = Array.zeros((3,))
    d = Array.zeros((3,))
    t = a * b
    c = t + d
    print(c.sum())
";
    let (code, stdout, report) = run("fusion-panic", program).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(stdout, "");
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 4 and 3\n  --> main.byn:5:9\n"
    );

    let later = program.replace("Array.zeros((3,))\n    d", "Array.zeros((4,))\n    d");
    let (code, _, report) = run("fusion-panic-later", &later).unwrap();
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 4 and 3\n  --> main.byn:6:9\n"
    );
}

/// The driver runs fusion: `t = a * b` then `t + a` allocates one array, the result, as
/// `a * b + a` does. An integer array is not fused, so it allocates two.
#[test]
fn the_driver_fuses_temporaries() {
    // Calls of the runtime's array allocation in the compiled LLVM module.
    let allocations = |program: &str| {
        let mut session = driver::Session::new();
        session.add_file("main.byn", program).unwrap();
        assert!(session.check().is_empty());
        let module = session.compile(None, driver::Profile::Debug).unwrap();
        module.matches("call ptr @baryon_rt_allocate_array").count()
    };
    let floats = "\
def f(a: Array[f64, 1], b: Array[f64, 1]) -> Array[f64, 1]:
    t = a * b
    return t + a


def main():
    print(f(Array.zeros((2,)), Array.zeros((2,))).sum())
";
    let integers = floats
        .replace("f64", "i64")
        .replace("zeros((2,))", "zeros((2,), dtype=i64)");
    assert_eq!(
        allocations(floats),
        allocations(&floats.replace("t = a * b\n    return t + a", "return a * b + a"))
    );
    assert_eq!(allocations(&integers), allocations(floats) + 1);
}
