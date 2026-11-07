//! The functions compiled code calls for `print`, one per kind of value (the `print` module).
//!
//! Each takes the stream first: 2 writes to standard error, anything else, 1 by convention, to
//! standard output. A narrower integer is widened before the call: signed types to `i64`,
//! unsigned ones to `u64`. `print(a, b)` with an `i32` and an `f64` compiles to these calls:
//!
//! ```
//! use runtime::{npl_rt_print_f64, npl_rt_print_i64, npl_rt_print_str};
//!
//! // SAFETY: each pointer and length is a string constant's.
//! unsafe {
//!     npl_rt_print_i64(1, 7);
//!     npl_rt_print_str(1, " ".as_ptr(), 1);
//!     npl_rt_print_f64(1, 0.5);
//!     npl_rt_print_str(1, "\n".as_ptr(), 1);
//! }
//! ```
//!
//! That prints `7 0.5`.

use crate::print;

/// Prints a signed integer in decimal, as Python does: `-12`.
///
/// # Example
///
/// ```
/// runtime::npl_rt_print_i64(1, -12);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn npl_rt_print_i64(stream: i32, value: i64) {
    print::write(stream, &value.to_string());
}

/// Prints an unsigned integer in decimal, as Python does: `18446744073709551615`.
///
/// # Example
///
/// ```
/// runtime::npl_rt_print_u64(1, u64::MAX);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn npl_rt_print_u64(stream: i32, value: u64) {
    print::write(stream, &value.to_string());
}

/// Prints an `f64` as Python's `repr` writes a float: `0.1`, `2.0`, `1e+16`, `nan`.
///
/// # Example
///
/// ```
/// runtime::npl_rt_print_f64(1, 0.1 + 0.2); // 0.30000000000000004
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn npl_rt_print_f64(stream: i32, value: f64) {
    print::write(stream, &print::float(&format!("{value:e}")));
}

/// Prints an `f32` with the fewest digits that read back as the same `f32`, placed as Python's
/// `repr` places them: `0.1`, not the `0.10000000149011612` of the same value widened.
///
/// # Example
///
/// ```
/// runtime::npl_rt_print_f32(1, 0.1);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn npl_rt_print_f32(stream: i32, value: f32) {
    print::write(stream, &print::float(&format!("{value:e}")));
}

/// Prints a `bool` as Python does: `True` or `False`.
///
/// # Example
///
/// ```
/// runtime::npl_rt_print_bool(1, true);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn npl_rt_print_bool(stream: i32, value: bool) {
    print::write(stream, print::bool(value));
}

/// Prints `length` bytes of UTF-8 text at `text` as they are: a `str` argument, and the space
/// between arguments and the line end that compiled code writes itself. Bytes that are not UTF-8,
/// which only a compiler bug passes, print as replacement characters.
///
/// # Safety
///
/// `text` points to `length` readable bytes.
///
/// # Example
///
/// ```
/// use runtime::npl_rt_print_str;
///
/// let line = "converged\n";
/// // SAFETY: the pointer and length are the constant's.
/// unsafe { npl_rt_print_str(1, line.as_ptr(), line.len()) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn npl_rt_print_str(stream: i32, text: *const u8, length: usize) {
    // SAFETY: the caller guarantees `text` points to `length` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(text, length) };
    print::write(stream, &String::from_utf8_lossy(bytes));
}
