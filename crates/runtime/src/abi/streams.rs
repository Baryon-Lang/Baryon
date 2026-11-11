//! The functions compiled code calls for the standard streams (decision 0036, part 6): the
//! `File` that `sys.stdin`, `sys.stdout` or `sys.stderr` gives, `print(..., file=f)`, and
//! `input`.
//!
//! A standard stream is a `File` like one `open` returns, a pointer to an [`OpenFile`], so the
//! file functions read, write, flush, close and drop it. `print(..., file=f)` prints each value
//! to stream 3, which holds the text, then calls [`baryon_rt_file_print`] to write it to the
//! file. Functions that can fail raise as the file functions do.

use super::files::{RaisedError, finish, open_file};
use super::strings::text;
use super::text::{OwnedText, owned};
use crate::files::OpenFile;
use crate::panic::{Failure, panic};
use crate::print;
use crate::streams::{self, Standard};

/// `sys.stdin`, `sys.stdout` or `sys.stderr`: a new `File` on the standard stream numbered
/// `stream`, 0, 1 or 2. Compiled code owns it and ends it as any `File`, with
/// [`baryon_rt_file_close`](crate::baryon_rt_file_close) or
/// [`baryon_rt_file_drop`](crate::baryon_rt_file_drop), neither of which closes the stream.
///
/// # Panics
///
/// Ends the program with a panic for a stream number outside 0 to 2, which lowering never
/// passes.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_file_drop, baryon_rt_file_standard};
///
/// let error_stream = baryon_rt_file_standard(2);
/// // SAFETY: the file came from `baryon_rt_file_standard` and is used no more.
/// unsafe { baryon_rt_file_drop(error_stream) };
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_file_standard(stream: u8) -> *mut OpenFile {
    let Some(standard) = Standard::from_number(stream) else {
        panic(Failure::Message("standard stream out of range"), None);
    };
    Box::into_raw(Box::new(OpenFile::standard(standard)))
}

/// The end of `print(..., file=f)`: writes to the file the text this thread printed to stream 3
/// since the last call. Returns `true`, with the error at `error`, when the write fails.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`](crate::baryon_rt_file_open) or
/// [`baryon_rt_file_standard`], is not closed or dropped, and nothing else uses it during the
/// call; `error` points to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_print(
    file: *mut OpenFile,
    error: *mut RaisedError,
) -> bool {
    let printed = print::captured();
    // SAFETY: the caller's promise about `file`, passed on.
    let result = unsafe { open_file(file) }.write(&printed);
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |_| {}, error) }
}

/// The error value `input` writes at the end of the input: `Raises[None, EOFError]`, a tag
/// that says which error type it holds, always 0. `EOFError` has no fields, so nothing
/// follows the tag.
#[repr(C)]
#[derive(Debug)]
pub struct RaisedEndOfInput {
    /// Which of the error types it is: 0, `EOFError`.
    pub tag: u8,
}

