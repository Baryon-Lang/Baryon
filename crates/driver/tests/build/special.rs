//! Special functions (roadmap 3.18), built and run: each function of the gamma family (3.18.1)
//! and of the error functions (3.18.3) gives, at a few hundred arguments each, the value mpmath
//! computes to 50 digits, within a tolerance in units in the last place (ULP) stated for each
//! function below, and `SciPy`'s value at its poles, zeros and infinities.
//!
//! The arguments and reference values are in `tests/fixtures/special`; its `README.md` holds the
//! script that recorded them, so the tests run without Python or mpmath.

use super::{baryon_build, package, stderr};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// The directory of recorded arguments and reference values.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/special"
    ))
}

/// The gamma family's program: reads `cases.txt`, one case a line, the function's name, its
/// arguments and the reference value, and prints what the function gives for those arguments,
/// one line each.
const GAMMA_FAMILY: &str = "\
from special import beta, betaln, digamma, gamma, gammaln, gammasgn, loggamma, poch
from special import polygamma, psi, rgamma


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            if name == \"polygamma\":
                print(polygamma(int(fields[1]), float(fields[2])))
                continue
            x = float(fields[1])
            if name == \"gamma\":
                print(gamma(x))
            elif name == \"rgamma\":
                print(rgamma(x))
            elif name == \"gammaln\":
                print(gammaln(x))
            elif name == \"loggamma\":
                print(loggamma(x))
            elif name == \"gammasgn\":
                print(gammasgn(x))
            elif name == \"psi\":
                print(psi(x))
            elif name == \"digamma\":
                print(digamma(x))
            elif name == \"beta\":
                print(beta(x, float(fields[2])))
            elif name == \"betaln\":
                print(betaln(x, float(fields[2])))
            elif name == \"poch\":
                print(poch(x, float(fields[2])))
";

/// The error functions' program, which reads the cases as [`GAMMA_FAMILY`] does.
const ERROR_FUNCTIONS: &str = "\
from special import dawsn, erf, erfc, erfcinv, erfcx, erfinv, log_ndtr, ndtr, ndtri


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            x = float(fields[1])
            if name == \"erf\":
                print(erf(x))
            elif name == \"erfc\":
                print(erfc(x))
            elif name == \"erfcx\":
                print(erfcx(x))
            elif name == \"erfinv\":
                print(erfinv(x))
            elif name == \"erfcinv\":
                print(erfcinv(x))
            elif name == \"ndtr\":
                print(ndtr(x))
            elif name == \"ndtri\":
                print(ndtri(x))
            elif name == \"log_ndtr\":
                print(log_ndtr(x))
            elif name == \"dawsn\":
                print(dawsn(x))
";

/// How far a value may be from the reference, in ULP of `scale(name, want)`.
///
/// Most functions are relative: within 8 ULP of the reference. `beta` is held to 8 ULP plus 2
/// for each unit of the logarithm of the value: above 171 it is `exp(betaln)`, and the
/// exponential turns each ULP of the logarithm's absolute error into its size in relative
/// error.
fn tolerance(name: &str, want: f64) -> f64 {
    match name {
        "beta" => 8.0 + 2.0 * want.abs().ln().abs(),
        _ => 8.0,
    }
}

/// The magnitude a function's error is measured against. `gammaln`, `psi` and `betaln` have
/// zeros where no formula keeps relative precision (`gammaln` between −3 and −2, `psi` between
/// each pair of negative whole numbers), so they are measured against the larger of the
/// reference and 1, the precision of the terms they are formed from.
fn scale(name: &str, want: f64) -> f64 {
    match name {
        "gammaln" | "psi" | "betaln" => want.abs().max(1.0),
        _ => want.abs(),
    }
}

/// The distance between adjacent `f64` values at `x`, which is finite and above 0.
fn ulp(x: f64) -> f64 {
    let x = x.abs();
    if x < f64::MIN_POSITIVE {
        return f64::from_bits(1);
    }
    f64::from_bits(x.to_bits() + 1) - x
}

/// Builds `program` as the package `name`, runs it on the cases of `fixture`, and checks each
/// printed value: finite references within their tolerance, and zeros, infinities and NaN as
/// `SciPy` gives them. The sign of a zero is not compared: `SciPy` gives `poch(-1.0, 2.0)` as 0.0
/// where the product (−1) × 0 is −0.0.
///
/// # Errors
///
/// Fails when the package cannot be written, the program cannot be run, or a line is not a
/// number.
///
/// # Panics
///
/// When the program does not build or run, or a value is not what the case records.
fn check(name: &str, program: &str, fixture: &str) -> io::Result<()> {
    let cases = fs::read_to_string(fixtures().join(fixture))?;
    let directory = package(name, &[("main.byn", program)])?;
    fs::write(directory.join("cases.txt"), &cases)?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).current_dir(&directory).output()?;
    assert!(ran.status.success(), "{}", stderr(&ran));
    let printed = String::from_utf8_lossy(&ran.stdout);
    assert_eq!(printed.lines().count(), cases.lines().count());
    for (case, line) in cases.lines().zip(printed.lines()) {
        let function = case.split(' ').next().unwrap_or_default();
        let want = number(case.rsplit(' ').next().unwrap_or_default())?;
        let got = number(line)?;
        if want.is_nan() {
            assert!(got.is_nan(), "{case}: got {got}");
        } else if want.is_infinite() {
            assert_eq!(got.to_bits(), want.to_bits(), "{case}: got {got}");
        } else if want.abs().to_bits() == 0 {
            assert_eq!(got.abs().to_bits(), 0, "{case}: got {got}");
        } else {
            let error = (got - want).abs() / ulp(scale(function, want));
            let allowed = tolerance(function, want);
            assert!(
                error <= allowed,
                "{case}: got {got}, {error} ULP, {allowed} allowed"
            );
        }
    }
    Ok(())
}

/// `text` read as an `f64`, as Python's `repr` and Baryon's `print` write one.
///
/// # Errors
///
/// Fails when `text` is not a number.
fn number(text: &str) -> io::Result<f64> {
    text.parse()
        .map_err(|error| io::Error::other(format!("`{text}`: {error}")))
}

/// `gamma`, `rgamma`, `gammaln`, `loggamma`, `gammasgn`, `psi`, `digamma`, `polygamma`, `beta`,
/// `betaln` and `poch` at the 2,074 cases of `gamma_family.txt`.
#[test]
fn gamma_family_matches_the_recorded_values() {
    check("special-gamma", GAMMA_FAMILY, "gamma_family.txt").unwrap();
}

/// `erf`, `erfc`, `erfcx`, `erfinv`, `erfcinv`, `ndtr`, `ndtri`, `log_ndtr` and `dawsn` at the
/// 1,486 cases of `error_functions.txt`, out to the far tails: `erfc` near its underflow at
/// 27, `ndtr` at −38 and `log_ndtr` at −1e100.
#[test]
fn error_functions_match_the_recorded_values() {
    check("special-error", ERROR_FUNCTIONS, "error_functions.txt").unwrap();
}
