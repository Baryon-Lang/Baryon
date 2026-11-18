//! The Bessel functions of real order and the spherical Bessel functions of `special` (roadmap
//! 3.18.5), built and run and checked against the values mpmath records, as [`super::special`]
//! checks the other families, with its tolerances.

use super::special::check;

/// The program: reads `cases.txt`, one case a line, the function's name, the order and the
/// argument, then the order of the derivative for `jvp`, `yvp`, `ivp` and `kvp`, or 1 for the
/// derivative of a spherical function and 0 for its value, then the size its error is measured
/// against and the reference, and prints what the function gives, one line each.
const BESSEL_REAL: &str = "\
from special import iv, ive, ivp, jv, jve, jvp, kv, kve, kvp, yv, yve, yvp
from special import spherical_in, spherical_jn, spherical_kn, spherical_yn


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            v = float(fields[1])
            x = float(fields[2])
            if name.startswith(\"spherical_\"):
                n = int(fields[1])
                derivative = fields[3] == \"1\"
                if name == \"spherical_jn\":
                    print(spherical_jn(n, x, derivative))
                elif name == \"spherical_yn\":
                    print(spherical_yn(n, x, derivative))
                elif name == \"spherical_in\":
                    print(spherical_in(n, x, derivative))
                else:
                    print(spherical_kn(n, x, derivative))
            elif name == \"jv\":
                print(jv(v, x))
            elif name == \"yv\":
                print(yv(v, x))
            elif name == \"iv\":
                print(iv(v, x))
            elif name == \"kv\":
                print(kv(v, x))
            elif name == \"jve\":
                print(jve(v, x))
            elif name == \"yve\":
                print(yve(v, x))
            elif name == \"ive\":
                print(ive(v, x))
            elif name == \"kve\":
                print(kve(v, x))
            elif name == \"jvp\":
                print(jvp(v, x, int(fields[3])))
            elif name == \"yvp\":
                print(yvp(v, x, int(fields[3])))
            elif name == \"ivp\":
                print(ivp(v, x, int(fields[3])))
            else:
                print(kvp(v, x, int(fields[3])))
";

/// `jv`, `yv`, `iv`, `kv`, their scaled forms and derivatives, and the four spherical functions
/// and their derivatives, at the 6,348 cases of `bessel_real.txt`: orders up to 60 of either
/// sign and from 60 to 250, arguments from 1e-3 to 1e8, and `SciPy`'s values, or the limits,
/// at the ends.
#[test]
fn real_order_bessel_functions_match_the_recorded_values() {
    check("special-bessel-real", BESSEL_REAL, "bessel_real.txt").unwrap();
}
