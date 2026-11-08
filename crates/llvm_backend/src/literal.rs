//! How names and constants are written in LLVM's text form: a global's name, quoted when it
//! needs to be, an integer wrapped to its width, and a float in exact hexadecimal.

use std::fmt::Write as _;

/// `@name` for a global, quoted when the name has characters LLVM does not allow bare.
pub(crate) fn global(name: &str) -> String {
    let bare = name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'));
    if bare {
        format!("@{name}")
    } else {
        let mut quoted = String::from("@\"");
        for byte in name.bytes() {
            if byte == b'"' || byte == b'\\' || !(0x20..0x7f).contains(&byte) {
                let _ = write!(quoted, "\\{byte:02X}");
            } else {
                quoted.push(char::from(byte));
            }
        }
        quoted.push('"');
        quoted
    }
}

/// `value` as an LLVM constant of an integer type `bits` wide: wrapped to the width and written
/// signed, the form LLVM reads for every integer type.
pub(crate) fn wrap(value: i128, bits: u32) -> i128 {
    if bits >= 128 {
        return value;
    }
    let shift = 128 - bits;
    (value << shift) >> shift
}

/// A float constant in LLVM's exact hexadecimal form. An `f32` constant is rounded to `f32`
/// first and written as the `f64` holding the same value, as LLVM requires.
pub(crate) fn float(value: f64, single: bool) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rounding an f32 constant to f32 is the point"
    )]
    let value = if single {
        f64::from(value as f32)
    } else {
        value
    };
    format!("0x{:016X}", value.to_bits())
}
