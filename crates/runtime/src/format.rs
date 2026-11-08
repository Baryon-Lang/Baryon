//! Formatting a value with a format specification, as Python's `format` does: the work behind
//! each value of an f-string (decision 0022).
//!
//! The compiler parses and checks each specification (`types::FormatSpec`) and passes its parts
//! as a [`FormatSpec`], so nothing here can meet a specification Python would reject. Every rule follows
//! Python's format specification mini-language: the default alignment is right for numbers and
//! left for text, `0` pads after the sign, `,` and `_` group digits in threes (`_` in fours for
//! binary, octal and hexadecimal), and `g` drops trailing zeros. A value with the empty
//! specification is written as `print` writes it, so `f"{x}"` and `print(x)` agree.

use crate::print;

/// A format specification's parts, laid out for C: the form the compiler passes them in, as a
/// constant `{ i32, i8, i8, i8, i8, i8, i8, i64, i64 }`. A null pointer in its place means the
/// empty specification.
///
/// # Example
///
/// `.6f`:
///
/// ```
/// let spec = runtime::FormatSpec { presentation: b'f', precision: 6, ..runtime::FormatSpec::DEFAULT };
/// assert_eq!(spec.width, -1);
/// ```
///
/// Each field holds a character's code, or 0 or -1 where the specification leaves the part out,
/// as each field says.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    /// The fill character's code point; a space by default.
    pub fill: u32,
    /// `<`, `>`, `^` or `=`, or 0 for the default.
    pub align: u8,
    /// `+`, `-` or a space.
    pub sign: u8,
    /// 1 for `#`, the alternate form; 0 otherwise.
    pub alternate: u8,
    /// 1 for `0` before the width; 0 otherwise.
    pub zero: u8,
    /// `,` or `_`, or 0 for no grouping.
    pub grouping: u8,
    /// The type letter, such as `f`, or 0 for none.
    pub presentation: u8,
    /// The width, or -1 for none.
    pub width: i64,
    /// The precision, or -1 for none.
    pub precision: i64,
}

impl FormatSpec {
    /// The empty specification, which writes a value as `print` does.
    pub const DEFAULT: Self = Self {
        fill: b' ' as u32,
        align: 0,
        sign: b'-',
        alternate: 0,
        zero: 0,
        grouping: 0,
        presentation: 0,
        width: -1,
        precision: -1,
    };

    /// The precision, if the specification has one.
    fn precision(&self) -> Option<usize> {
        usize::try_from(self.precision).ok()
    }

    /// Whether it is the empty specification.
    fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }
}

/// `text` formatted with `spec`: cut to the precision, then padded to the width.
pub(crate) fn text(text: &str, spec: &FormatSpec) -> String {
    let cut: String = match spec.precision() {
        Some(precision) => text.chars().take(precision).collect(),
        None => text.to_owned(),
    };
    pad(&cut, "", spec, b'<')
}

/// The signed integer `value` formatted with `spec`.
pub(crate) fn signed(value: i64, spec: &FormatSpec) -> String {
    integer(value < 0, value.unsigned_abs(), spec)
}

/// The unsigned integer `value` formatted with `spec`.
pub(crate) fn unsigned(value: u64, spec: &FormatSpec) -> String {
    integer(false, value, spec)
}

/// `value` as a `bool`: `True` or `False` with the empty specification, and the integer 1 or 0
/// with any other, as Python formats a `bool`.
pub(crate) fn boolean(value: bool, spec: &FormatSpec) -> String {
    if spec.is_default() {
        print::bool(value).to_owned()
    } else {
        unsigned(u64::from(value), spec)
    }
}

