//! The functions of `math` beyond the first six (roadmap 1.13.1), built and run: each gives
//! what CPython's `math` module prints for the same program, read from the C library as
//! Python's are. Results that C libraries round differently in the last bit are printed to 12
//! significant digits, so the test passes on macOS and Linux alike.

use super::examples::prints;

/// Every function of `math` for real numbers: the trigonometric, inverse and hyperbolic
/// functions, logarithms and exponentials, `erf` and `gamma`, `fma`, `fmod` beside `%`,
/// `floor`, `ceil` and `trunc` as `int`s, the `is` tests, `degrees` and `radians`, and `f32`
/// arguments staying `f32`. CPython prints the same lines.
#[test]
fn the_rest_of_math_builds_and_runs() {
    let program = "\
import math
from math import floor, isnan


def main():
    x = 0.5
    print(f\"{math.tan(x):.12g} {math.asin(x):.12g} {math.acos(x):.12g} {math.atan(x):.12g}\")
    print(f\"{math.atan2(1.0, -1.0):.12g} {math.hypot(3, 4)} {math.sinh(x):.12g} {math.cosh(x):.12g}\")
    print(f\"{math.tanh(x):.12g} {math.asinh(x):.12g} {math.acosh(2.0):.12g} {math.atanh(x):.12g}\")
    print(math.exp2(10), f\"{math.expm1(1e-10):.12g}\", math.log2(1024), math.log10(1000.0))
    print(f\"{math.log1p(1e-10):.12g}\", math.log(8, 2), f\"{math.cbrt(-27.0):.12g}\", math.fabs(-3))
    print(f\"{math.erf(x):.12g} {math.erfc(x):.12g}\", f\"{math.gamma(5.0):.12g}\", f\"{math.lgamma(10.5):.12g}\")
    print(math.copysign(2.0, -0.0), math.fmod(-7.5, 2.0), -7.5 % 2.0, math.fma(2.0, 3.0, 1.0))
    print(floor(-2.5), math.ceil(-2.5), math.trunc(-2.5), floor(7), math.ceil(2.000001))
    print(isnan(math.nan), isnan(1.0), math.isinf(-math.inf), math.isinf(1e308), math.isfinite(math.nan), math.isfinite(3))
    print(math.degrees(math.pi), math.radians(180), math.pow(2.0, -1))
    y: f32 = 2.0
    print(math.hypot(y, 1.5), math.fma(y, y, 0.25), floor(y), math.isinf(y), math.degrees(y) > 114.0)
";
    prints(
        "math-module",
        program,
        "0.546302489844 0.523598775598 1.0471975512 0.463647609001\n\
         2.35619449019 5.0 0.521095305494 1.12762596521\n\
         0.46211715726 0.48121182506 1.31695789692 0.549306144334\n\
         1024.0 1.00000000005e-10 10.0 3.0\n\
         9.9999999995e-11 3.0 -3 3.0\n\
         0.520499877813 0.479500122187 24 13.9406252194\n\
         -2.0 -1.5 0.5 7.0\n\
         -3 -2 -2 7 3\n\
         True False True False False True\n\
         180.0 3.141592653589793 0.5\n\
         2.5 4.25 2 False True\n",
    )
    .unwrap();
}
