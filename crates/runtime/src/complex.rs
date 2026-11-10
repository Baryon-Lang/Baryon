//! Complex numbers: how one prints, and `z ** w`, each as Python's `complex` does it.
//!
//! Compiled code does complex `+`, `-`, `*`, `/` and `==` itself, part by part. What is left here
//! is too long to write inline: Python's `repr` of a complex number, and its power.
//!
//! **Printing.** Python writes each part as `repr` writes a float, but drops the `.0` of a whole
//! number, so `1+2j` prints `(1+2j)`, not `(1.0+2.0j)`. The imaginary part always has its sign.
//! When the real part is `+0.0`, Python writes the imaginary part alone, without parentheses:
//! `2j`, but `(-0-2j)` for `-2j`, whose real part is `-0.0`.
//!
//! **Power.** `z ** w` follows CPython 3.12's `complex_pow`: a whole exponent of at most 100 in
//! size is computed by repeated squaring, and its reciprocal for a negative one, which is exact
//! for small powers such as `z ** 2`; any other exponent goes through polar form,
//! `|z|^w.real * e^(-arg(z) * w.imag)` at angle `arg(z) * w.real + w.imag * ln|z|`. Python
//! raises `ZeroDivisionError` for zero to a negative or complex power; here, as `NumPy` gives, the
//! result is `nan+nanj`. Where Python raises `OverflowError`, the parts are infinite, as for
//! float `**` (language reference, "Math functions").

use crate::print;
use std::fmt::Write as _;

/// A complex number as its real and imaginary parts.
pub(crate) type Parts = (f64, f64);

/// The text Python's `repr` writes for the complex number whose parts, written by Rust's `{:e}`
/// with the fewest digits that read back as the same value at their own width, are `real` and
/// `imaginary`; `positive_zero` says whether the real part is `+0.0`.
pub(crate) fn repr(real: &str, imaginary: &str, positive_zero: bool) -> String {
    let imaginary = part(imaginary);
    if positive_zero {
        return format!("{imaginary}j");
    }
    let mut text = format!("({}", part(real));
    if !imaginary.starts_with('-') {
        text.push('+');
    }
    let _ = write!(text, "{imaginary}j)");
    text
}

/// One part as Python writes it in a complex number: as a float's `repr`, without the `.0` of a
/// whole number.
fn part(shortest: &str) -> String {
    let text = print::float(shortest);
    match text.strip_suffix(".0") {
        Some(whole) => whole.to_owned(),
        None => text,
    }
}

/// `base ** exponent`, as the module documentation says.
pub(crate) fn power(base: Parts, exponent: Parts) -> Parts {
    let (real, imaginary) = exponent;
    let result = if imaginary == 0.0 && real.fract() == 0.0 && real.abs() <= 100.0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a whole number of size at most 100"
        )]
        integer_power(base, real as i64)
    } else {
        polar_power(base, exponent)
    };
    result.unwrap_or((f64::NAN, f64::NAN))
}

/// `base ** n` for a whole `n`, as CPython's `c_powi`; `None` where Python raises, zero to a
/// negative power.
fn integer_power(base: Parts, n: i64) -> Option<Parts> {
    if n > 0 {
        Some(unsigned_power(base, n))
    } else {
        quotient((1.0, 0.0), unsigned_power(base, -n))
    }
}

/// `base ** n` for `n` of at least 0 by repeated squaring, as CPython's `c_powu`.
fn unsigned_power(base: Parts, n: i64) -> Parts {
    let (mut result, mut square, mut mask) = ((1.0, 0.0), base, 1_i64);
    while mask > 0 && n >= mask {
        if n & mask != 0 {
            result = product(result, square);
        }
        mask <<= 1;
        square = product(square, square);
    }
    result
}

/// `base ** exponent` through polar form, as CPython's `_Py_c_pow`; `None` where Python raises,
/// zero to a negative or complex power.
fn polar_power(base: Parts, exponent: Parts) -> Option<Parts> {
    if exponent == (0.0, 0.0) {
        return Some((1.0, 0.0));
    }
    if base == (0.0, 0.0) {
        if exponent.1 != 0.0 || exponent.0 < 0.0 {
            return None;
        }
        return Some((0.0, 0.0));
    }
    let size = base.0.hypot(base.1);
    let mut length = size.powf(exponent.0);
    let angle = base.1.atan2(base.0);
    let mut phase = angle * exponent.0;
    if exponent.1 != 0.0 {
        length /= (angle * exponent.1).exp();
        phase += exponent.1 * size.ln();
    }
    Some((length * phase.cos(), length * phase.sin()))
}

