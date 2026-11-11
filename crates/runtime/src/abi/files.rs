//! The functions compiled code calls for text files (decision 0036, parts 3 and 4): `open`, the
//! methods of the `File` it returns, and the drop that closes one.
//!
//! A `File` is one pointer, to an [`OpenFile`] this module allocates and frees. Each function that
//! can fail follows the convention of a compiled function that raises (decision 0005): it takes
//! a last parameter, the address of the caller's error value, a `Raises[None, OSError]`; it writes
//! the error there and returns `true` when it fails, and writes its result through `out` and
//! returns `false` when it succeeds. The error value is a one-byte tag, always 0 for the one error
//! type, then the `OSError` at offset 8 in the field order `<std>/builtins.byn` declares
//! ([`RaisedError`]).

use super::string_lists::{OwnedList, write_list};
use super::strings::text;
use super::text::{OwnedText, owned};
use crate::files::{ErrorKind, Mode, OpenFile, OsFailure};
use crate::panic::{Failure, panic};

/// An `OSError` as compiled code lays it out: `class OSError(Exception)` in
/// `crates/driver/library/builtins.byn`, its fields in declaration order with C alignment.
#[repr(C)]
#[derive(Debug)]
pub struct OsError {
    /// `kind`, the position of an `os.ErrorKind` member.
    pub kind: u8,
    /// `errno`.
    pub errno: i32,
    /// `strerror`, an owned `str`.
    pub strerror: OwnedText,
    /// `filename`, an owned `str`.
    pub filename: OwnedText,
}

/// The error value a call that can fail writes: `Raises[None, OSError]`, a tag that says which
/// error type it holds, always 0, then the error.
#[repr(C)]
#[derive(Debug)]
pub struct RaisedError {
    /// Which of the error types it is: 0, `OSError`.
    pub tag: u8,
    /// The error.
    pub error: OsError,
}

