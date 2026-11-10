//! The functions compiled code calls to build a `str`: an f-string, a `copy()` or a `+`
//! (decision 0022).
//!
//! Compiled code starts a builder, appends each piece, then finishes it into an owned `str`.
//! `f"step {i}: {energy:.6f}"` with an `i64` and an `f64` compiles to these calls:
//!
//! ```
//! use runtime::{
//!     FormatSpec, baryon_rt_deallocate, baryon_rt_text_finish,
//!     baryon_rt_text_f64, baryon_rt_text_i64, baryon_rt_text_new, baryon_rt_text_str,
//! };
//!
//! static FIXED: FormatSpec = FormatSpec { presentation: b'f', precision: 6, ..FormatSpec::DEFAULT };
//! let (step, colon) = ("step ", ": ");
//! // SAFETY: each pointer and length is a constant's, the builder is used once, and the text
//! // is freed with its length.
//! unsafe {
//!     let builder = baryon_rt_text_new();
//!     baryon_rt_text_str(builder, step.as_ptr(), step.len(), std::ptr::null());
//!     baryon_rt_text_i64(builder, 3, std::ptr::null());
//!     baryon_rt_text_str(builder, colon.as_ptr(), colon.len(), std::ptr::null());
//!     baryon_rt_text_f64(builder, 0.25, &FIXED);
//!     let text = baryon_rt_text_finish(builder);
//!     let written = std::slice::from_raw_parts(text.bytes, text.length as usize);
//!     assert_eq!(written, b"step 3: 0.250000");
//!     baryon_rt_deallocate(text.bytes, text.length as usize, 1);
//! }
//! ```

use crate::format::{self, FormatSpec};
use crate::memory;
use crate::panic::panic;

/// A `str` value as compiled code holds it: the bytes and their number. A builder's result owns
/// its bytes, so the top bit of `length` is clear, and a drop frees them with
/// [`baryon_rt_deallocate`](crate::baryon_rt_deallocate), `length` bytes aligned to 1.
#[repr(C)]
#[derive(Debug)]
pub struct OwnedText {
    /// The UTF-8 bytes.
    pub bytes: *mut u8,
    /// How many there are.
    pub length: u64,
}

/// A `str` being built: the text so far.
pub struct TextBuilder {
    /// The text so far.
    text: String,
}

/// Appends what `write` makes of the specification `spec` points to, or of the empty one for
/// a null pointer, to the builder `builder` points to.
///
/// # Safety
///
/// `builder` came from [`baryon_rt_text_new`] and has not been finished, and `spec` is null or
/// points to a valid [`FormatSpec`].
#[allow(
    unsafe_code,
    reason = "reads a builder and a specification compiled code points to"
)]
unsafe fn push(
    builder: *mut TextBuilder,
    spec: *const FormatSpec,
    write: impl FnOnce(&FormatSpec) -> String,
) {
    // SAFETY: the caller guarantees the pointer is null or valid.
    let spec = unsafe { spec.as_ref() }.unwrap_or(&FormatSpec::DEFAULT);
    let text = write(spec);
    // SAFETY: the caller guarantees the builder is live, and nothing else refers to it.
    unsafe { &mut *builder }.text.push_str(&text);
}

/// Starts a `str`: an empty builder, which [`baryon_rt_text_finish`] ends.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_text_new() -> *mut TextBuilder {
    Box::into_raw(Box::new(TextBuilder {
        text: String::new(),
    }))
}

/// Appends the `length` bytes of UTF-8 text at `text`, formatted with `spec`: a `str` value, or
/// a literal piece of an f-string with a null `spec`.
///
/// # Safety
///
/// `builder` came from [`baryon_rt_text_new`] and has not been finished, `text` points to
/// `length` readable bytes, and `spec` is null or points to a valid [`FormatSpec`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_str(
    builder: *mut TextBuilder,
    text: *const u8,
    length: usize,
    spec: *const FormatSpec,
) {
    // SAFETY: the caller guarantees `text` points to `length` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(text, length) };
    let text = String::from_utf8_lossy(bytes);
    // SAFETY: the caller's promises, passed on.
    unsafe { push(builder, spec, |spec| format::text(&text, spec)) };
}

/// Appends a signed integer formatted with `spec`; a narrower integer is widened first.
///
/// # Safety
///
/// As for [`baryon_rt_text_str`], without the text.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_i64(
    builder: *mut TextBuilder,
    value: i64,
    spec: *const FormatSpec,
) {
    // SAFETY: the caller's promises, passed on.
    unsafe { push(builder, spec, |spec| format::signed(value, spec)) };
}

/// Appends an unsigned integer formatted with `spec`; a narrower integer is widened first.
///
/// # Safety
///
/// As for [`baryon_rt_text_str`], without the text.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_u64(
    builder: *mut TextBuilder,
    value: u64,
    spec: *const FormatSpec,
) {
    // SAFETY: the caller's promises, passed on.
    unsafe { push(builder, spec, |spec| format::unsigned(value, spec)) };
}

/// Appends an `f64` formatted with `spec`.
///
/// # Safety
///
/// As for [`baryon_rt_text_str`], without the text.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_f64(
    builder: *mut TextBuilder,
    value: f64,
    spec: *const FormatSpec,
) {
    // SAFETY: the caller's promises, passed on.
    unsafe { push(builder, spec, |spec| format::float(value, false, spec)) };
}

/// Appends an `f32` formatted with `spec`; the empty specification writes its own shortest
/// digits, as `print` does.
///
/// # Safety
///
/// As for [`baryon_rt_text_str`], without the text.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_f32(
    builder: *mut TextBuilder,
    value: f32,
    spec: *const FormatSpec,
) {
    // SAFETY: the caller's promises, passed on.
    unsafe {
        push(builder, spec, |spec| {
            format::float(f64::from(value), true, spec)
        });
    }
}

/// Appends a `bool` formatted with `spec`: `True` or `False` with the empty specification, 1 or
/// 0 with any other, as in Python.
///
/// # Safety
///
/// As for [`baryon_rt_text_str`], without the text.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_bool(
    builder: *mut TextBuilder,
    value: bool,
    spec: *const FormatSpec,
) {
    // SAFETY: the caller's promises, passed on.
    unsafe { push(builder, spec, |spec| format::boolean(value, spec)) };
}

/// Ends the builder and returns the `str` it built, in memory of its own that the value owns.
/// The two words come back in registers, as C returns a two-word struct, so compiled code
/// declares the result `{ ptr, i64 }`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `builder` came from [`baryon_rt_text_new`] and has not been finished.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_text_finish(builder: *mut TextBuilder) -> OwnedText {
    // SAFETY: the caller guarantees `builder` came from `Box::into_raw` and is finished once.
    let built = unsafe { Box::from_raw(builder) }.text;
    owned(&built)
}

/// A new owned `str` holding a copy of `text`, allocated as compiled code frees it: `length`
/// bytes aligned to 1. Running out of memory panics.
#[allow(unsafe_code, reason = "copies into memory the runtime allocates")]
pub(super) fn owned(text: &str) -> OwnedText {
    let bytes = match memory::layout(text.len(), 1).and_then(memory::allocate) {
        Ok(pointer) => pointer.as_ptr(),
        Err(failure) => panic(failure, None),
    };
    // SAFETY: `bytes` has room for `text.len()` bytes, and the two do not overlap.
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), bytes, text.len()) };
    let length = u64::try_from(text.len()).unwrap_or(u64::MAX);
    OwnedText { bytes, length }
}
