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
