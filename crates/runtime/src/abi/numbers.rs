//! The functions compiled code calls for `int(text)` and `float(text)` (decision 0037).
//!
//! Each follows the convention of a compiled function that raises (decision 0005), as the file
//! functions do: its last parameter is the address of the caller's error value, a
//! `Raises[None, ValueError]`; it writes the error there and returns `true` when the text is not
//! a number, and writes the number through `out` and returns `false` when it is. The error value
//! is a one-byte tag, always 0 for the one error type, then the `ValueError` at offset 8
//! ([`RaisedValueError`]).

use super::strings::text;
use super::text::{OwnedText, owned};
use crate::numbers::{parse_float, parse_integer};

/// A `ValueError` as compiled code lays it out: `class ValueError(Exception)` in
/// `crates/driver/library/builtins.byn`, whose one field is `message: str`.
#[repr(C)]
#[derive(Debug)]
pub struct ValueError {
    /// `message`, an owned `str`, such as `invalid literal for int() with base 10: '12a'`.
    pub message: OwnedText,
}

/// The error value `int(text)` and `float(text)` write: `Raises[None, ValueError]`, a tag that
/// says which error type it holds, always 0, then the error.
#[repr(C)]
#[derive(Debug)]
pub struct RaisedValueError {
    /// Which of the error types it is: 0, `ValueError`.
    pub tag: u8,
    /// The error.
    pub error: ValueError,
}

/// Writes `parsed`'s number through `out`, or its message to `error` as a raised `ValueError`,
/// and returns whether it failed.
///
/// # Safety
///
/// `out` points to a writable `T`, and `error` to a writable [`RaisedValueError`].
#[allow(unsafe_code, reason = "writes results compiled code owns")]
unsafe fn finish<T>(parsed: Result<T, String>, out: *mut T, error: *mut RaisedValueError) -> bool {
    match parsed {
        Ok(value) => {
            // SAFETY: the caller guarantees `out` is writable.
            unsafe { out.write(value) };
            false
        }
        Err(message) => {
            let raised = RaisedValueError {
                tag: 0,
                error: ValueError {
                    message: owned(&message),
                },
            };
            // SAFETY: the caller guarantees `error` is writable.
            unsafe { error.write(raised) };
            true
        }
    }
}

/// `int(text)`: reads the integer the `length` bytes at `text` write, as Python's `int(text)`
/// does, into `out`, or raises a `ValueError` into `error`, and returns whether it failed.
/// Whitespace around the number, a sign, and ASCII digits with single underscores between them
/// are accepted, inside `i64`'s range.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0; `out` points to a
/// writable `i64` and `error` to a writable [`RaisedValueError`].
///
/// # Example
///
/// ```
/// use runtime::{RaisedValueError, baryon_rt_str_parse_int};
/// use std::mem::MaybeUninit;
///
/// let (text, mut value) = (" -1_024\n", 0);
/// let mut error = MaybeUninit::<RaisedValueError>::uninit();
/// // SAFETY: the pointer and length are the constant's, and both outputs are writable.
/// let failed = unsafe {
///     baryon_rt_str_parse_int(text.as_ptr(), text.len(), &mut value, error.as_mut_ptr())
/// };
/// assert!(!failed);
/// assert_eq!(value, -1024);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_parse_int(
    text: *const u8,
    length: usize,
    out: *mut i64,
    error: *mut RaisedValueError,
) -> bool {
    // SAFETY: the caller guarantees the text is readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: the caller guarantees both outputs are writable.
    unsafe { finish(parse_integer(text), out, error) }
}

/// `float(text)`: reads the float the `length` bytes at `text` write, as Python's `float(text)`
/// does, into `out`, or raises a `ValueError` into `error`, and returns whether it failed. A
/// decimal number with an optional exponent and underscores between digits, or `inf`,
/// `infinity` or `nan` in any case, with a sign and whitespace around it; the result is the
/// nearest `f64`.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0; `out` points to a
/// writable `f64` and `error` to a writable [`RaisedValueError`].
///
/// # Example
///
/// ```
/// use runtime::{RaisedValueError, baryon_rt_str_parse_float};
/// use std::mem::MaybeUninit;
///
/// let (text, mut value) = ("6.022e23", 0.0);
/// let mut error = MaybeUninit::<RaisedValueError>::uninit();
/// // SAFETY: the pointer and length are the constant's, and both outputs are writable.
/// let failed = unsafe {
///     baryon_rt_str_parse_float(text.as_ptr(), text.len(), &mut value, error.as_mut_ptr())
/// };
/// assert!(!failed);
/// assert_eq!(value, 6.022e23);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_parse_float(
    text: *const u8,
    length: usize,
    out: *mut f64,
    error: *mut RaisedValueError,
) -> bool {
    // SAFETY: the caller guarantees the text is readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: the caller guarantees both outputs are writable.
    unsafe { finish(parse_float(text), out, error) }
}

#[cfg(test)]
mod tests {
    use super::{RaisedValueError, baryon_rt_str_parse_float, baryon_rt_str_parse_int};
    use crate::baryon_rt_deallocate;
    use std::mem::{MaybeUninit, offset_of};

    /// The text of the raised error's message, which it frees.
    #[allow(unsafe_code, reason = "reads and frees text the runtime made")]
    fn message(raised: &RaisedValueError) -> String {
        let text = &raised.error.message;
        let length = usize::try_from(text.length).unwrap();
        // SAFETY: the runtime made `text` with `length` initialized bytes.
        let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) };
        let copy = String::from_utf8(bytes.to_vec()).unwrap();
        // SAFETY: freed once, with its length.
        unsafe { baryon_rt_deallocate(text.bytes, length, 1) };
        copy
    }

    /// The error sits at offset 8 after its tag, and its message is a `str`, two words, as
    /// compiled code lays out `Raises[None, ValueError]`.
    #[test]
    fn the_error_is_laid_out_as_compiled_code_expects() {
        assert_eq!(offset_of!(RaisedValueError, error), 8);
        assert_eq!(size_of::<RaisedValueError>(), 24);
    }

    /// Bad text raises a `ValueError` with Python's message and leaves `out` alone.
    #[test]
    #[allow(
        unsafe_code,
        reason = "calls the exported functions as compiled code does"
    )]
    fn bad_text_raises_with_pythons_message() {
        let text = "12a";
        let (mut integer, mut float) = (7_i64, 7.0_f64);
        let mut error = MaybeUninit::<RaisedValueError>::uninit();
        // SAFETY: the pointer and length are the constant's, and both outputs are writable.
        let failed = unsafe {
            baryon_rt_str_parse_int(
                text.as_ptr(),
                text.len(),
                &raw mut integer,
                error.as_mut_ptr(),
            )
        };
        assert!(failed);
        assert_eq!(integer, 7);
        // SAFETY: the call failed, so it wrote the error.
        let raised = unsafe { error.assume_init_read() };
        assert_eq!(raised.tag, 0);
        assert_eq!(
            message(&raised),
            "invalid literal for int() with base 10: '12a'"
        );
        // SAFETY: as above.
        let failed = unsafe {
            baryon_rt_str_parse_float(
                text.as_ptr(),
                text.len(),
                &raw mut float,
                error.as_mut_ptr(),
            )
        };
        assert!(failed);
        assert_eq!(float.to_bits(), 7.0_f64.to_bits());
        // SAFETY: the call failed, so it wrote the error.
        let raised = unsafe { error.assume_init_read() };
        assert_eq!(message(&raised), "could not convert string to float: '12a'");
    }
}