/// `input(prompt)`: writes the `length` bytes at `prompt` to standard output, writes standard
/// output out, and writes the next line of standard input, without its line end, to `*out` as
/// a new owned `str`. Bytes that are not UTF-8 read as U+FFFD. Returns `true`, with an
/// `EOFError` at `error`, at the end of the input.
///
/// # Safety
///
/// `prompt` points to `length` readable bytes of UTF-8, `out` to a writable [`OwnedText`] and
/// `error` to a writable [`RaisedEndOfInput`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_input(
    prompt: *const u8,
    length: usize,
    out: *mut OwnedText,
    error: *mut RaisedEndOfInput,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `prompt`.
    let prompt = unsafe { text(prompt, length) };
    let Some(line) = streams::input(prompt) else {
        // SAFETY: the caller guarantees `error` is writable.
        unsafe { error.write(RaisedEndOfInput { tag: 0 }) };
        return true;
    };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { out.write(owned(&line)) };
    false
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests {
    use super::{baryon_rt_file_print, baryon_rt_file_standard};
    use crate::abi::files::RaisedError;
    use crate::{
        baryon_rt_file_close, baryon_rt_file_drop, baryon_rt_file_open, baryon_rt_file_read,
        baryon_rt_file_write, baryon_rt_print_i64, baryon_rt_print_str,
    };
    use std::mem::MaybeUninit;

    /// The `strerror` of a raised error, which it frees.
    fn message(raised: &RaisedError) -> String {
        let text = |text: &crate::abi::text::OwnedText| {
            let length = usize::try_from(text.length).unwrap();
            // SAFETY: the runtime made the text with `length` initialized bytes.
            let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) };
            let copy = String::from_utf8(bytes.to_vec()).unwrap();
            // SAFETY: freed once, with its length.
            unsafe { crate::baryon_rt_deallocate(text.bytes, length, 1) };
            copy
        };
        let _ = text(&raised.error.filename);
        text(&raised.error.strerror)
    }

    #[test]
    fn a_standard_stream_is_a_file_that_closing_and_dropping_leave_open() {
        let mut error = MaybeUninit::<RaisedError>::uninit();
        let mut count = 0_i64;
        for _ in 0..2 {
            let output = baryon_rt_file_standard(1);
            // SAFETY: `output` is a live file, the text a constant, the rest locals.
            let failed = unsafe {
                baryon_rt_file_write(output, "".as_ptr(), 0, &raw mut count, error.as_mut_ptr())
            };
            assert!(!failed);
            // SAFETY: `output` is live and used no more after the close.
            assert!(!unsafe { baryon_rt_file_close(output, error.as_mut_ptr()) });
        }
        let errors = baryon_rt_file_standard(2);
        // SAFETY: `errors` is live and used no more after the drop.
        unsafe { baryon_rt_file_drop(errors) };
    }

    #[test]
    fn standard_output_is_not_readable_and_standard_input_not_writable() {
        let mut error = MaybeUninit::<RaisedError>::uninit();
        let mut text = MaybeUninit::uninit();
        let output = baryon_rt_file_standard(1);
        // SAFETY: `output` is a live file; `text` and `error` are locals.
        let failed = unsafe { baryon_rt_file_read(output, text.as_mut_ptr(), error.as_mut_ptr()) };
        assert!(failed);
        // SAFETY: a failure wrote the error.
        assert_eq!(message(unsafe { error.assume_init_ref() }), "not readable");
        // SAFETY: `output` is live and used no more.
        unsafe { baryon_rt_file_drop(output) };

        let input = baryon_rt_file_standard(0);
        let mut count = 0_i64;
        // SAFETY: `input` is a live file, the text a constant, the rest locals.
        let failed = unsafe {
            baryon_rt_file_write(input, "x".as_ptr(), 1, &raw mut count, error.as_mut_ptr())
        };
        assert!(failed);
        // SAFETY: a failure wrote the error.
        assert_eq!(message(unsafe { error.assume_init_ref() }), "not writable");
        // SAFETY: `input` is live and used no more.
        unsafe { baryon_rt_file_drop(input) };
    }

    #[test]
    fn printing_to_a_file_writes_what_stream_three_held() {
        let path = std::env::temp_dir().join(format!("baryon-print-{}.txt", std::process::id()));
        let name = path.to_str().unwrap();
        let mut file = std::ptr::null_mut();
        let mut error = MaybeUninit::<RaisedError>::uninit();
        // SAFETY: the pointer and length are the path's; `file` and `error` are locals.
        let failed = unsafe {
            baryon_rt_file_open(
                name.as_ptr(),
                name.len(),
                1,
                &raw mut file,
                error.as_mut_ptr(),
            )
        };
        assert!(!failed);
        // SAFETY: the pointer and length are a constant's.
        unsafe { baryon_rt_print_str(3, "step ".as_ptr(), 5) };
        baryon_rt_print_i64(3, 7);
        // SAFETY: the pointer and length are a constant's.
        unsafe { baryon_rt_print_str(3, "\n".as_ptr(), 1) };
        // The second call finds nothing held and writes nothing.
        for _ in 0..2 {
            // SAFETY: `file` is open and `error` a local.
            assert!(!unsafe { baryon_rt_file_print(file, error.as_mut_ptr()) });
        }
        // SAFETY: `file` is open and used no more after the close.
        assert!(!unsafe { baryon_rt_file_close(file, error.as_mut_ptr()) });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "step 7\n");
        std::fs::remove_file(path).unwrap();
    }
}
