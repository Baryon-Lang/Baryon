//! `print`: how numbers, `bool` and text appear on standard output.
//!
//! The language prints values as Python does (decision 0004, Python's spelling and behavior where
//! Python has them), so a script ported from Python prints the same lines. Compiled code calls
//! one runtime function per argument, and prints the separator and the line end with
//! [`baryon_rt_print_str`](crate::baryon_rt_print_str): `print(a, b)` is four calls. Each call names
//! its stream: 1 for standard output, where `print` writes, or 2 for standard error, where the
//! report of an error that ends `main` goes.
//!
//! A float prints as Python's `repr` writes it: the fewest digits that read back as the same
//! value, `.0` on a whole number, and an exponent below `1e-4` and from `1e16` up.
//!
//! Output goes through Rust's standard output, which holds a line until it ends and then writes
//! it, so a program that prints many lines makes one system call per line. Each argument is a
//! separate call, so the pieces of lines printed by different threads of a `prange` loop can
//! interleave. A failed write, such as to a closed pipe, is ignored: the program carries on, as
//! one printing to `head` should.

use std::fmt::Write as _;
use std::io::Write as _;

/// Writes `text` to standard error when `stream` is 2, the POSIX number of standard error, and
/// to standard output otherwise, ignoring a failed write.
pub(crate) fn write(stream: i32, text: &str) {
    // A program whose output is closed has nowhere to report it; it carries on.
    let _ = if stream == STDERR {
        std::io::stderr().lock().write_all(text.as_bytes())
    } else {
        std::io::stdout().lock().write_all(text.as_bytes())
    };
}

/// The stream number of standard error, as POSIX numbers it. `print` writes to 1, standard
/// output; the report of an error that ends `main` writes to 2.
const STDERR: i32 = 2;

/// `value` as Python writes a `bool`: `True` or `False`.
pub(crate) fn bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// `value` as Python's `repr` writes a float. `shortest` is Rust's `{:e}` form of it, which has
/// the fewest digits that read back as the same value at its own precision: the caller formats
/// an `f32` and an `f64` alike.
///
/// Rust and Python pick the same digits; they differ only in where they put the point. Python
/// writes `d.ddde±XX` when the decimal exponent `X` is below -4 or at least 16, with at least
/// two exponent digits, and plain digits otherwise, with `.0` after a whole number.
pub(crate) fn float(shortest: &str) -> String {
    let (sign, unsigned) = match shortest.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", shortest),
    };
    let Some((mantissa, exponent)) = unsigned.split_once('e') else {
        // `inf` and `NaN`, which have no exponent.
        return format!("{sign}{}", unsigned.to_lowercase());
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return shortest.to_owned();
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let mut text = String::from(sign);
    if !(-4..16).contains(&exponent) {
        let (first, rest) = digits.split_at(1);
        text.push_str(first);
        if !rest.is_empty() {
            text.push('.');
            text.push_str(rest);
        }
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(text, "e{exponent_sign}{:02}", exponent.unsigned_abs());
    } else if exponent < 0 {
        text.push_str("0.");
        text.extend(std::iter::repeat_n(
            '0',
            exponent.unsigned_abs() as usize - 1,
        ));
        text.push_str(&digits);
    } else {
        let whole = exponent.unsigned_abs() as usize + 1;
        if digits.len() <= whole {
            text.push_str(&digits);
            text.extend(std::iter::repeat_n('0', whole - digits.len()));
            text.push_str(".0");
        } else {
            let (integer, fraction) = digits.split_at(whole);
            text.push_str(integer);
            text.push('.');
            text.push_str(fraction);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{bool, float};

    /// Python's `repr` of the same value, for `f64`.
    fn repr(value: f64) -> String {
        float(&format!("{value:e}"))
    }

    #[test]
    fn floats_print_as_python_writes_them() {
        assert_eq!(repr(0.0), "0.0");
        assert_eq!(repr(-0.0), "-0.0");
        assert_eq!(repr(1.0), "1.0");
        assert_eq!(repr(-2.5), "-2.5");
        assert_eq!(repr(0.1), "0.1");
        assert_eq!(repr(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(repr(123.456), "123.456");
        assert_eq!(repr(1e15), "1000000000000000.0");
        assert_eq!(repr(1e16), "1e+16");
        assert_eq!(repr(1.5e300), "1.5e+300");
        assert_eq!(repr(0.0001), "0.0001");
        assert_eq!(repr(0.00012), "0.00012");
        assert_eq!(repr(1.5e-5), "1.5e-05");
        assert_eq!(repr(5e-324), "5e-324");
        assert_eq!(repr(f64::INFINITY), "inf");
        assert_eq!(repr(f64::NEG_INFINITY), "-inf");
        assert_eq!(repr(f64::NAN), "nan");
    }

    #[test]
    fn f32_values_print_their_own_shortest_digits() {
        assert_eq!(float(&format!("{:e}", 0.1_f32)), "0.1");
        assert_eq!(float(&format!("{:e}", 16_777_216_f32)), "16777216.0");
    }

    #[test]
    fn bools_print_capitalized() {
        assert_eq!(bool(true), "True");
        assert_eq!(bool(false), "False");
    }
}
