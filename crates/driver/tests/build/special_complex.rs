//! The functions of `special` that take a complex argument (roadmap 3.18.6), each an overload of
//! the real function's name (decision 0067), built and run and checked against the values
//! mpmath records, as [`super::special`] checks the other families, with its tolerances.

use super::special::check;

/// The program: reads `cases.txt`, one case a line, `loggamma_re` or `loggamma_im`, the
/// argument's real and imaginary parts, the size its error is measured against and the
/// reference, and prints that part of `loggamma` of the argument, one line each.
const LOGGAMMA: &str = "\
from special import loggamma


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            w = loggamma(complex(float(fields[1]), float(fields[2])))
            if fields[0] == \"loggamma_re\":
                print(w.real)
            else:
                print(w.imag)
";

/// The complex `loggamma` at the 312 cases of `loggamma_complex.txt`: around 1, 1.5 and 2,
/// near the poles 0 to −8, on both sides of the cut along the negative axis, far out to 10³,
/// and at the arguments that give NaN.
#[test]
fn complex_loggamma_matches_the_recorded_values() {
    check("special-loggamma-complex", LOGGAMMA, "loggamma_complex.txt").unwrap();
}