/// An integer, negative when `negative`, of magnitude `magnitude`, formatted with `spec`.
fn integer(negative: bool, magnitude: u64, spec: &FormatSpec) -> String {
    let (digits, prefix, group) = match spec.presentation {
        b'b' => (format!("{magnitude:b}"), "0b", 4),
        b'o' => (format!("{magnitude:o}"), "0o", 4),
        b'x' => (format!("{magnitude:x}"), "0x", 4),
        b'X' => (format!("{magnitude:X}"), "0X", 4),
        0 | b'd' => (magnitude.to_string(), "", 3),
        _ => {
            // A float presentation: the integer is converted, as Python converts it.
            #[allow(
                clippy::cast_precision_loss,
                reason = "Python rounds a large integer to the nearest float here too"
            )]
            let value = magnitude as f64;
            return float(if negative { -value } else { value }, false, spec);
        }
    };
    let prefix = if spec.alternate == 1 { prefix } else { "" };
    let sign = sign(negative, spec);
    number(&sign, prefix, &digits, "", group, spec)
}

/// `value` formatted with `spec`. `single` says the value is an `f32`, whose empty
/// specification writes its own shortest digits, as `print` does.
pub(crate) fn float(value: f64, single: bool, spec: &FormatSpec) -> String {
    let sign = sign(value.is_sign_negative() && !value.is_nan(), spec);
    let magnitude = value.abs();
    let upper = spec.presentation.is_ascii_uppercase();
    if !magnitude.is_finite() {
        let word = if magnitude.is_nan() { "nan" } else { "inf" };
        let word = if upper {
            word.to_uppercase()
        } else {
            word.to_owned()
        };
        let suffix = if spec.presentation == b'%' { "%" } else { "" };
        return pad(&format!("{word}{suffix}"), &sign, spec, b'>');
    }
    let alternate = spec.alternate == 1;
    let (body, suffix) = match (spec.presentation, spec.precision()) {
        (0, None) => {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "an `f32` value widened to `f64` narrows back exactly"
            )]
            let shortest = if single {
                format!("{:e}", magnitude as f32)
            } else {
                format!("{magnitude:e}")
            };
            (print::float(&shortest), "")
        }
        (0, Some(precision)) => (general(magnitude, precision, alternate, true), ""),
        (b'f' | b'F', precision) => (fixed(magnitude, precision.unwrap_or(6), alternate), ""),
        (b'e' | b'E', precision) => (exponent(magnitude, precision.unwrap_or(6), alternate), ""),
        (b'g' | b'G', precision) => (
            general(magnitude, precision.unwrap_or(6), alternate, false),
            "",
        ),
        (b'%', precision) => (
            fixed(magnitude * 100.0, precision.unwrap_or(6), alternate),
            "%",
        ),
        _ => (print::float(&format!("{magnitude:e}")), ""),
    };
    let body = if upper { body.to_uppercase() } else { body };
    // Only the digits before the point are grouped.
    let split = body
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(body.len());
    let (digits, rest) = body.split_at(split);
    number(&sign, "", digits, &format!("{rest}{suffix}"), 3, spec)
}

/// `value`, not negative, in fixed point with `precision` digits after the point; a point with
/// no digits after it when `alternate` and the precision is 0.
fn fixed(value: f64, precision: usize, alternate: bool) -> String {
    let mut text = format!("{value:.precision$}");
    if alternate && precision == 0 {
        text.push('.');
    }
    text
}

/// `value`, not negative, in scientific notation with `precision` digits after the point and a
/// signed exponent of at least two digits, as `1.500000e+03`.
fn exponent(value: f64, precision: usize, alternate: bool) -> String {
    let text = format!("{value:.precision$e}");
    let (mantissa, power) = text.split_once('e').unwrap_or((&text, "0"));
    let power: i32 = power.parse().unwrap_or(0);
    let point = if alternate && precision == 0 { "." } else { "" };
    let sign = if power < 0 { '-' } else { '+' };
    format!("{mantissa}{point}e{sign}{:02}", power.unsigned_abs())
}