/// `a * b`, as CPython's `_Py_c_prod`.
fn product(a: Parts, b: Parts) -> Parts {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// `a / b` by Smith's method, as CPython's `_Py_c_quot`; `None` when `b` is zero, where Python
/// raises.
fn quotient(a: Parts, b: Parts) -> Option<Parts> {
    let (real_size, imaginary_size) = (b.0.abs(), b.1.abs());
    if real_size >= imaginary_size {
        if real_size == 0.0 {
            return None;
        }
        let ratio = b.1 / b.0;
        let denominator = b.0 + b.1 * ratio;
        Some((
            (a.0 + a.1 * ratio) / denominator,
            (a.1 - a.0 * ratio) / denominator,
        ))
    } else if imaginary_size >= real_size {
        let ratio = b.0 / b.1;
        let denominator = b.0 * ratio + b.1;
        Some((
            (a.0 * ratio + a.1) / denominator,
            (a.1 * ratio - a.0) / denominator,
        ))
    } else {
        Some((f64::NAN, f64::NAN))
    }
}

#[cfg(test)]
mod tests {
    use super::{power, repr};

    /// Python's `repr` of `complex(real, imaginary)`.
    fn python(real: f64, imaginary: f64) -> String {
        let positive_zero = real == 0.0 && real.is_sign_positive();
        repr(
            &format!("{real:e}"),
            &format!("{imaginary:e}"),
            positive_zero,
        )
    }

    #[test]
    fn complex_numbers_print_as_python_writes_them() {
        assert_eq!(python(1.0, 2.0), "(1+2j)");
        assert_eq!(python(1.5, -0.25), "(1.5-0.25j)");
        assert_eq!(python(0.0, 2.0), "2j");
        assert_eq!(python(0.0, 0.0), "0j");
        assert_eq!(python(0.0, -0.0), "-0j");
        assert_eq!(python(-0.0, -2.0), "(-0-2j)");
        assert_eq!(python(1e16, 1e-5), "(1e+16+1e-05j)");
        assert_eq!(python(0.1 + 0.2, 3.0), "(0.30000000000000004+3j)");
        assert_eq!(python(f64::INFINITY, f64::NAN), "(inf+nanj)");
        assert_eq!(python(f64::NAN, f64::NEG_INFINITY), "(nan-infj)");
        assert_eq!(python(2.0, -f64::NAN), "(2+nanj)");
    }

    #[test]
    fn complex64_parts_print_their_own_shortest_digits() {
        assert_eq!(
            repr(&format!("{:e}", 0.1_f32), &format!("{:e}", 0.2_f32), false),
            "(0.1+0.2j)"
        );
    }

    /// Each expected value is what CPython 3.12 prints for the same power.
    #[test]
    fn powers_match_python() {
        assert_eq!(power((1.0, 2.0), (2.0, 0.0)), (-3.0, 4.0));
        assert_eq!(power((1.0, 2.0), (0.0, 0.0)), (1.0, 0.0));
        assert_eq!(power((0.0, 1.0), (-1.0, 0.0)), (0.0, -1.0));
        assert_eq!(power((1.0, 1.0), (3.0, 0.0)), (-2.0, 2.0));
        // (1+1j) ** 0.5 == (1.0986841134678098+0.45508986056222733j)
        assert_eq!(
            power((1.0, 1.0), (0.5, 0.0)),
            (1.098_684_113_467_809_8, 0.455_089_860_562_227_33)
        );
        // 1j ** 1j == (0.20787957635076193+0j)
        assert_eq!(
            power((0.0, 1.0), (0.0, 1.0)),
            (0.207_879_576_350_761_93, 0.0)
        );
        assert_eq!(power((0.0, 0.0), (2.5, 0.0)), (0.0, 0.0));
    }

    #[test]
    fn zero_to_a_negative_or_complex_power_is_nan_as_numpy_gives() {
        for exponent in [(-1.0, 0.0), (-0.5, 0.0), (1.0, 1.0)] {
            let (real, imaginary) = power((0.0, 0.0), exponent);
            assert!(real.is_nan() && imaginary.is_nan(), "{exponent:?}");
        }
    }
}
