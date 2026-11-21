//! The elliptic integrals and Jacobi's elliptic functions of `special` (roadmap 3.18.9), built
//! and run and checked against the values mpmath records, as [`super::special`] checks the other
//! families, with its tolerances.

use super::special::check;

/// The elliptic functions' program: reads `cases.txt`, one case a line, the function's name, its
/// arguments and the reference value, and prints what the function gives for those arguments,
/// one line each. A case names one value of `ellipj`: `ellipj_sn`, `ellipj_cn`, `ellipj_dn` or
/// `ellipj_ph`.
const ELLIPTIC_FUNCTIONS: &str = "\
from special import ellipe, ellipeinc, ellipj, ellipk, ellipkinc, ellipkm1
from special import elliprc, elliprd, elliprf, elliprg, elliprj


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            x = float(fields[1])
            if name == \"ellipk\":
                print(ellipk(x))
            elif name == \"ellipkm1\":
                print(ellipkm1(x))
            elif name == \"ellipe\":
                print(ellipe(x))
            elif name == \"ellipkinc\":
                print(ellipkinc(x, float(fields[2])))
            elif name == \"ellipeinc\":
                print(ellipeinc(x, float(fields[2])))
            elif name == \"elliprc\":
                print(elliprc(x, float(fields[2])))
            elif name == \"elliprf\":
                print(elliprf(x, float(fields[2]), float(fields[3])))
            elif name == \"elliprd\":
                print(elliprd(x, float(fields[2]), float(fields[3])))
            elif name == \"elliprg\":
                print(elliprg(x, float(fields[2]), float(fields[3])))
            elif name == \"elliprj\":
                print(elliprj(x, float(fields[2]), float(fields[3]), float(fields[4])))
            else:
                sn, cn, dn, ph = ellipj(x, float(fields[2]))
                if name == \"ellipj_sn\":
                    print(sn)
                elif name == \"ellipj_cn\":
                    print(cn)
                elif name == \"ellipj_dn\":
                    print(dn)
                else:
                    print(ph)
";

/// `ellipk`, `ellipkm1`, `ellipe`, `ellipkinc`, `ellipeinc`, `ellipj`, `elliprc`, `elliprd`,
/// `elliprf`, `elliprg` and `elliprj` at the 1,860 cases of `elliptic_functions.txt`: parameters
/// from −1e300 to within 1e-16 of 1, amplitudes out to ±12, Carlson's arguments from 1e-20 to
/// 1e20, the principal values of `elliprc` and `elliprj`, and `SciPy`'s values at the ends.
#[test]
fn elliptic_functions_match_the_recorded_values() {
    check(
        "special-elliptic",
        ELLIPTIC_FUNCTIONS,
        "elliptic_functions.txt",
    )
    .unwrap();
}