/// Writes `failure` to `error` as a raised `OSError` and returns `true`, the failure flag.
///
/// # Safety
///
/// `error` points to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "writes an error compiled code owns")]
pub(super) unsafe fn raise(failure: &OsFailure, error: *mut RaisedError) -> bool {
    let kind: ErrorKind = failure.kind;
    let value = RaisedError {
        tag: 0,
        error: OsError {
            kind: kind as u8,
            errno: failure.errno,
            strerror: owned(&failure.strerror),
            filename: owned(&failure.filename),
        },
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { error.write(value) };
    true
}

/// Writes `result`'s value through `out` with `write`, or its failure to `error`, and returns
/// whether it failed.
///
/// # Safety
///
/// `error` points to a writable [`RaisedError`], and `write` may be called once.
#[allow(unsafe_code, reason = "writes results compiled code owns")]
pub(super) unsafe fn finish<T>(
    result: Result<T, OsFailure>,
    write: impl FnOnce(T),
    error: *mut RaisedError,
) -> bool {
    match result {
        Ok(value) => {
            write(value);
            false
        }
        // SAFETY: the caller's promise about `error`, passed on.
        Err(failure) => unsafe { raise(&failure, error) },
    }
}

/// `open(path, mode)`: opens the file whose path is the `length` bytes at `path`, in the mode
/// numbered `mode` (0 `"r"`, 1 `"w"`, 2 `"a"`, 3 `"x"`), and writes the new `File` to `*out`.
/// Returns `true`, with the error at `error`, when the operating system refuses.
///
/// # Panics
///
/// Ends the program with a panic for a mode number outside 0 to 3, which the checker never
/// passes.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, `out` to a writable pointer and `error` to a
/// writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_open(
    path: *const u8,
    length: usize,
    mode: u8,
    out: *mut *mut OpenFile,
    error: *mut RaisedError,
) -> bool {
    let Some(mode) = Mode::from_number(mode) else {
        panic(Failure::Message("`open` mode out of range"), None);
    };
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let result = OpenFile::open(path, mode);
    // SAFETY: the caller guarantees `out` is writable.
    let write = |file| unsafe { out.write(Box::into_raw(file)) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// The open file `file` points to.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`], has not been closed or dropped, and nothing else
/// uses it during the call.
#[allow(unsafe_code, reason = "reads a file compiled code owns")]
pub(super) unsafe fn open_file<'a>(file: *mut OpenFile) -> &'a mut OpenFile {
    // SAFETY: the caller guarantees `file` is a live `OpenFile` used by nothing else.
    unsafe { &mut *file }
}

/// `f.read()`: writes the rest of the file to `*out` as a new owned `str`.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`], is not closed or dropped, and nothing else uses it
/// during the call; `out` points to a writable [`OwnedText`] and `error` to a writable
/// [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_read(
    file: *mut OpenFile,
    out: *mut OwnedText,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller's promise about `file`, passed on.
    let result = unsafe { open_file(file) }.read();
    // SAFETY: the caller guarantees `out` is writable.
    let write = |text: String| unsafe { out.write(owned(&text)) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `f.readline()`: writes the next line, with its `\n`, or `""` at the end, to `*out`.
///
/// # Safety
///
/// As for [`baryon_rt_file_read`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_read_line(
    file: *mut OpenFile,
    out: *mut OwnedText,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller's promise about `file`, passed on.
    let result = unsafe { open_file(file) }.read_line();
    // SAFETY: the caller guarantees `out` is writable.
    let write = |line: String| unsafe { out.write(owned(&line)) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `f.readlines()`: writes the remaining lines to `*out` as a new `list[str]`.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`], is not closed or dropped, and nothing else uses it
/// during the call; `out` points to a writable [`OwnedList`] and `error` to a writable
/// [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_read_lines(
    file: *mut OpenFile,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller's promise about `file`, passed on.
    let result = unsafe { open_file(file) }.read_lines();
    let write = |lines: Vec<String>| {
        let pieces: Vec<&str> = lines.iter().map(String::as_str).collect();
        // SAFETY: the caller guarantees `out` is writable.
        unsafe { write_list(&pieces, out) };
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `f.write(text)`: writes the `length` bytes at `text` and the number of characters to `*out`.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`], is not closed or dropped, and nothing else uses it
/// during the call; `text` points to `length` readable bytes of UTF-8, `out` to a writable
/// `i64` and `error` to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_write(
    file: *mut OpenFile,
    text_bytes: *const u8,
    length: usize,
    out: *mut i64,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8.
    let written = unsafe { text(text_bytes, length) };
    // SAFETY: the caller's promise about `file`, passed on.
    let result = unsafe { open_file(file) }.write(written);
    // SAFETY: the caller guarantees `out` is writable.
    let write = |count| unsafe { out.write(count) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `f.flush()`: writes out the file's buffer.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`], is not closed or dropped, and nothing else uses it
/// during the call; `error` points to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_flush(
    file: *mut OpenFile,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller's promise about `file`, passed on.
    let result = unsafe { open_file(file) }.flush();
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
}

/// `f.close()` and the end of a `with` block: writes out the buffer, closes the file and frees
/// it. The file is closed and freed whether or not the write fails.
///
/// # Safety
///
/// `file` came from [`baryon_rt_file_open`], is not closed or dropped, and nothing else uses it
/// during the call, and compiled code never uses `file` again; `error` points to a writable
/// [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_close(
    file: *mut OpenFile,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `file` came from `Box::into_raw` and is used no more.
    let file = unsafe { Box::from_raw(file) };
    // SAFETY: the caller's promise about `error`, passed on.
    unsafe { finish(file.close(), |()| {}, error) }
}

/// The drop of a `File` that was neither closed nor ended by `with`: writes out the buffer,
/// closes the file and frees it. A drop cannot raise, so a failed write panics, naming the file
/// (decision 0036, part 3). A handle on a standard stream is only freed: nothing is written and
/// the stream stays open.
///
/// # Panics
///
/// Ends the program with a panic when the last write fails, such as on a full disk.
///
/// # Safety
///
/// As for [`baryon_rt_file_close`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_file_drop(file: *mut OpenFile) {
    // SAFETY: the caller guarantees `file` came from `Box::into_raw` and is used no more.
    let file = unsafe { Box::from_raw(file) };
    let path = file.path().to_owned();
    if let Err(failure) = file.release() {
        let message = format!(
            "could not write out `{path}` when it was dropped: {}",
            failure.strerror
        );
        panic(Failure::Message(&message), None);
    }
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