/// `value`, not negative, with `precision` significant digits, in fixed point when its exponent
/// is from -4 up to the precision and in scientific notation otherwise, without trailing zeros
/// unless `alternate`. `keep_point` is the empty type letter's rule: fixed point keeps one digit
/// after the point, and scientific notation starts one exponent sooner.
fn general(value: f64, precision: usize, alternate: bool, keep_point: bool) -> String {
    let precision = precision.max(1);
    let rounded = format!("{value:.*e}", precision - 1);
    let power: i64 = rounded
        .split_once('e')
        .and_then(|(_, power)| power.parse().ok())
        .unwrap_or(0);
    let limit = i64::try_from(precision).unwrap_or(i64::MAX) - i64::from(keep_point);
    let mut text = if (-4..limit.max(1)).contains(&power) || (value == 0.0 && !keep_point) {
        let decimals =
            usize::try_from(i64::try_from(precision).unwrap_or(i64::MAX) - 1 - power).unwrap_or(0);
        fixed(value, decimals, alternate)
    } else {
        exponent(value, precision - 1, alternate)
    };
    if !alternate {
        let (mantissa, power) = match text.find('e') {
            Some(at) => (text[..at].to_owned(), text[at..].to_owned()),
            None => (text.clone(), String::new()),
        };
        let mut mantissa = if mantissa.contains('.') {
            mantissa
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned()
        } else {
            mantissa
        };
        if keep_point && power.is_empty() && !mantissa.contains('.') {
            mantissa.push_str(".0");
        }
        text = format!("{mantissa}{power}");
    }
    text
}

/// The sign a number is written with: `-` when `negative`, and otherwise what the
/// specification asks for.
fn sign(negative: bool, spec: &FormatSpec) -> String {
    match (negative, spec.sign) {
        (true, _) => "-".to_owned(),
        (false, b'+') => "+".to_owned(),
        (false, b' ') => " ".to_owned(),
        _ => String::new(),
    }
}

/// A number from its parts, grouped and padded: the sign, the prefix such as `0x`, the integer
/// `digits`, then `rest`, such as the point and decimals. `group` is how many digits a separator
/// follows. Zero padding goes between the prefix and the digits, grouped too, as in Python.
fn number(
    sign: &str,
    prefix: &str,
    digits: &str,
    rest: &str,
    group: usize,
    spec: &FormatSpec,
) -> String {
    let separator = char::from(spec.grouping);
    let grouped = |digits: &str| {
        if spec.grouping == 0 {
            return digits.to_owned();
        }
        let mut text = String::new();
        for (position, digit) in digits.chars().enumerate() {
            if position > 0 && (digits.len() - position).is_multiple_of(group) {
                text.push(separator);
            }
            text.push(digit);
        }
        text
    };
    let zero_padded = spec.zero == 1 && spec.align == 0;
    let width = usize::try_from(spec.width).unwrap_or(0);
    let mut digits = digits.to_owned();
    if zero_padded {
        // Lengthen the digits until the whole number, grouped, fills the width.
        let fixed = sign.chars().count() + prefix.len() + rest.chars().count();
        while fixed + grouped(&digits).chars().count() < width {
            digits.insert(0, '0');
        }
    }
    let body = format!("{prefix}{}{rest}", grouped(&digits));
    pad(&body, sign, spec, b'>')
}

/// `body` after `sign`, padded to the specification's width with its fill character where its
/// alignment says; `default` is the alignment when the specification gives none.
fn pad(body: &str, sign: &str, spec: &FormatSpec, default: u8) -> String {
    let length = sign.chars().count() + body.chars().count();
    let width = usize::try_from(spec.width).unwrap_or(0);
    let fill = char::from_u32(spec.fill).unwrap_or(' ');
    let (fill, align) = match (spec.zero, spec.align) {
        // `0` pads a number after its sign, and text, which has none, at its end.
        (1, 0) => (
            if fill == ' ' { '0' } else { fill },
            if default == b'<' { b'<' } else { b'=' },
        ),
        (_, 0) => (fill, default),
        (_, align) => (fill, align),
    };
    let missing = width.saturating_sub(length);
    let padding = |count: usize| fill.to_string().repeat(count);
    match align {
        b'<' => format!("{sign}{body}{}", padding(missing)),
        b'^' => format!(
            "{}{sign}{body}{}",
            padding(missing / 2),
            padding(missing - missing / 2)
        ),
        b'=' => format!("{sign}{}{body}", padding(missing)),
        _ => format!("{}{sign}{body}", padding(missing)),
    }
}

#[cfg(test)]
mod tests;
