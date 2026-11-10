//! The one format every float of a printed array shares, as `NumPy`'s `FloatingFormat` chooses
//! it with the default options: `precision=8`, `floatmode='maxprec'`, `suppress=False`.
//!
//! - **Notation.** Scientific when the largest magnitude of the finite, non-zero elements
//!   reaches 10⁸ (10⁶ for `f32`, whose precision `NumPy` takes as 6 digits), the smallest is
//!   below 10⁻⁴, or the largest is more than 1000 times the smallest. Positional otherwise.
//! - **Digits.** Each element's fewest digits that read back as the same value, but at most 8
//!   after the point; a value that needs more is rounded to 8 and its trailing zeros dropped.
//!   So `0.1 + 0.2` prints `0.3`, and `1.0` prints `1.`.
//! - **Width.** Positional: the digits before the point right-aligned to the widest, those
//!   after left-aligned and padded with spaces, so `[1.  2.5]`. Scientific: every element gets
//!   as many digits after the point as the one that needs most, padded with zeros, and the
//!   exponent as many digits as the widest, at least two: `[1.5e-05 1.0e+03]`.
//! - **`nan` and `inf`** right-aligned to the width of the others.
//!
//! Rust's `{:e}` gives the same fewest digits as `NumPy`'s Dragon4 in unique mode, and Rust's
//! `{:.8}` and `{:.8e}` round the exact binary value, halves to even, as Dragon4 does at its
//! cutoff.

use std::fmt::{Display, LowerExp};

/// The most digits a float shows after the point; `NumPy`'s default `precision`.
const PRECISION: usize = 8;

/// A float width an array can hold: `f32` or `f64`.
pub(crate) trait Float: Copy + Display + LowerExp + PartialOrd {
    /// The magnitude from which a positional array switches to scientific notation: 10 to the
    /// power of the type's decimal precision, at most 8.
    const SCIENTIFIC_FROM: f64;
    /// The value, widened exactly to `f64`.
    fn wide(self) -> f64;
    /// `self / other` computed in the type's own arithmetic, as `NumPy` divides them, widened.
    fn ratio(self, other: Self) -> f64;
    /// The value with its sign dropped.
    fn absolute(self) -> Self;
}

impl Float for f32 {
    const SCIENTIFIC_FROM: f64 = 1e6;
    fn wide(self) -> f64 {
        f64::from(self)
    }
    fn ratio(self, other: Self) -> f64 {
        f64::from(self / other)
    }
    fn absolute(self) -> Self {
        self.abs()
    }
}

impl Float for f64 {
    const SCIENTIFIC_FROM: f64 = 1e8;
    fn wide(self) -> f64 {
        self
    }
    fn ratio(self, other: Self) -> f64 {
        self / other
    }
    fn absolute(self) -> Self {
        self.abs()
    }
}

/// Which values show a sign.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sign {
    /// Only negative ones: `NumPy`'s `sign='-'`, the default.
    Negative,
    /// Every one, `+` for the rest: `NumPy`'s `sign='+'`, used for a complex number's imaginary
    /// part.
    Always,
}

/// The format chosen for one array's floats, as [`FloatFormat::new`] chooses it.
pub(crate) struct FloatFormat {
    /// Which values show a sign.
    sign: Sign,
    /// `Some` with the exponent's digit count for scientific notation, `None` for positional.
    exponent_digits: Option<usize>,
    /// Scientific notation only: the digits every element shows after the point.
    precision: usize,
    /// The width of the part before the point, the sign included.
    pad_left: usize,
    /// The width of the part after the point: the digits, and in scientific notation the
    /// exponent too.
    pad_right: usize,
}

/// A finite value's digits, split as they are written.
struct Digits {
    /// The sign, if shown, and the digits before the point.
    whole: String,
    /// The digits after the point, with no trailing zeros.
    fraction: String,
    /// The decimal exponent, in scientific notation only.
    exponent: i32,
}

impl FloatFormat {
    /// The format for the floats `values`, all of one array, with signs shown as `sign` says.
    pub(crate) fn new<T: Float>(values: &[T], sign: Sign) -> Self {
        let finite: Vec<T> = values
            .iter()
            .copied()
            .filter(|value| value.wide().is_finite())
            .collect();
        let magnitudes = finite
            .iter()
            .filter(|value| value.wide() != 0.0)
            .map(|value| (value.wide().abs(), *value));
        let scientific = magnitudes
            .clone()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .zip(magnitudes.min_by(|a, b| a.0.total_cmp(&b.0)))
            .is_some_and(|((largest, wide), (smallest, narrow))| {
                let ratio = wide.absolute().ratio(narrow.absolute());
                largest >= T::SCIENTIFIC_FROM || smallest < 1e-4 || ratio > 1000.0
            });
        let mut format = Self {
            sign,
            exponent_digits: None,
            precision: 0,
            pad_left: 0,
            pad_right: 0,
        };
        if scientific && !finite.is_empty() {
            let digits: Vec<Digits> = finite
                .iter()
                .map(|&value| scientific_digits(value, sign))
                .collect();
            let exponent_digits = digits
                .iter()
                .map(|digits| digits.exponent.unsigned_abs().to_string().len().max(2))
                .max()
                .unwrap_or(2);
            format.precision = digits.iter().map(|d| d.fraction.len()).max().unwrap_or(0);
            format.pad_left = digits.iter().map(|d| d.whole.len()).max().unwrap_or(0);
            format.pad_right = exponent_digits + 2 + format.precision;
            format.exponent_digits = Some(exponent_digits);
        } else if !finite.is_empty() {
            let digits = finite.iter().map(|&value| positional_digits(value, sign));
            for digits in digits {
                format.pad_left = format.pad_left.max(digits.whole.len());
                format.pad_right = format.pad_right.max(digits.fraction.len());
            }
        }
        if finite.len() != values.len() {
            // `nan` and `inf` take the whole width, the point included.
            let negative_infinity = sign == Sign::Always
                || values.iter().any(|value| value.wide() == f64::NEG_INFINITY);
            let room = format.pad_right + 1;
            format.pad_left = format
                .pad_left
                .max(3_usize.saturating_sub(room))
                .max((3 + usize::from(negative_infinity)).saturating_sub(room));
        }
        format
    }

