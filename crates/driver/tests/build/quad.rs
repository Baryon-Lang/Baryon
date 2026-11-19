//! `quad` on finite intervals (roadmap 3.19.4), built and run: QUADPACK's QAGS and, with
//! `points=`, QAGP give `SciPy` 1.18's bits for the integral and its error estimate, and raise
//! `IntegrationError` with `SciPy`'s message for each of QUADPACK's five failures, where `SciPy`
//! warns (decision 0065).
//!
//! The integrands use additions, multiplications, divisions and square roots alone, which round
//! the same everywhere, so both languages integrate the same numbers. The expected lines were
//! printed by `SciPy` 1.18.1 for the same calls, with each `IntegrationWarning` turned into an
//! error and its message printed with `|` for each line break.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// The program that calls `quad` the ways the test checks, one call per line of output.
const PROGRAM: &str = r#"import math
from math import inf
from integrate import quad, IntegrationError


def square(x: f64) -> f64:
    return x * x


def hump(x: f64, k: f64) -> f64:
    return k * x * (1.0 - x)


def root(x: f64) -> f64:
    return 1.0 / math.sqrt(x)


def kink(x: f64) -> f64:
    return abs(x - 0.3)


def bell(x: f64) -> f64:
    return 1.0 / (1.0 + x * x)


def inverse(x: f64) -> f64:
    return 1.0 / x


def steep(x: f64) -> f64:
    return 1.0 / (x * math.sqrt(x))


def pole(x: f64) -> f64:
    return 1.0 / (1.000000000000001 - x)


def eighth(x: f64) -> f64:
    return 1.0 / math.sqrt(math.sqrt(math.sqrt(x))) ** 7


def twin(x: f64) -> f64:
    return 1.0 / math.sqrt(abs(x - 0.3)) + 1.0 / math.sqrt(abs(x - 0.7))


def stripes(x: f64) -> f64:
    return f64(int(x * 37.0) % 2)


def step(x: f64) -> f64:
    return 1.0 if x <= 0.0 else 0.0


def label(x: f64, name: str, n: i64) -> f64:
    return x * f64(n) if name == "double" else x


