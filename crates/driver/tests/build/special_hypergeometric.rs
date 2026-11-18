//! The confluent hypergeometric functions of `special` (roadmap 3.18.8), `hyp0f1`, `hyp1f1` and
//! `hyperu`, built and run and checked against the values mpmath records, as
//! [`super::special`] checks the other families, with its tolerances.

use super::special::check;

/// The program: reads `cases.txt`, one case a line, the name, the arguments, the size its error
/// is measured against and the reference, and prints the value, one line each.
const HYPERGEOMETRIC: &str = "\
from special import hyp0f1, hyp1f1, hyperu


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            if name == \"hyp0f1\":
                print(hyp0f1(float(fields[1]), float(fields[2])))
            elif name == \"hyp1f1\":
                print(hyp1f1(float(fields[1]), float(fields[2]), float(fields[3])))
            else:
                print(hyperu(float(fields[1]), float(fields[2]), float(fields[3])))
";

/// `hyp0f1`, `hyp1f1` and `hyperu` at the cases of `hypergeometric.txt`: parameters from −150
/// to 150, a of `hyp1f1` to ±100 and a of `hyperu` from −35, arguments from 1e-5 to ±3e4, and
/// the ends.
#[test]
fn hypergeometric_functions_match_the_recorded_values() {
    check(
        "special-hypergeometric",
        HYPERGEOMETRIC,
        "hypergeometric.txt",
    )
    .unwrap();
}
