//! The functions of `math` beyond the first six (roadmap 1.13.1), built and run: each gives
//! what CPython's `math` module prints for the same program, read from the C library as
//! Python's are. Results that C libraries round differently in the last bit are printed to 12
//! significant digits, so the test passes on macOS and Linux alike.

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use driver::{Profile, Session};
use std::process::Command;

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

/// `math.fsum` of lists and rank-1 arrays, views included (roadmap 1.13.16): each sum is the
/// exact sum rounded once, so it gives CPython's `math.fsum` bits where `sum` loses digits to
/// cancellation, and infinities and NaN give their IEEE 754 sum where CPython raises.
#[test]
fn fsum_rounds_the_exact_sum_once() {
    let program = "\
import math
from math import fsum, inf, nan


def main():
    tenths = [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1]
    print(fsum(tenths), sum(tenths) == 1.0, math.fsum([1e100, 1.0, -1e100, 1e-100, 1e50, -1.0, -1e50]))
    print(fsum([1, 1e-16, 1e-16, 1e-16]), fsum([1e16, 1.0, 1e-16]), fsum([]), fsum([-0.0, -0.0]))
    a = Array([1.0, 2.0, 3.0, 4.0, 5.0])
    table = Array.zeros((2, 2))
    table[1, 0] = 0.5
    table[1, 1] = 1e-20
    print(fsum(a), fsum(a[::2]), fsum(table[1, :]), fsum(Array([2.0 ** 53, 1.0, 1.0])))
    print(fsum([inf, 1.0]), fsum([inf, -inf]), fsum([nan, 1.0]), fsum([1e308, 1e308, -1e308]))
";
    prints(
        "math-fsum",
        program,
        "1.0 True 1e-100\n\
         1.0000000000000002 1.0000000000000002e+16 0.0 0.0\n\
         15.0 9.0 0.5 9007199254740994.0\n\
         inf nan nan inf\n",
    )
    .unwrap();
}

/// `math.f64x2`, the double-double float (decision 0051, roadmap 3.33): the four operations,
/// `sqrt`, comparisons and conversions. The square root of 2, a third and `0.1` print the 31
/// digits Python's `fractions` and `decimal` compute exactly; squaring the root back leaves a
/// residual near 1e-31, where an `f64` leaves one near 1e-16; edge values follow IEEE 754.
#[test]
fn f64x2_keeps_about_32_digits() {
    let program = "\
from math import f64x2, inf


def residual(a: f64, x: f64, b: f64) -> f64:
    product = f64x2(a, 0.0).mul(f64x2(x, 0.0))
    return f64x2(b, 0.0).sub(product).to_f64()


def main():
    one = f64x2(1.0, 0.0)
    three = f64x2(3.0, 0.0)
    root = f64x2(2.0, 0.0).sqrt()
    third = one.div(three)
    print(root.to_str(), root.hi, root.lo)
    print(third.to_str(), third.mul(three).eq(one), third.lt(f64x2(1.0 / 3.0, 0.0)))
    print(root.mul(root).sub(f64x2(2.0, 0.0)).to_str(), residual(0.1, 3.0, 0.3))
    tenth = f64x2(0.1, 0.0)
    print(tenth.to_str(), tenth.neg().abs().ge(tenth), tenth.add(tenth).to_f64())
    print(f64x2(-0.0, 0.0).to_str(), f64x2(5e-324, 0.0).to_str(), f64x2(-1.0, 0.0).sqrt().to_str())
    print(f64x2(inf, 0.0).add(one).to_str(), f64x2(1e300, 0.0).mul(f64x2(1e10, 0.0)).neg().to_str())
";
    prints(
        "math-f64x2",
        program,
        "1.414213562373095048801688724210e+00 1.4142135623730951 -9.667293313452916e-17\n\
         3.333333333333333333333333333333e-01 True False\n\
         -9.860761315262647567646607066035e-32 -2.7755575615628914e-17\n\
         1.000000000000000055511151231258e-01 True 0.2\n\
         -0.000000000000000000000000000000e+00 4.940656458412465441765687928682e-324 nan\n\
         inf -inf\n",
    )
    .unwrap();
}

/// `f64x2.of(x)` builds an `f64x2` from an `f64` exactly, as `f64x2(x, 0.0)` does (roadmap
/// 3.33.1, decision 0051 amended): `of(0.1)` keeps the `f64` nearest 0.1, while a quotient of
/// two exact values gives 0.1 to 32 digits. The constructor still works.
#[test]
fn f64x2_of_builds_one_from_an_f64() {
    let program = "\
from math import f64x2


def main():
    tenth = f64x2.of(1.0).div(f64x2.of(10.0))
    print(f64x2.of(0.1).to_str(), tenth.to_str())
    negative = f64x2.of(-3.0)
    print(f64x2.of(2.5).eq(f64x2(2.5, 0.0)), negative.lo)
";
    prints(
        "math-f64x2-of",
        program,
        "1.000000000000000055511151231258e-01 1.000000000000000000000000000000e-01\n\
         True 0.0\n",
    )
    .unwrap();
}

/// `math.trunc` is the checked float-to-integer conversion alone, with no separate rounding
/// call (roadmap 1.14.4), so it must round toward zero and fit-check exactly as before: halves
/// and negatives go toward zero as CPython's do, the largest `f64` below 2^63 and -2^63 itself
/// fit, an `f32` truncates, and 2^63 stops the program naming `i64`.
#[test]
fn trunc_rounds_toward_zero_and_checks_the_range() {
    let program = "\
import math
import sys


def main():
    big = 2.0 ** 63
    values = [-0.5, 0.999, -2.7, 4503599627370495.5, big - 1024.0, -big, float(len(sys.argv)) - 1.5]
    for value in values:
        print(math.trunc(value), f64(math.trunc(value)))
    small: f32 = -3.75
    print(math.trunc(small))
    print(math.trunc(big))
";
    let directory = package("math-trunc", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "\
0 0.0
0 0.0
-2 -2.0
4503599627370495 4503599627370495.0
9223372036854774784 9.223372036854775e+18
-9223372036854775808 -9.223372036854776e+18
0 0.0
-3
"
    );
    assert_eq!(run.status.code(), Some(101));
    assert_eq!(
        panic_report(&run),
        "panic: the value does not fit in `i64`\n  --> main.byn:12:11\n"
    );
}

/// `math.trunc` to `int` emits the checked conversion alone, never LLVM's `trunc`, which on
/// x86-64 CPUs without SSE4.1 is a call into the C math library that spills every float
/// register around it (roadmap 1.14.4). `math.floor` still rounds first, since the conversion
/// rounds toward zero.
#[test]
fn trunc_is_the_conversion_alone() {
    let mut session = Session::new();
    let program = "\
import math


def whole(x: f64) -> int:
    return math.trunc(x)


def below(x: f64) -> int:
    return math.floor(x)


def main():
    print(whole(2.5), below(2.5))
";
    session.add_file("main.byn", program.to_owned()).unwrap();
    let modules = session.compile_parts(None, Profile::Release, 1).unwrap();
    let text = modules.concat();
    assert!(!text.contains("@llvm.trunc.f64("), "{text}");
    assert!(text.contains("@llvm.floor.f64("), "{text}");
}
