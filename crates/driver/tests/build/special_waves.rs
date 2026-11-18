//! The Airy, Kelvin and Struve functions of `special` (roadmap 3.18.7), built and run and
//! checked against the values mpmath records, as [`super::special`] checks the other families,
//! with its tolerances.

use super::special::check;

/// The program for Airy: reads `cases.txt`, one case a line, the name (`airy_` or `airye_`
/// and the part, `ai`, `aip`, `bi` or `bip`), the argument, the size its error is measured
/// against and the reference, and prints the part, one line each.
const AIRY: &str = "\
from special import airy, airye


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            x = float(fields[1])
            ai = 0.0
            ai_slope = 0.0
            bi = 0.0
            bi_slope = 0.0
            if name.startswith(\"airye_\"):
                ai, ai_slope, bi, bi_slope = airye(x)
            else:
                ai, ai_slope, bi, bi_slope = airy(x)
            if name.endswith(\"_aip\"):
                print(ai_slope)
            elif name.endswith(\"_ai\"):
                print(ai)
            elif name.endswith(\"_bip\"):
                print(bi_slope)
            else:
                print(bi)
";

/// The program for the Kelvin functions: the name, the argument, the size and the reference.
const KELVIN: &str = "\
from special import bei, beip, ber, berp, kei, keip, ker, kerp


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            x = float(fields[1])
            if name == \"ber\":
                print(ber(x))
            elif name == \"bei\":
                print(bei(x))
            elif name == \"ker\":
                print(ker(x))
            elif name == \"kei\":
                print(kei(x))
            elif name == \"berp\":
                print(berp(x))
            elif name == \"beip\":
                print(beip(x))
            elif name == \"kerp\":
                print(kerp(x))
            else:
                print(keip(x))
";

/// The program for the Struve functions: the name, the order, the argument, the size and the
/// reference.
const STRUVE: &str = "\
from special import modstruve, struve


def main() -> Raises[None, OSError | ValueError]:
    with open(\"cases.txt\") as cases:
        for line in cases:
            fields = line.split()
            name = fields[0].copy()
            v = float(fields[1])
            x = float(fields[2])
            if name == \"struve\":
                print(struve(v, x))
            else:
                print(modstruve(v, x))
";

/// `airy` and `airye` at the 3,580 cases of `airy.txt`: 457 arguments from −1e4 to 1e8,
/// each part of each, and the ends.
#[test]
fn airy_functions_match_the_recorded_values() {
    check("special-airy", AIRY, "airy.txt").unwrap();
}

/// `ber`, `bei`, `ker`, `kei` and their derivatives at the 2,892 cases of `kelvin.txt`:
/// arguments from −50 to 1,000 and the ends.
#[test]
fn kelvin_functions_match_the_recorded_values() {
    check("special-kelvin", KELVIN, "kelvin.txt").unwrap();
}

/// `struve` and `modstruve` at the 1,181 cases of `struve.txt`: orders from −10 to 100,
/// arguments from 1e-3 to 1e9, and the ends.
#[test]
fn struve_functions_match_the_recorded_values() {
    check("special-struve", STRUVE, "struve.txt").unwrap();
}
