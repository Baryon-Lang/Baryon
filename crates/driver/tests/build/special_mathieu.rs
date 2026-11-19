//! The Mathieu and spheroidal wave functions of `special` (roadmap 3.18.14): `mathieu_a`,
//! `mathieu_b`, `mathieu_cem`, `mathieu_sem` and the radial Mathieu functions, and the spheroidal
//! characteristic values and angular functions, built and run and checked against the values
//! mpmath records, as [`super::special`] checks the other families, with its tolerances.

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

/// The program for `spheroidal.txt`: the characteristic values read m, n and c; the angular
/// functions x after them, or the characteristic value and then x for the `_cv` forms; and the
/// sequences the place of the value in the array.
const SPHEROIDAL: &str = "\
from special import pro_cv, obl_cv, pro_cv_seq, obl_cv_seq, pro_ang1, obl_ang1, pro_ang1_cv
from special import obl_ang1_cv


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            m = float(fields[1])
            n = float(fields[2])
            c = float(fields[3])
            pair = (0.0, 0.0)
            if name == \"pro_cv\":
                print(pro_cv(m, n, c))
                continue
            elif name == \"obl_cv\":
                print(obl_cv(m, n, c))
                continue
            elif name == \"pro_cv_seq\":
                print(pro_cv_seq(m, n, c)[int(fields[4])])
                continue
            elif name == \"obl_cv_seq\":
                print(obl_cv_seq(m, n, c)[int(fields[4])])
                continue
            elif name.startswith(\"pro_ang1_cv\"):
                pair = pro_ang1_cv(m, n, c, float(fields[4]), float(fields[5]))
            elif name.startswith(\"obl_ang1_cv\"):
                pair = obl_ang1_cv(m, n, c, float(fields[4]), float(fields[5]))
            elif name.startswith(\"pro_ang1\"):
                pair = pro_ang1(m, n, c, float(fields[4]))
            else:
                pair = obl_ang1(m, n, c, float(fields[4]))
            if name.endswith(\"_d\"):
                print(pair[1])
            else:
                print(pair[0])
";

/// The spheroidal characteristic values, alone and in sequences, and the angular functions and
/// their derivatives, with the characteristic value found or given, at the cases of
/// `spheroidal.txt`: orders to 30 and 40 above them, c from 1e-4 to 250 and below 0, x across
/// −1 < x < 1 and near its ends, and arguments that are not allowed.
#[test]
fn spheroidal_functions_match_the_recorded_values() {
    check("special-spheroidal", SPHEROIDAL, "spheroidal.txt").unwrap();
}

/// The program for `spheroidal_radial.txt`: m, n, c and x, or m, n, c, the characteristic value
/// and x for the `_cv` forms.
const SPHEROIDAL_RADIAL: &str = "\
from special import pro_rad1, pro_rad2, obl_rad1, obl_rad2, pro_rad1_cv, pro_rad2_cv
from special import obl_rad1_cv, obl_rad2_cv


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            m = float(fields[1])
            n = float(fields[2])
            c = float(fields[3])
            fourth = float(fields[4])
            pair = (0.0, 0.0)
            if name.startswith(\"pro_rad1_cv\"):
                pair = pro_rad1_cv(m, n, c, fourth, float(fields[5]))
            elif name.startswith(\"pro_rad2_cv\"):
                pair = pro_rad2_cv(m, n, c, fourth, float(fields[5]))
            elif name.startswith(\"obl_rad1_cv\"):
                pair = obl_rad1_cv(m, n, c, fourth, float(fields[5]))
            elif name.startswith(\"obl_rad2_cv\"):
                pair = obl_rad2_cv(m, n, c, fourth, float(fields[5]))
            elif name.startswith(\"pro_rad1\"):
                pair = pro_rad1(m, n, c, fourth)
            elif name.startswith(\"pro_rad2\"):
                pair = pro_rad2(m, n, c, fourth)
            elif name.startswith(\"obl_rad1\"):
                pair = obl_rad1(m, n, c, fourth)
            else:
                pair = obl_rad2(m, n, c, fourth)
            if name.endswith(\"_d\"):
                print(pair[1])
            else:
                print(pair[0])
";

/// The radial spheroidal functions of both kinds and their derivatives, with the characteristic
/// value found or given, at the cases of `spheroidal_radial.txt`: orders to 10 and 12 above them,
/// c from 0.01 to 50, prolate x from 1 + 1e-6 to 30 and oblate x from 0 to 30, and arguments
/// that are not allowed.
#[test]
fn radial_spheroidal_functions_match_the_recorded_values() {
    check(
        "special-spheroidal-radial",
        SPHEROIDAL_RADIAL,
        "spheroidal_radial.txt",
    )
    .unwrap();
}
