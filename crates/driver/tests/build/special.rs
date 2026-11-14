//! Special functions (roadmap 3.18), built and run: each function of the gamma family (3.18.1), the
//! incomplete gamma and beta functions (3.18.2), the error functions (3.18.3), the Bessel functions
//! of whole order (3.18.4), the everyday helpers, integrals, zeta functions and Lambert W function
//! (3.18.11) and the distribution functions (3.18.12) gives, at a few hundred arguments each, the
//! value mpmath computes to 50 digits, within a tolerance in units in the last place (ULP) stated
//! for each function below, and `SciPy`'s value at its poles, zeros and infinities.
//!
//! The arguments and reference values are in `tests/fixtures/special`; its `README.md` holds the
//! script that recorded them, so the tests run without Python or mpmath. The everyday helpers,
//! integrals, zeta and Lambert W functions are checked in [`super::special_everyday`], through
//! [`check`] here.

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

/// How far a value may be from the reference, in ULP of `scale(name, want, x)`, for a case
/// whose first argument is `first`.
///
/// Most functions are relative: within 8 ULP of the reference. The exponential, sine, cosine and
/// Fresnel integrals, zeta and Spence's function are held to 16: each is a series or a continued
/// fraction of many terms. The Bessel functions are held to 16, since each is a sum or a recurrence
/// of many terms, and `yn` to 32: its upward recurrence adds about half a ULP at each order, and
/// the cases reach order 30. `beta` is held to 8 ULP plus 2 for each unit of the logarithm of the
/// value: above 171 it is `exp(betaln)`, and the exponential turns each ULP of the logarithm's
/// absolute error into its size in relative error. `binom` and `comb` are held to the same: past
/// 1,000 factors they are 1/beta, and a product of hundreds of factors rounds once for each.
///
/// The incomplete gamma and beta functions are an exponential of a sum of logarithms times a
/// series, so they too are held to a number of ULP for each unit of the value's logarithm, 16
/// for the gamma functions and 32 for the beta functions, whose sum has three terms. That is
/// divided by the shape `a` when it is below 1: a small shape raises the value to the power
/// 1/a in the inverses and makes the complement 1 minus a number near 1, so each ULP of input
/// becomes 1/a of output. The smallest shape in the cases is 0.001. The distribution functions
/// are the same functions after a change of variables, and are held to 32 ULP for each unit of
/// the logarithm; their first argument is a count or degrees of freedom, not a shape.
fn tolerance(name: &str, want: f64, first: f64) -> f64 {
    let logarithm = 1.0 + want.abs().ln().abs();
    let shape = first.abs().clamp(1e-3, 1.0);
    match name {
        "beta" | "binom" | "comb" => 8.0 + 2.0 * want.abs().ln().abs(),
        "yn" => 32.0,
        "j0" | "j1" | "jn" | "y0" | "y1" | "i0" | "i1" | "i0e" | "i1e" | "k0" | "k1" | "k0e"
        | "k1e" | "kn" | "exp1" | "expi" | "expn" | "si" | "ci" | "shi" | "chi" | "fresnel_s"
        | "fresnel_c" | "zeta" | "zetac" | "hurwitz" | "spence" => 16.0,
        "gammainc" | "gammaincc" | "gammaincinv" | "gammainccinv" => 16.0 * logarithm / shape,
        "betainc" | "betaincc" | "betaincinv" | "betainccinv" => 32.0 * logarithm / shape,
        "bdtr" | "bdtrc" | "bdtri" | "nbdtr" | "nbdtrc" | "nbdtri" | "pdtr" | "pdtrc" | "pdtri"
        | "chdtr" | "chdtrc" | "chdtri" | "fdtr" | "fdtrc" | "fdtri" | "gdtr" | "gdtrc"
        | "stdtr" | "stdtrit" | "kolmogorov" | "kolmogi" | "smirnov" | "smirnovi" | "owens_t" => {
            32.0 * logarithm
        }
        _ => 8.0,
    }
}

