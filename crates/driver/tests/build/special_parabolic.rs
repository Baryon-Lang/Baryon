//! The parabolic cylinder functions of `special` (roadmap 3.18.14), `pbdv` and `pbvv` with their
//! derivatives, built and run and checked against the values mpmath records, as
//! [`super::special`] checks the other families, with its tolerances.

use super::special::check;

/// The program: reads `cases.txt`, one case a line, the name, v and x, the size its error is
/// measured against and the reference, and prints the value or the derivative, one line each.
const PARABOLIC: &str = "\
from special import pbdv, pbvv


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            v = float(fields[1])
            x = float(fields[2])
            if name == \"pbdv\":
                print(pbdv(v, x)[0])
            elif name == \"pbdv_d\":
                print(pbdv(v, x)[1])
            elif name == \"pbvv\":
                print(pbvv(v, x)[0])
            else:
                print(pbvv(v, x)[1])
";

/// `pbdv` and `pbvv` and their derivatives at the 1,766 cases of `parabolic.txt`: orders from
/// −60 to 60, arguments from −60 to 60 and down to 1e-4, and the ends.
#[test]
fn parabolic_cylinder_functions_match_the_recorded_values() {
    check("special-parabolic", PARABOLIC, "parabolic.txt").unwrap();
}
