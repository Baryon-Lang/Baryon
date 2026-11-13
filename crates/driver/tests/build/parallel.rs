//! `prange` loops built and run (decision 0018): the iterations spread over the runtime's
//! thread pool, arrays written at each iteration's own elements, and reductions combined in a
//! fixed order, so the result is the same on every run.

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// A `prange` over 100,000 iterations writes `ys[i]` and sums into `total`, and one with a
/// start, a step and `continue` multiplies into `product`. The float sum gives the same bits on
/// every run, whatever the thread count.
#[test]
fn prange_loops_build_and_run() {
    let program = "\
def scale(xs: Array[f64, 1], ys: InOut[Array[f64, 1]], factor: f64) -> f64:
    total = 0.0
    for i in prange(len(xs)):
        y = xs[i] * factor
        ys[i] = y
        total += y
    return total


def odd(n: i64) -> i64:
    product = 1
    for i in prange(1, n, 2):
        if i == 5:
            continue
        product *= i
    return product


def main():
    n = 100000
    xs: Array[f64, 1] = Array.zeros(n)
    for i in range(n):
        xs[i] = 1.0 / (i + 1)
    ys: Array[f64, 1] = Array.zeros(n)
    print(scale(xs, ys, 2.0), ys[0], ys[n - 1])
    print(odd(12), odd(0))
";
    let directory = package("prange", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let first = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&first), "");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        "24.180292259726855 2.0 2e-05\n2079 1\n"
    );
    for _ in 0..5 {
        let again = Command::new(&program).output().unwrap();
        assert_eq!(again.stdout, first.stdout);
    }
}

/// Arrays listed in `accumulate=` (decisions 0013 and 0024): a histogram whose iterations add
/// into any bin, and a pair loop that applies each pair's force to both atoms, `Fixed`
/// elements and single lanes included. The results match the serial loops and give the same
/// bits on every run.
#[test]
fn accumulated_arrays_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def histogram(bins: InOut[Array[i64, 1]], n: i64):
    for i in prange(n, accumulate=bins):
        bins[(i * 7919) % 10] += 1
        bins[0] -= 1


def pairs(positions: Array[Vec3, 1], forces: InOut[Array[Vec3, 1]], n: i64):
    for i in prange(n, accumulate=forces):
        for j in range(i + 1, n):
            force = (positions[i] - positions[j]) / float(j - i)
            forces[i] += force
            forces[j] -= force
            forces[j][2] += 0.5


def main():
    bins: Array[i64, 1] = Array.zeros(10)
    histogram(bins, 100000)
    print(bins[0], bins[1], bins[9])
    n = 2000
    positions = Array.zeros(n, dtype=Vec3)
    for k in range(n):
        positions[k][0] = float(k)
        positions[k][1] = 1.0 / float(k + 1)
    forces = Array.zeros(n, dtype=Vec3)
    pairs(positions, forces, n)
    print(forces[0][0], forces[0][1], forces[n - 1][1], forces[n - 1][2])
";
    let directory = package("accumulate", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let first = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&first), "");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        "-90000 10000 10000\n-1999.0 7.178368103610292 -0.004088934051805141 999.5\n"
    );
    for _ in 0..5 {
        let again = Command::new(&program).output().unwrap();
        assert_eq!(again.stdout, first.stdout);
    }
}

/// `accumulate=` of a struct field (decision 0024): the same pair loop as
/// `accumulated_arrays_build_and_run`, with positions and forces held in one struct, gives the
/// same bits as that loop on every run.
#[test]
fn accumulated_fields_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


class Atoms(Struct):
    positions: Array[Vec3, 1]
    forces: Array[Vec3, 1]


def pairs(atoms: InOut[Atoms], n: i64):
    for i in prange(n, accumulate=atoms.forces):
        for j in range(i + 1, n):
            force = (atoms.positions[i] - atoms.positions[j]) / float(j - i)
            atoms.forces[i] += force
            atoms.forces[j] -= force
            atoms.forces[j][2] += 0.5


def main():
    n = 2000
    atoms = Atoms(Array.zeros(n, dtype=Vec3), Array.zeros(n, dtype=Vec3))
    for k in range(n):
        atoms.positions[k][0] = float(k)
        atoms.positions[k][1] = 1.0 / float(k + 1)
    pairs(atoms, n)
    print(atoms.forces[0][0], atoms.forces[0][1], atoms.forces[n - 1][1], atoms.forces[n - 1][2])
";
    let directory = package("accumulate_field", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let first = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&first), "");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        "-1999.0 7.178368103610292 -0.004088934051805141 999.5\n"
    );
    for _ in 0..5 {
        let again = Command::new(&program).output().unwrap();
        assert_eq!(again.stdout, first.stdout);
    }
}

/// `sys.exit` reached through a call inside a `prange` body panics instead of writing out files
/// other iterations are still writing, which crashed the program before (roadmap 1.13.17). Each
/// of 64 iterations writes 400 KB to its own file, so writers are busy when iteration 3 exits.
#[test]
fn exit_inside_a_parallel_loop_panics() {
    let program = "\
import sys


def stop(i: int):
    if i == 3:
        sys.exit(0)


def main():
    for i in prange(64):
        try:
            out = open(f\"out{i}.txt\", \"w\")
            for k in range(20000):
                out.write(\"xxxxxxxxxxxxxxxxxxxx\")
        except OSError:
            pass
        stop(i)
";
    let directory = package("exit_in_prange", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    for _ in 0..3 {
        let run = Command::new(&executable)
            .current_dir(&directory)
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(101));
        assert_eq!(
            panic_report(&run),
            "panic: `sys.exit` cannot end the program inside a parallel loop\n  --> main.byn:6:9\n"
        );
    }
}

/// A number variable bound inside a `prange` body is private to the iteration, so the body may
/// assign it again: `r = r * 2.0`, and `j = j + 1` walking a `while` loop. Lowering took any
/// such assignment for a write to a shared variable and stopped with "not lowered yet", which
/// kept a linked-list walk, as in the half-list Lennard-Jones kernel, out of `prange`.
#[test]
fn a_variable_bound_in_the_body_is_assigned_again() {
    prints(
        "prange-rebind",
        "\
def main():
    total = 0.0
    for i in prange(10):
        r = f64(i)
        r = r * 2.0
        j = 0
        while j < 3:
            total += r
            j = j + 1
    print(total)
",
        "270.0\n",
    )
    .unwrap();
}
