//! The function compiled code calls to write HDF5 files (decision 0052): the standard
//! library's `h5by` module keeps each file's log of changes, a `list[str]`, and passes it whole
//! with every change, as `files::hdf5::writing` documents.

use super::super::array_files::runs;
use super::super::files::{RaisedError, finish};
use super::super::fits::pixel;
use super::super::string_lists::TextValue;
use super::super::strings::text;
use crate::files::OsFailure;
use crate::files::hdf5;

/// A change to the HDF5 file whose path is the `length` bytes at `path`: the `log_count` lines
/// at `log` are its log, the last line the change; `array`, when not null, is the header of
/// the new dataset's values, an array of `rank` dimensions and elements of the type numbered
/// `element`. No lines make a new, empty file, replacing any there. Returns `true`, with the
/// error at `error`, when the operating system refuses, or with the kind `INVALID_DATA` when a
/// line is not one the log holds, h5py would refuse the change (a name already taken, a group
/// that is not there), or the array does not fill the dataset.
///
/// Cost: the array's elements gathered once; the log replayed, which builds the file's
/// metadata; and the metadata and the new values written.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type, which the
/// checker never passes.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8; `log` to `log_count` readable `str`
/// values, each pointing to readable UTF-8; `array`, unless null, to a readable array header of
/// `rank` dimensions, whose every element its lengths and strides reach is readable and
/// unchanged during the call; and `error` to a writable [`RaisedError`].
///
/// # Example
///
/// ```
/// use runtime::{ELEMENT_F64, TextValue, baryon_rt_hdf5_write};
/// use std::mem::MaybeUninit;
///
/// let path = std::env::temp_dir().join(format!("baryon-doc-h5w-{}.h5", std::process::id()));
/// let path = path.to_str().unwrap();
/// let lines = ["G\truns", "D\truns/x\t<f8\t3"];
/// let log: Vec<TextValue> = lines
///     .iter()
///     .map(|line| TextValue { bytes: line.as_ptr(), word: line.len() as u64 })
///     .collect();
/// let mut values = [1.5_f64, 2.5, 3.5];
/// // A rank-1 array of three: the pointer, the length, then the stride.
/// let array = [values.as_mut_ptr() as i64, 3, 1];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the lines are the strings' bytes, `array` describes `values`, and
/// // `error` is a local.
/// unsafe {
///     for count in 0..=2 {
///         let array = if count == 2 { array.as_ptr() } else { std::ptr::null() };
///         let failed = baryon_rt_hdf5_write(
///             path.as_ptr(), path.len(), log.as_ptr(), count, array, 1, ELEMENT_F64,
///             error.as_mut_ptr(),
///         );
///         assert!(!failed);
///     }
/// }
/// // As long as the file h5py writes for `f.create_group("runs")`, then
/// // `f.create_dataset("runs/x", data=[1.5, 2.5, 3.5])`.
/// assert_eq!(std::fs::metadata(path).unwrap().len(), 2456);
/// std::fs::remove_file(path).unwrap();
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_hdf5_write(
    path: *const u8,
    length: usize,
    log: *const TextValue,
    log_count: usize,
    array: *const i64,
    rank: usize,
    element: u32,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let lines: &[TextValue] = if log_count == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `log_count` readable values at `log`.
        unsafe { std::slice::from_raw_parts(log, log_count) }
    };
    let lines: Vec<&str> = lines
        .iter()
        .map(|line| {
            let length = usize::try_from(line.word & !(1 << 63)).unwrap_or(0);
            // SAFETY: the caller guarantees each line points to `length` readable bytes of
            // UTF-8.
            unsafe { text(line.bytes, length) }
        })
        .collect();
    let mut values = Vec::new();
    if !array.is_null() {
        let size = pixel(element).size();
        // SAFETY: the caller guarantees a header at `array`, whose first word is the pointer.
        let data = unsafe { array.cast::<*const u8>().read() };
        // SAFETY: the header's `rank` lengths follow the pointer.
        let lengths = unsafe { std::slice::from_raw_parts(array.wrapping_add(1), rank) };
        // SAFETY: and its `rank` strides follow the lengths.
        let strides = unsafe { std::slice::from_raw_parts(array.wrapping_add(1 + rank), rank) };
        // SAFETY: the caller's promise about the elements, passed on; writing to a `Vec`
        // cannot fail.
        let gathered = unsafe { runs(&mut values, data, lengths, strides, size) };
        if let Err(failure) = gathered {
            // SAFETY: the caller guarantees `error` is writable.
            return unsafe {
                finish(
                    Err::<(), _>(OsFailure::from_io(&failure, path)),
                    |()| {},
                    error,
                )
            };
        }
    }
    let result = hdf5::write(path, &lines, &values);
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
}