def main():
    try:
        print(quad(square, 0.0, 3.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(hump, 0.0, 1.0, (6.0,)))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(root, 0.0, 1.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(kink, 0.0, 1.0, points=[0.3]))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(bell, 2.0, -1.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(square, 1.5, 1.5))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(step, -1.0, 100.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(step, -1.0, 10000.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(label, 0.0, 2.0, ("double", 3)))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(twin, 0.0, 1.0, points=[0.7, 0.3, 0.3, 5.0]))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(twin, 1.0, 0.0, points=[0.3, 0.7], epsabs=0.0, epsrel=1e-10))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(stripes, 0.0, 1.0, points=[0.5], limit=3))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(square, 0.0, 3.0, limit=1))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(inverse, 0.0, 1.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(square, 0.0, 1.0, epsabs=1e-300, epsrel=1e-14))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(pole, 0.0, 1.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(eighth, 0.0, 1.0, epsabs=1e-300, epsrel=3e-15))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(steep, 0.0, 1.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
"#;

/// What `SciPy` 1.18.1 prints for the same calls.
const EXPECTED: &str = "\
    (9.000000000000002, 9.992007221626411e-14)\n\
    (1.0, 1.1102230246251565e-14)\n\
    (1.9999999999999984, 5.773159728050814e-15)\n\
    (0.29, 3.219646771412954e-15)\n\
    (-1.892546881191539, 2.040024582366986e-11)\n\
    (0.0, 0.0)\n\
    (1.0000000002199108, 1.0189464580163188e-08)\n\
    (0.0, 0.0)\n\
    (6.0, 6.661338147750939e-14)\n\
    (5.5375303361568555, 5.88862292261183e-13)\n\
    (-5.5375303361568555, 5.88862292261183e-13)\n\
    The maximum number of subdivisions (3) has been achieved.|  If increasing the limit \
    yields no improvement it is advised to analyze |  the integrand in order to determine \
    the difficulties.  If the position of a |  local difficulty can be determined \
    (singularity, discontinuity) one will |  probably gain from splitting up the interval \
    and calling the integrator |  on the subranges.  Perhaps a special-purpose integrator \
    should be used.\n\
    The maximum number of subdivisions (1) has been achieved.|  If increasing the limit \
    yields no improvement it is advised to analyze |  the integrand in order to determine \
    the difficulties.  If the position of a |  local difficulty can be determined \
    (singularity, discontinuity) one will |  probably gain from splitting up the interval \
    and calling the integrator |  on the subranges.  Perhaps a special-purpose integrator \
    should be used.\n\
    The maximum number of subdivisions (50) has been achieved.|  If increasing the limit \
    yields no improvement it is advised to analyze |  the integrand in order to determine \
    the difficulties.  If the position of a |  local difficulty can be determined \
    (singularity, discontinuity) one will |  probably gain from splitting up the interval \
    and calling the integrator |  on the subranges.  Perhaps a special-purpose integrator \
    should be used.\n\
    The occurrence of roundoff error is detected, which prevents |  the requested tolerance \
    from being achieved.  The error may be |  underestimated.\n\
    Extremely bad integrand behavior occurs at some points of the|  integration interval.\n\
    The algorithm does not converge.  Roundoff error is detected|  in the extrapolation \
    table.  It is assumed that the requested tolerance|  cannot be achieved, and that the \
    returned result (if full_output = 1) is |  the best which can be obtained.\n\
    The integral is probably divergent, or slowly convergent.\n";

/// Builds `program` as the package `name` and runs it, giving its exit code, standard output and
/// panic report.
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

/// Smooth integrands, a singularity at an end, a kink and singularities given by `points=`
/// (duplicated and outside the interval too, which `SciPy` drops), extra arguments of two
/// types, reversed limits, an empty interval, a step function that the rule misses, and one
/// call for each failure: the subdivision limit (with and without `points=`, and `limit=1`),
/// roundoff, bad behavior, no convergence and divergence.
#[test]
fn quad_gives_scipys_bits_and_failures() {
    let (code, stdout, report) = run("quad-scipy", PROGRAM).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, EXPECTED);
}

/// Arguments `SciPy` rejects with `ValueError` panic with `SciPy`'s message (decision 0065 point
/// 3): `limit` below 1, `epsabs <= 0` with too small an `epsrel`, as many break points as
/// `limit`, a break point outside the interval, and break points on an infinite interval.
#[test]
fn invalid_arguments_panic() {
    let cases = [
        (
            "0.0, 3.0, limit=0",
            "limit >= 1, \"Invalid 'limit' argument. There must be at least one subinterval\"",
            422,
            9,
        ),
        (
            "0.0, 3.0, epsabs=0.0, epsrel=1e-15",
            "not tight, ( \"If 'epsabs'<=0, 'epsrel' must be greater than both 5e-29 and \
             50*(machine epsilon).\" )",
            418,
            5,
        ),
        (
            "0.0, 3.0, points=[1.0, 2.0], limit=2",
            "len(points) < limit, \"Number of break points must be less than subinterval \
             limit\"",
            428,
            9,
        ),
        (
            "0.0, 3.0, points=[1.0, 5.0], limit=1",
            "inside, \"All break points in 'points' must lie within the integration limits.\"",
            427,
            9,
        ),
        (
            "0.0, inf, points=[1.0]",
            "not infinite, \"Infinity inputs cannot be used with break points.\"",
            474,
            9,
        ),
    ];
    for (index, (arguments, assertion, line, column)) in cases.into_iter().enumerate() {
        let program = format!(
            "\
from math import inf
from integrate import quad, IntegrationError


def square(x: f64) -> f64:
    return x * x


def main() -> Raises[None, IntegrationError]:
    print(quad(square, {arguments}))
"
        );
        let (code, stdout, report) = run(&format!("quad-invalid-{index}"), &program).unwrap();
        assert_eq!((code, stdout.as_str()), (Some(101), ""), "{arguments}");
        assert_eq!(
            report,
            format!(
                "panic: assertion failed: {assertion}\n  \
                 --> <std>/integrate_quad.byn:{line}:{column}\n"
            ),
            "{arguments}"
        );
    }
}

