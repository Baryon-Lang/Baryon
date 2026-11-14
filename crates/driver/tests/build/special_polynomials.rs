//! The orthogonal polynomials, associated Legendre functions and spherical harmonics of
//! `special` (roadmap 3.18.10), built and run and checked against the values mpmath records, as
//! [`super::special`] checks the other families, with its tolerances.

use super::special::check;

/// The orthogonal polynomials' program: reads `cases.txt`, one case a line, the function's name,
/// its arguments, the size its error is measured against and the reference value, and prints
/// what the function gives for those arguments, one line each. A case names one part of
/// `sph_harm_y`: `sph_harm_y_re` or `sph_harm_y_im`.
const ORTHOGONAL_POLYNOMIALS: &str = "\
from special import eval_chebyt, eval_chebyu, eval_gegenbauer, eval_genlaguerre, eval_hermite
from special import eval_hermitenorm, eval_jacobi, eval_laguerre, eval_legendre, lpmv, sph_harm_y


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            n = int(fields[1])
            if name == \"eval_legendre\":
                print(eval_legendre(n, float(fields[2])))
            elif name == \"eval_chebyt\":
                print(eval_chebyt(n, float(fields[2])))
            elif name == \"eval_chebyu\":
                print(eval_chebyu(n, float(fields[2])))
            elif name == \"eval_hermite\":
                print(eval_hermite(n, float(fields[2])))
            elif name == \"eval_hermitenorm\":
                print(eval_hermitenorm(n, float(fields[2])))
            elif name == \"eval_laguerre\":
                print(eval_laguerre(n, float(fields[2])))
            elif name == \"eval_genlaguerre\":
                print(eval_genlaguerre(n, float(fields[2]), float(fields[3])))
            elif name == \"eval_gegenbauer\":
                print(eval_gegenbauer(n, float(fields[2]), float(fields[3])))
            elif name == \"eval_jacobi\":
                print(eval_jacobi(n, float(fields[2]), float(fields[3]), float(fields[4])))
            elif name == \"lpmv\":
                print(lpmv(n, float(fields[2]), float(fields[3])))
            else:
                y = sph_harm_y(n, int(fields[2]), float(fields[3]), float(fields[4]))
                if name == \"sph_harm_y_re\":
                    print(y.real)
                else:
                    print(y.imag)
";

/// The nine `eval_` polynomials, `lpmv` and `sph_harm_y` at the 1,487 cases of
/// `orthogonal_polynomials.txt`: degrees up to 200, arguments inside and outside the interval of
/// orthogonality, and `SciPy`'s values at the ends.
#[test]
fn orthogonal_polynomials_match_the_recorded_values() {
    check(
        "special-orthogonal",
        ORTHOGONAL_POLYNOMIALS,
        "orthogonal_polynomials.txt",
    )
    .unwrap();
}