    /// `value` written in this format, padded to its width.
    pub(crate) fn write<T: Float>(&self, value: T) -> String {
        let wide = value.wide();
        if !wide.is_finite() {
            let text = match (wide.is_nan(), wide < 0.0, self.sign) {
                (true, _, Sign::Negative) => "nan",
                (true, _, Sign::Always) => "+nan",
                (false, true, _) => "-inf",
                (false, false, Sign::Negative) => "inf",
                (false, false, Sign::Always) => "+inf",
            };
            let width = self.pad_left + self.pad_right + 1;
            return format!("{text:>width$}");
        }
        let (pad_left, pad_right) = (self.pad_left, self.pad_right);
        if let Some(exponent_digits) = self.exponent_digits {
            let digits = scientific_digits(value, self.sign);
            let exponent_sign = if digits.exponent < 0 { '-' } else { '+' };
            return format!(
                "{:>pad_left$}.{:0<precision$}e{exponent_sign}{:0>exponent_digits$}",
                digits.whole,
                digits.fraction,
                digits.exponent.unsigned_abs(),
                precision = self.precision,
            );
        }
        let digits = positional_digits(value, self.sign);
        format!(
            "{:>pad_left$}.{:<pad_right$}",
            digits.whole, digits.fraction
        )
    }
}

/// The sign `value` shows: `-` for a negative value or negative zero, `+` for any other when
/// `sign` is [`Sign::Always`], and nothing otherwise.
fn sign_of<T: Float>(value: T, sign: Sign) -> &'static str {
    if format!("{value:e}").starts_with('-') {
        "-"
    } else if sign == Sign::Always {
        "+"
    } else {
        ""
    }
}

/// `value`'s digits in positional notation: its fewest digits when they need at most
/// [`PRECISION`] after the point, else rounded to [`PRECISION`], trailing zeros dropped.
fn positional_digits<T: Float>(value: T, sign: Sign) -> Digits {
    let shortest = format!("{value:e}");
    let (mantissa, exponent) = split_exponent(&shortest);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let point = i64::from(exponent) + 1;
    let after = i64::try_from(digits.len()).unwrap_or(i64::MAX) - point;
    let few = usize::try_from(after).map_or(true, |after| after <= PRECISION);
    let (whole, fraction) = if few {
        if point <= 0 {
            let zeros = "0".repeat(usize::try_from(-point).unwrap_or(0));
            ("0".to_owned(), format!("{zeros}{digits}"))
        } else {
            let point = usize::try_from(point).unwrap_or(0);
            if digits.len() <= point {
                (format!("{digits:0<point$}"), String::new())
            } else {
                let (whole, fraction) = digits.split_at(point);
                (whole.to_owned(), fraction.to_owned())
            }
        }
    } else {
        let rounded = format!("{value:.PRECISION$}");
        let rounded = rounded.trim_start_matches('-');
        let (whole, fraction) = rounded.split_once('.').unwrap_or((rounded, ""));
        (whole.to_owned(), fraction.to_owned())
    };
    Digits {
        whole: format!("{}{whole}", sign_of(value, sign)),
        fraction: fraction.trim_end_matches('0').to_owned(),
        exponent: 0,
    }
}

/// `value`'s digits in scientific notation: its fewest digits when they need at most
/// [`PRECISION`] after the point, else rounded to [`PRECISION`], trailing zeros dropped.
fn scientific_digits<T: Float>(value: T, sign: Sign) -> Digits {
    let shortest = format!("{value:e}");
    let (mantissa, _) = split_exponent(&shortest);
    let after = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let text = if after <= PRECISION {
        shortest
    } else {
        format!("{value:.PRECISION$e}")
    };
    let (mantissa, exponent) = split_exponent(&text);
    let mantissa = mantissa.trim_start_matches('-');
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    Digits {
        whole: format!("{}{whole}", sign_of(value, sign)),
        fraction: fraction.trim_end_matches('0').to_owned(),
        exponent,
    }
}

/// Rust's `{:e}` text split into its mantissa and its decimal exponent.
fn split_exponent(text: &str) -> (&str, i32) {
    match text.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, exponent.parse().unwrap_or(0)),
        None => (text, 0),
    }
}