/// The program that calls `quad` over infinite intervals, one call per line of output.
const INFINITE: &str = r#"import math
from math import inf
from integrate import quad, IntegrationError


def bell(x: f64) -> f64:
    return 1.0 / (1.0 + x * x)


def scaled(x: f64, k: f64) -> f64:
    return k / (k * k + x * x)


def tail(x: f64) -> f64:
    return 1.0 / ((1.0 + x) * (1.0 + x))


def root(x: f64) -> f64:
    return 1.0 / ((1.0 + x) * math.sqrt(x))


def odd(x: f64) -> f64:
    return x / (1.0 + x * x * x * x)


def inverse(x: f64) -> f64:
    return 1.0 / x


def slow(x: f64) -> f64:
    return 1.0 / math.sqrt(x)


def wave(x: f64) -> f64:
    return (x - 3.0) / ((1.0 + (x - 3.0) * (x - 3.0)) * (1.0 + (x - 3.0) * (x - 3.0)))


def main():
    try:
        print(quad(bell, -inf, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(bell, 0.0, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(bell, -inf, 0.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(bell, inf, -inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(scaled, 1.0, inf, (2.0,)))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(tail, 0.0, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(tail, inf, 0.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(root, 0.0, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(odd, -inf, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(wave, -inf, 2.0))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(bell, inf, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(bell, 0.0, inf, limit=1))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(inverse, 1.0, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
    try:
        print(quad(slow, 1.0, inf))
    except IntegrationError as error:
        print(error.message.replace("\n", "|"))
"#;

/// What `SciPy` 1.18.1 prints for the same calls.
const INFINITE_EXPECTED: &str = "\
    (3.141592653589793, 5.155583041103855e-10)\n\
    (1.5707963267948966, 2.5777915205519274e-10)\n\
    (1.5707963267948966, 2.5777915205519274e-10)\n\
    (-3.141592653589793, 5.155583041103855e-10)\n\
    (1.1071487177940906, 1.2889314817856373e-10)\n\
    (1.0, 1.1102230246251565e-14)\n\
    (-1.0, 1.1102230246251565e-14)\n\
    (3.141592653591123, 3.770408874004261e-09)\n\
    (0.0, 0.0)\n\
    (-0.24999999999999994, 1.0524504433644551e-09)\n\
    (0.0, 0.0)\n\
    The maximum number of subdivisions (1) has been achieved.|  If increasing the limit yields \
    no improvement it is advised to analyze |  the integrand in order to determine the \
    difficulties.  If the position of a |  local difficulty can be determined (singularity, \
    discontinuity) one will |  probably gain from splitting up the interval and calling the \
    integrator |  on the subranges.  Perhaps a special-purpose integrator should be used.\n\
    The maximum number of subdivisions (50) has been achieved.|  If increasing the limit yields \
    no improvement it is advised to analyze |  the integrand in order to determine the \
    difficulties.  If the position of a |  local difficulty can be determined (singularity, \
    discontinuity) one will |  probably gain from splitting up the interval and calling the \
    integrator |  on the subranges.  Perhaps a special-purpose integrator should be used.\n\
    The integral is probably divergent, or slowly convergent.\n";

/// Infinite intervals by QUADPACK's QAGI, which maps each onto (0, 1]: the whole line, each
/// half-line, reversed limits, `args`, an odd integrand whose halves cancel, an interval from
/// infinity to itself, and the subdivision limit and divergence.
#[test]
fn infinite_intervals_give_scipys_bits() {
    let (code, stdout, report) = run("quad-infinite", INFINITE).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, INFINITE_EXPECTED);
}
