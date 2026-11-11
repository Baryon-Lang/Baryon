//! The functions compiled code calls for paths and directories (decision 0036, parts 1 and 7):
//! the parts of a `pathlib.Path`, what it asks the file system, and the directory functions of
//! `os`.
//!
//! A path arrives as the address of its UTF-8 bytes and their count, as every `str` does. A
//! function that makes text returns a new owned `str`, and one that can fail follows the
//! convention of the file functions (`abi::files`): its last parameter is the address of the
//! caller's `Raises[None, OSError]`, and it returns `true` with the error written there when it
//! fails.
//!
//! ```
//! use runtime::{baryon_rt_deallocate, baryon_rt_path_join, baryon_rt_path_part};
//!
//! let (runs, file) = ("runs/", "out.csv");
//! // SAFETY: each pointer and length is a constant's, and each result is freed once.
//! unsafe {
//!     // `Path("runs/") / "out.csv"`, then `.suffix`.
//!     let path = baryon_rt_path_join(runs.as_ptr(), runs.len(), file.as_ptr(), file.len());
//!     let joined = std::slice::from_raw_parts(path.bytes, path.length as usize);
//!     assert_eq!(joined, b"runs/out.csv");
//!     let suffix = baryon_rt_path_part(path.bytes, path.length as usize, 3);
//!     assert_eq!(std::slice::from_raw_parts(suffix.bytes, 4), b".csv");
//!     baryon_rt_deallocate(suffix.bytes, suffix.length as usize, 1);
//!     baryon_rt_deallocate(path.bytes, path.length as usize, 1);
//! }
//! ```

use super::files::{RaisedError, finish};
use super::string_lists::{OwnedList, write_list};
use super::strings::text;
use super::text::{OwnedText, owned};
use crate::panic::{Failure, panic};
use crate::paths;

/// One part of the path whose text is the `length` bytes at `path`, as a new owned `str`. `part`
/// picks it: 0 the path itself in the form `pathlib` keeps, which is `Path(text)` of any text, 1
/// `name`, 2 `stem`, 3 `suffix` and 4 `parent`, each of a path already in that form.
///
/// # Panics
///
/// Ends the program with a panic for a `part` above 4, which compiled code never passes, and
/// when the system has no memory left.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, or `length` is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_part(
    path: *const u8,
    length: usize,
    part: u8,
) -> OwnedText {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    match part {
        0 => owned(&paths::normalize(path)),
        1 => owned(paths::name(path)),
        2 => owned(paths::stem(path)),
        3 => owned(paths::suffix(path)),
        4 => owned(paths::parent(path)),
        _ => panic(Failure::Message("path part out of range"), None),
    }
}

/// `path / other` as a new owned `str`: the path whose text is at `path`, already in the form
/// `pathlib` keeps, joined with the text at `other`, which replaces it when it starts with `/`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8 and `other` to `other_length`, or the
/// length is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_join(
    path: *const u8,
    length: usize,
    other: *const u8,
    other_length: usize,
) -> OwnedText {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: as for `path`.
    let other = unsafe { text(other, other_length) };
    owned(&paths::join(path, other))
}

/// What the path whose text is at `path` names, following symbolic links. `what` picks the
/// question: 0 `exists()`, anything at all, 1 `is_file()`, a regular file, and 2 `is_dir()`, a
/// directory. A failure to find out is `false`, as in Python.
///
/// # Panics
///
/// Ends the program with a panic for a `what` above 2, which compiled code never passes.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, or `length` is 0.
///
/// # Example
///
/// ```
/// let here = ".";
/// // SAFETY: the pointer and length are the constant's.
/// unsafe {
///     assert!(runtime::baryon_rt_path_is(here.as_ptr(), here.len(), 2));
///     assert!(!runtime::baryon_rt_path_is(here.as_ptr(), here.len(), 1));
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_is(path: *const u8, length: usize, what: u8) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    match what {
        0 => paths::exists(path),
        1 => paths::is_file(path),
        2 => paths::is_directory(path),
        _ => panic(Failure::Message("path question out of range"), None),
    }
}

/// `os.listdir(path)`, or `path.iterdir()` when `joined`: writes a new `list[str]` to `*out`,
/// the name of every entry of the directory at `path`, each joined to the path when `joined`,
/// in the order the operating system gives. Returns `true`, with the error at `error`, when the
/// directory cannot be read or holds a name that is not UTF-8.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, `out` to a writable [`OwnedList`] and
/// `error` to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_list(
    path: *const u8,
    length: usize,
    joined: bool,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let write = |names: Vec<String>| {
        let pieces: Vec<&str> = names.iter().map(String::as_str).collect();
        // SAFETY: the caller guarantees `out` is writable.
        unsafe { write_list(&pieces, out) };
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(paths::list_directory(path, joined), write, error) }
}

/// `os.remove(path)`: removes the file at `path`. Returns `true`, with the error at `error`,
/// when there is no such file, it is a directory, or the operating system refuses.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8 and `error` to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_remove(
    path: *const u8,
    length: usize,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(paths::remove(path), |()| {}, error) }
}

/// `os.rename(source, destination)`: moves what is at `source` to `destination`, replacing a
/// file there. Returns `true`, with the error at `error` naming `source`, when it fails.
///
/// # Safety
///
/// `source` points to `length` readable bytes of UTF-8 and `destination` to
/// `destination_length`, and `error` points to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_rename(
    source: *const u8,
    length: usize,
    destination: *const u8,
    destination_length: usize,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `source`.
    let source = unsafe { text(source, length) };
    // SAFETY: as for `source`.
    let destination = unsafe { text(destination, destination_length) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(paths::rename(source, destination), |()| {}, error) }
}

/// `os.mkdir(path)`, or `os.makedirs(path, exist_ok=exist_ok)` when `parents`: makes the
/// directory at `path`, and with `parents` every missing directory on the way to it. Returns
/// `true`, with the error at `error`, when the path exists, unless `parents` and `exist_ok` and
/// it is a directory, when a parent is missing without `parents`, or when the operating system
/// refuses. Without `parents`, `exist_ok` is not read.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8 and `error` to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_path_make_directory(
    path: *const u8,
    length: usize,
    parents: bool,
    exist_ok: bool,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let result = if parents {
        paths::make_directories(path, exist_ok)
    } else {
        paths::make_directory(path)
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
}

/// `os.getcwd()`: writes the directory the program runs in to `*out` as a new owned `str`.
/// Returns `true`, with the error at `error`, when the directory is gone or its path is not
/// UTF-8.
///
/// # Safety
///
/// `out` points to a writable [`OwnedText`] and `error` to a writable [`RaisedError`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_current_directory(
    out: *mut OwnedText,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `out` is writable.
    let write = |directory: String| unsafe { out.write(owned(&directory)) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(paths::current_directory(), write, error) }
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
