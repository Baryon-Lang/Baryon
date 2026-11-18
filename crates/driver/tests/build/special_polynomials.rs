//! The orthogonal polynomials, associated Legendre functions, spherical harmonics and Gauss
//! quadrature rules of `special` (roadmap 3.18.10), built and run and checked against the values
//! mpmath records, as [`super::special`] checks the other families, with its tolerances.

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

/// The Gauss quadrature program: reads `cases.txt`, one case a line, `roots_<family>_x` or
/// `roots_<family>_w`, the number of nodes n, the index of the node, the parameters α and β
/// (0.0 where the family has none), the size its error is measured against and the reference,
/// and prints that node or weight. Consecutive cases of one rule share it, so each rule is built
/// once.
const GAUSS_QUADRATURE: &str = "\
from special import roots_chebyt, roots_chebyu, roots_gegenbauer, roots_genlaguerre, roots_hermite
from special import roots_hermitenorm, roots_jacobi, roots_laguerre, roots_legendre


def rule(family: str, n: int, alpha: f64, beta: f64) -> tuple[Array[f64, 1], Array[f64, 1]]:
    if family == \"roots_legendre\":
        return roots_legendre(n)
    if family == \"roots_chebyt\":
        return roots_chebyt(n)
    if family == \"roots_chebyu\":
        return roots_chebyu(n)
    if family == \"roots_hermite\":
        return roots_hermite(n)
    if family == \"roots_hermitenorm\":
        return roots_hermitenorm(n)
    if family == \"roots_laguerre\":
        return roots_laguerre(n)
    if family == \"roots_genlaguerre\":
        return roots_genlaguerre(n, alpha)
    if family == \"roots_jacobi\":
        return roots_jacobi(n, alpha, beta)
    return roots_gegenbauer(n, alpha)


def main() -> Raises[None, OSError | ValueError]:
    key = \"\"
    nodes = Array.zeros(1, f64)
    weights = Array.zeros(1, f64)
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            n = int(fields[1])
            i = int(fields[2])
            case = name[:-2] + \" \" + fields[1] + \" \" + fields[3] + \" \" + fields[4]
            if case != key:
                nodes, weights = rule(name[:-2], n, float(fields[3]), float(fields[4]))
                key = case
            if name.endswith(\"_x\"):
                print(nodes[i])
            else:
                print(weights[i])
";

/// The nine `roots_` rules at the 3,100 cases of `gauss_quadrature.txt`: rules of 1 to 150 nodes
/// for every family and parameter, 400 nodes for Hermite's and Laguerre's, whose recurrences pass
/// the range of `f64` in the tails, and `SciPy`'s NaN rules at the edge of a parameter's range.
#[test]
fn gauss_quadrature_matches_the_recorded_values() {
    check(
        "special-quadrature",
        GAUSS_QUADRATURE,
        "gauss_quadrature.txt",
    )
    .unwrap();
}
