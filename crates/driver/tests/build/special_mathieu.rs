//! The Mathieu functions of `special` (roadmap 3.18.14), `mathieu_a`, `mathieu_b`, `mathieu_cem`
//! and `mathieu_sem`, built and run and checked against the values mpmath records, as
//! [`super::special`] checks the other families, with its tolerances.

use super::special::check;

/// The program: reads `cases.txt`, one case a line, the name, m, q, x for the functions, the
/// size its error is measured against and the reference, and prints the value, one line each.
const MATHIEU: &str = "\
from special import mathieu_a, mathieu_b, mathieu_cem, mathieu_sem


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            m = float(fields[1])
            q = float(fields[2])
            if name == \"mathieu_a\":
                print(mathieu_a(m, q))
            elif name == \"mathieu_b\":
                print(mathieu_b(m, q))
            else:
                x = float(fields[3])
                if name == \"mathieu_cem\":
                    print(mathieu_cem(m, q, x)[0])
                elif name == \"mathieu_cem_d\":
                    print(mathieu_cem(m, q, x)[1])
                elif name == \"mathieu_sem\":
                    print(mathieu_sem(m, q, x)[0])
                else:
                    print(mathieu_sem(m, q, x)[1])
";

/// The characteristic values, the functions and their derivatives at the 1,267 cases of
/// `mathieu.txt`: orders to 40, q of either sign out to 1,000, x across two periods, and orders
/// that are not allowed.
#[test]
fn mathieu_functions_match_the_recorded_values() {
    check("special-mathieu", MATHIEU, "mathieu.txt").unwrap();
}

/// The program for `mathieu_radial.txt`: the radial functions read as the angular ones are, with
/// z in the place of x, and a coefficient by its place in the array.
const RADIAL: &str = "\
from special import mathieu_modcem1, mathieu_modcem2, mathieu_modsem1, mathieu_modsem2
from special import mathieu_even_coef, mathieu_odd_coef


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            m = float(fields[1])
            q = float(fields[2])
            z = float(fields[3])
            if name == \"mathieu_even_coef\":
                print(mathieu_even_coef(m, q)[int(z)])
            elif name == \"mathieu_odd_coef\":
                print(mathieu_odd_coef(m, q)[int(z)])
            else:
                pair = (0.0, 0.0)
                if name.startswith(\"mathieu_modcem1\"):
                    pair = mathieu_modcem1(m, q, z)
                elif name.startswith(\"mathieu_modcem2\"):
                    pair = mathieu_modcem2(m, q, z)
                elif name.startswith(\"mathieu_modsem1\"):
                    pair = mathieu_modsem1(m, q, z)
                else:
                    pair = mathieu_modsem2(m, q, z)
                if name.endswith(\"_d\"):
                    print(pair[1])
                else:
                    print(pair[0])
";

/// The radial functions and their derivatives, and some Fourier coefficients, at the 906 cases of
/// `mathieu_radial.txt`: orders to 12, q from 1e-3 to 200, z from −1 to 3, and arguments
/// `SciPy` gives NaN for.
#[test]
fn radial_mathieu_functions_match_the_recorded_values() {
    check("special-mathieu-radial", RADIAL, "mathieu_radial.txt").unwrap();
}