/// The magnitude a function's error is measured against. `gammaln`, `psi`, `betaln`, `expi` and
/// `chi` (the second value of `shichi`) have zeros where no formula keeps relative precision
/// (`gammaln` between −3 and −2, `psi` between each pair of negative whole numbers, `expi` at
/// 0.3725 and `chi` at 0.5238), so they are measured against the larger of the reference and 1, the
/// precision of the terms they are formed from. `ci`, the second value of `sici`, oscillates like
/// sin(x)/x, and is measured against the larger of the reference and 1/x. The Bessel functions of
/// the first and second kind oscillate, so near each zero they are measured against the larger of
/// the reference and their envelope √(2/(π|x|)), for `|x|` above 1. Each part of the complex
/// `lambertw` is measured against the size of the whole value, |W|, which its case records in
/// the place of `x`: a part far smaller than the other has only the precision of the larger.
fn scale(name: &str, want: f64, x: f64) -> f64 {
    match name {
        "gammaln" | "psi" | "betaln" | "expi" | "chi" => want.abs().max(1.0),
        "ci" => want.abs().max(1.0 / x.abs().max(1.0)),
        "lambertw_re" | "lambertw_im" => x,
        "j0" | "j1" | "jn" | "y0" | "y1" | "yn" if x.abs() > 1.0 => want
            .abs()
            .max((2.0 / (std::f64::consts::PI * x.abs())).sqrt()),
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
pub(super) fn check(name: &str, program: &str, fixture: &str) -> io::Result<()> {
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
        let fields: Vec<&str> = case.split(' ').collect();
        let want = number(fields[fields.len() - 1])?;
        let x = number(fields[fields.len() - 2])?;
        let first = number(fields[1])?;
        let got = number(line)?;
        if want.is_nan() {
            assert!(got.is_nan(), "{case}: got {got}");
        } else if want.is_infinite() {
            assert_eq!(got.to_bits(), want.to_bits(), "{case}: got {got}");
        } else if want.abs().to_bits() == 0 {
            assert_eq!(got.abs().to_bits(), 0, "{case}: got {got}");
        } else {
            let error = (got - want).abs() / ulp(scale(function, want, x));
            let allowed = tolerance(function, want, first);
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

/// The Bessel functions' program, which reads the cases as [`GAMMA_FAMILY`] does.
const BESSEL_FUNCTIONS: &str = "\
from special import i0, i0e, i1, i1e, j0, j1, jn, k0, k0e, k1, k1e, kn, y0, y1, yn


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            if name == \"jn\":
                print(jn(int(fields[1]), float(fields[2])))
                continue
            if name == \"yn\":
                print(yn(int(fields[1]), float(fields[2])))
                continue
            if name == \"kn\":
                print(kn(int(fields[1]), float(fields[2])))
                continue
            x = float(fields[1])
            if name == \"j0\":
                print(j0(x))
            elif name == \"j1\":
                print(j1(x))
            elif name == \"y0\":
                print(y0(x))
            elif name == \"y1\":
                print(y1(x))
            elif name == \"i0\":
                print(i0(x))
            elif name == \"i1\":
                print(i1(x))
            elif name == \"i0e\":
                print(i0e(x))
            elif name == \"i1e\":
                print(i1e(x))
            elif name == \"k0\":
                print(k0(x))
            elif name == \"k1\":
                print(k1(x))
            elif name == \"k0e\":
                print(k0e(x))
            elif name == \"k1e\":
                print(k1e(x))
";

/// The incomplete gamma and beta functions' program, which reads the cases as [`GAMMA_FAMILY`]
/// does.
const INCOMPLETE_FUNCTIONS: &str = "\
from special import betainc, betaincc, betainccinv, betaincinv, gammainc, gammaincc, gammainccinv
from special import gammaincinv


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            a = float(fields[1])
            x = float(fields[2])
            if name == \"gammainc\":
                print(gammainc(a, x))
            elif name == \"gammaincc\":
                print(gammaincc(a, x))
            elif name == \"gammaincinv\":
                print(gammaincinv(a, x))
            elif name == \"gammainccinv\":
                print(gammainccinv(a, x))
            elif name == \"betainc\":
                print(betainc(a, x, float(fields[3])))
            elif name == \"betaincc\":
                print(betaincc(a, x, float(fields[3])))
            elif name == \"betaincinv\":
                print(betaincinv(a, x, float(fields[3])))
            elif name == \"betainccinv\":
                print(betainccinv(a, x, float(fields[3])))
";

/// The distribution functions' program, which reads the cases as [`GAMMA_FAMILY`] does.
const DISTRIBUTION_FUNCTIONS: &str = "\
from special import bdtr, bdtrc, bdtri, chdtr, chdtrc, chdtri, fdtr, fdtrc, fdtri, gdtr, gdtrc
from special import kolmogi, kolmogorov, nbdtr, nbdtrc, nbdtri, owens_t, pdtr, pdtrc, pdtri
from special import smirnov, smirnovi, stdtr, stdtrit


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            a = float(fields[1])
            if name == \"kolmogorov\":
                print(kolmogorov(a))
                continue
            if name == \"kolmogi\":
                print(kolmogi(a))
                continue
            b = float(fields[2])
            if name == \"pdtr\":
                print(pdtr(a, b))
            elif name == \"pdtrc\":
                print(pdtrc(a, b))
            elif name == \"pdtri\":
                print(pdtri(a, b))
            elif name == \"chdtr\":
                print(chdtr(a, b))
            elif name == \"chdtrc\":
                print(chdtrc(a, b))
            elif name == \"chdtri\":
                print(chdtri(a, b))
            elif name == \"stdtr\":
                print(stdtr(a, b))
            elif name == \"stdtrit\":
                print(stdtrit(a, b))
            elif name == \"smirnov\":
                print(smirnov(a, b))
            elif name == \"smirnovi\":
                print(smirnovi(a, b))
            elif name == \"owens_t\":
                print(owens_t(a, b))
            else:
                c = float(fields[3])
                if name == \"bdtr\":
                    print(bdtr(a, b, c))
                elif name == \"bdtrc\":
                    print(bdtrc(a, b, c))
                elif name == \"bdtri\":
                    print(bdtri(a, b, c))
                elif name == \"nbdtr\":
                    print(nbdtr(a, b, c))
                elif name == \"nbdtrc\":
                    print(nbdtrc(a, b, c))
                elif name == \"nbdtri\":
                    print(nbdtri(a, b, c))
                elif name == \"fdtr\":
                    print(fdtr(a, b, c))
                elif name == \"fdtrc\":
                    print(fdtrc(a, b, c))
                elif name == \"fdtri\":
                    print(fdtri(a, b, c))
                elif name == \"gdtr\":
                    print(gdtr(a, b, c))
                elif name == \"gdtrc\":
                    print(gdtrc(a, b, c))
";

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

/// The fifteen Bessel functions of whole order at the 2,001 cases of `bessel_functions.txt`,
/// from tiny arguments to 1e8, orders up to 30 and below 0, and the limits at 0 and infinity.
#[test]
fn bessel_functions_match_the_recorded_values() {
    check("special-bessel", BESSEL_FUNCTIONS, "bessel_functions.txt").unwrap();
}

/// `gammainc`, `gammaincc`, `gammaincinv`, `gammainccinv`, `betainc`, `betaincc`, `betaincinv` and
/// `betainccinv` at the 1,166 cases of `incomplete_functions.txt`, with shapes from 0.001 to 1000
/// and values down to 1e-297.
#[test]
fn incomplete_functions_match_the_recorded_values() {
    check(
        "special-incomplete",
        INCOMPLETE_FUNCTIONS,
        "incomplete_functions.txt",
    )
    .unwrap();
}

/// The binomial, negative binomial, Poisson, chi-squared, F, gamma, Student's t, Kolmogorov
/// and Smirnov distribution functions, their complements and inverses, and Owen's T function,
/// at the 1,918 cases of `distribution_functions.txt`, with counts up to 5,000 and values down
/// to 1e-297.
#[test]
fn distribution_functions_match_the_recorded_values() {
    check(
        "special-distributions",
        DISTRIBUTION_FUNCTIONS,
        "distribution_functions.txt",
    )
    .unwrap();
}
