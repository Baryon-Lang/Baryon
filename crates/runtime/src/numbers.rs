//! Reading a number from text for `int(text)` and `float(text)` (decision 0037), as Python's
//! `int` and `float` read it, with Python's message for text that is not a number.
//!
//! These functions are safe Rust over `&str`; `abi::numbers` turns compiled code's pointers into
//! `&str`s, calls these, and writes a failure as a raised `ValueError`. Each reads the text once,
//! and allocates only to take out underscores or to write a message.

use crate::print::quoted;
use crate::strings::is_python_whitespace;

/// The digits of `text` with Python's underscores taken out, or `None` when an underscore is not
/// between two digits, as Python's `int` and `float` require (PEP 515): `1_000` is a thousand,
/// `_1`, `1_` and `1__0` are not numbers.
fn without_underscores(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for (at, &byte) in bytes.iter().enumerate() {
        if byte == b'_' {
            let before = at.checked_sub(1).and_then(|before| bytes.get(before));
            let after = bytes.get(at + 1);
            if !before.is_some_and(u8::is_ascii_digit) || !after.is_some_and(u8::is_ascii_digit) {
                return None;
            }
        }
    }
    Some(text.replace('_', ""))
}

/// The integer `text` writes, as Python's `int(text)` reads it: whitespace around it, an
/// optional `+` or `-`, then decimal digits with single underscores between them.
///
/// # Errors
///
/// The `ValueError` message for any other text, Python's
/// `invalid literal for int() with base 10: '12a'`, and for a number outside `i64`, which
/// Python's unbounded `int` would hold, `int() result out of range: '…'` (decision 0037). Only
/// the ASCII digits `0` to `9` count, where Python also reads other scripts' digits.
pub(crate) fn parse_integer(text: &str) -> Result<i64, String> {
    let invalid = || format!("invalid literal for int() with base 10: {}", quoted(text));
    let trimmed = text.trim_matches(is_python_whitespace);
    let (negative, digits) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    if digits.is_empty()
        || !digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_')
    {
        return Err(invalid());
    }
    let digits = without_underscores(digits).ok_or_else(invalid)?;
    let magnitude: Option<u64> = digits.parse().ok();
    let value = magnitude.and_then(|magnitude| {
        if negative {
            0_i64.checked_sub_unsigned(magnitude)
        } else {
            i64::try_from(magnitude).ok()
        }
    });
    value.ok_or_else(|| format!("int() result out of range: {}", quoted(text)))
}

/// The float `text` writes, as Python's `float(text)` reads it: whitespace around it, an
/// optional sign, then a decimal number with an optional exponent, such as `1.5`, `.5`, `1.`,
/// `6.022e23` or `1_000.5`, or `inf`, `infinity` or `nan` in any case. The result is the nearest
/// float, and a number too large for `f64` is infinity, as in Python.
///
/// # Errors
///
/// Python's `ValueError` message for any other text,
/// `could not convert string to float: '12a'`.
pub(crate) fn parse_float(text: &str) -> Result<f64, String> {
    let invalid = || format!("could not convert string to float: {}", quoted(text));
    let trimmed = text.trim_matches(is_python_whitespace);
    let allowed =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-');
    if trimmed.is_empty() || !trimmed.bytes().all(allowed) {
        return Err(invalid());
    }
    let unsigned = trimmed.trim_start_matches(['+', '-']);
    if trimmed.len() - unsigned.len() > 1 {
        return Err(invalid());
    }
    let word = unsigned.to_ascii_lowercase();
    if matches!(word.as_str(), "inf" | "infinity" | "nan") {
        return trimmed.parse().map_err(|_| invalid());
    }
    if !unsigned.bytes().all(|byte| {
        byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'e' | b'E' | b'+' | b'-')
    }) {
        return Err(invalid());
    }
    without_underscores(trimmed)
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::{parse_float, parse_integer};

    /// `int(text)` accepts what Python's does, and rejects the rest with Python's message.
    #[test]
    fn integers_parse_as_python_reads_them() {
        for (text, value) in [
            ("42", 42),
            ("  -17\n", -17),
            ("+3", 3),
            ("1_000_000", 1_000_000),
            ("007", 7),
            ("-9223372036854775808", i64::MIN),
            ("9223372036854775807", i64::MAX),
        ] {
            assert_eq!(parse_integer(text), Ok(value), "{text}");
        }
        for text in [
            "", " ", "1.0", "1e3", "_1", "1_", "1__0", "--1", "+-1", "0x10", "1 2", "١٢",
        ] {
            assert!(parse_integer(text).is_err(), "{text}");
        }
        assert_eq!(
            parse_integer("12a"),
            Err("invalid literal for int() with base 10: '12a'".to_owned())
        );
        assert_eq!(
            parse_integer("x\n"),
            Err("invalid literal for int() with base 10: 'x\\n'".to_owned())
        );
        assert_eq!(
            parse_integer("9223372036854775808"),
            Err("int() result out of range: '9223372036854775808'".to_owned())
        );
        assert_eq!(
            parse_integer("-9_223_372_036_854_775_809"),
            Err("int() result out of range: '-9_223_372_036_854_775_809'".to_owned())
        );
    }

    /// `float(text)` accepts what Python's does, rounds to the nearest float, and rejects the
    /// rest with Python's message.
    #[test]
    fn floats_parse_as_python_reads_them() {
        for (text, value) in [
            ("1.5", 1.5),
            (" -0.25 ", -0.25),
            (".5", 0.5),
            ("1.", 1.0),
            ("6.022e23", 6.022e23),
            ("1E-3", 1e-3),
            ("1_000.5", 1000.5),
            ("1e1_0", 1e10),
            ("+7", 7.0),
            ("0.1", 0.1),
        ] {
            assert_eq!(parse_float(text), Ok(value), "{text}");
        }
        assert_eq!(parse_float("-Infinity"), Ok(f64::NEG_INFINITY));
        assert_eq!(parse_float("inf"), Ok(f64::INFINITY));
        assert_eq!(parse_float("1e400"), Ok(f64::INFINITY));
        assert!(parse_float("NaN").is_ok_and(f64::is_nan));
        for text in [
            "", ".", "e5", "1e", "1.5.2", "--1", "1_.5", "0x1p3", "nan1", "in f", "1,5",
        ] {
            assert!(parse_float(text).is_err(), "{text}");
        }
        assert_eq!(
            parse_float("1,5"),
            Err("could not convert string to float: '1,5'".to_owned())
        );
    }
}
