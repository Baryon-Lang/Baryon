//! The everyday helpers, exponential integrals, zeta functions and Lambert W function of
//! `special` (roadmap 3.18.11), built and run and checked against the values mpmath records, as
//! [`super::special`] checks the other families, with its tolerances.

use super::special::check;
use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// The everyday helpers' program: reads `cases.txt`, one case a line, the function's name, its
/// arguments and the reference value, and prints what the function gives for those arguments,
/// one line each.
const HELPER_FUNCTIONS: &str = "\
from special import binom, comb, entr, expit, factorial, kl_div, log_expit, logit, perm, rel_entr
from special import xlog1py, xlogy


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            a = float(fields[1])
            if name == \"expit\":
                print(expit(a))
            elif name == \"log_expit\":
                print(log_expit(a))
            elif name == \"logit\":
                print(logit(a))
            elif name == \"entr\":
                print(entr(a))
            elif name == \"factorial\":
                print(factorial(a))
            else:
                b = float(fields[2])
                if name == \"xlogy\":
                    print(xlogy(a, b))
                elif name == \"xlog1py\":
                    print(xlog1py(a, b))
                elif name == \"rel_entr\":
                    print(rel_entr(a, b))
                elif name == \"kl_div\":
                    print(kl_div(a, b))
                elif name == \"binom\":
                    print(binom(a, b))
                elif name == \"comb\":
                    print(comb(a, b))
                elif name == \"perm\":
                    print(perm(a, b))
";

/// The integrals' and zeta functions' program, which reads the cases as [`HELPER_FUNCTIONS`]
/// does. A case names one value of a function that returns two: `si` and `ci` for `sici`, `shi`
/// and `chi` for `shichi`, and `fresnel_s` and `fresnel_c` for `fresnel`; `hurwitz` is `zeta`
/// with its second argument.
const INTEGRAL_FUNCTIONS: &str = "\
from special import exp1, expi, expn, fresnel, shichi, sici, spence, zeta, zetac


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            if name == \"expn\":
                print(expn(int(fields[1]), float(fields[2])))
                continue
            x = float(fields[1])
            if name == \"hurwitz\":
                print(zeta(x, float(fields[2])))
            elif name == \"exp1\":
                print(exp1(x))
            elif name == \"expi\":
                print(expi(x))
            elif name == \"zeta\":
                print(zeta(x))
            elif name == \"zetac\":
                print(zetac(x))
            elif name == \"spence\":
                print(spence(x))
            elif name == \"si\" or name == \"ci\":
                si, ci = sici(x)
                if name == \"si\":
                    print(si)
                else:
                    print(ci)
            elif name == \"shi\" or name == \"chi\":
                shi, chi = shichi(x)
                if name == \"shi\":
                    print(shi)
                else:
                    print(chi)
            else:
                s, c = fresnel(x)
                if name == \"fresnel_s\":
                    print(s)
                else:
                    print(c)
";

/// The Lambert W function's program. A case is one part of Wₖ(z): `lambertw_re` or
/// `lambertw_im`, the branch `k`, the real and imaginary parts of `z`, the size |Wₖ(z)| the
/// error is measured against, and the part's reference value.
const LAMBERT_W: &str = "\
from special import lambertw


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            w = lambertw(complex(float(fields[2]), float(fields[3])), int(fields[1]))
            if name == \"lambertw_re\":
                print(w.real)
            else:
                print(w.imag)
";

/// `expit`, `log_expit`, `logit`, `xlogy`, `xlog1py`, `entr`, `rel_entr`, `kl_div`, `binom`,
/// `comb`, `perm` and `factorial` at the 978 cases of `helper_functions.txt`.
#[test]
fn helper_functions_match_the_recorded_values() {
    check("special-helpers", HELPER_FUNCTIONS, "helper_functions.txt").unwrap();
}

/// `exp1`, `expi`, `expn`, `sici`, `shichi`, `fresnel`, `zeta`, `zetac` and `spence` at the
/// 1,375 cases of `integral_functions.txt`.
#[test]
fn integral_functions_match_the_recorded_values() {
    check(
        "special-integrals",
        INTEGRAL_FUNCTIONS,
        "integral_functions.txt",
    )
    .unwrap();
}

/// `lambertw` at the 1,050 cases of `lambertw.txt`: branches −3 to 3 and 40, real arguments
/// on both real branches from the branch point −1/e out to 1e300, complex arguments from 1e-10
/// to 1e10 in size, and `SciPy`'s values at 0, 1, infinity and NaN.
#[test]
fn lambert_w_matches_the_recorded_values() {
    check("special-lambertw", LAMBERT_W, "lambertw.txt").unwrap();
}

/// `logsumexp` and `softmax`, which take arrays, give `SciPy` 1.18's values, its infinities
/// included.
#[test]
fn array_helpers_match_scipy() -> io::Result<()> {
    let program = "\
from math import inf
from special import logsumexp, softmax


def main():
    print(logsumexp(Array([1000.0, 1000.0])))
    print(logsumexp(Array([-1.0, 0.5, 3.0])))
    weights = softmax(Array([1.0, 2.0, 3.0]))
    print(weights[0])
    print(weights[1])
    print(weights[2])
    print(logsumexp(Array([-inf, -inf])))
    print(logsumexp(Array([inf, 1.0])))
";
    let directory = package("special-arrays", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output()?;
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "1000.6931471805599\n3.0956743294143823\n0.09003057317038046\n\
         0.24472847105479764\n0.6652409557748218\n-inf\ninf\n"
    );
    Ok(())
}
