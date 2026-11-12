//! The functions compiled code calls for `NumPy`'s `.npy` files (decision 0043):
//! `Array.load(path)` and `Array.save(path, a)`.
//!
//! Both raise as the array file functions do (the `array_files` module): the last parameter is
//! the address of the caller's `Raises[None, OSError]`, and the result is `true` when the call
//! failed and wrote its error there. The compiler passes the element type in `NumPy`'s
//! spelling, such as `<f8`, which it works out from the type; the runtime writes it into the
//! header, or checks the file's header against it.
//!
//! An `Array[T, rank]` is its header, as compiled code lays it out: a pointer to the elements,
//! then `rank` lengths, then `rank` strides in elements, each 64 bits.

use super::array_files::{ElementLayout, read_elements, runs, write_header};
use super::files::{RaisedError, finish};
use super::strings::text;
use crate::files::npy;
use crate::files::{ErrorKind, OsFailure, arrays};
use std::io::Write;

/// `Array.load(path)`: reads the `.npy` file whose path is the `length` bytes at `path` into a
/// new array of `rank` dimensions and elements of the layout at `element`, which `NumPy` spells
/// as the `descr_length` bytes at `descr`, and writes the new array's header to `out`: the
/// pointer, the `rank` lengths the file names, and the row-major strides. A byte read where the
/// element has a `bool` becomes 1 unless it is 0. Returns `true`, with the error at `error`,
/// when the operating system refuses, or with the kind `INVALID_DATA` when the file is not a
/// `.npy` file, names another element type or rank, is in Fortran order, or holds fewer
/// elements than its shape.
///
/// Cost: the header, then one allocation and one pass over the bytes, read straight into the
/// array, as [`baryon_rt_array_from_file`](super::baryon_rt_array_from_file) reads.
///
/// # Panics
///
/// Ends the program with a panic when the array does not fit in memory, as any allocation does.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8 and `descr` to `descr_length`; `element`
/// points to a readable [`ElementLayout`] whose `bools` points to `bool_count` readable
/// offsets, each below `size`; `out` points to `1 + 2 * rank` writable 64-bit words, and
/// `error` to a writable [`RaisedError`].
///
/// # Example
///
/// ```
/// use runtime::{ElementLayout, baryon_rt_array_load, baryon_rt_array_save, baryon_rt_deallocate};
/// use std::mem::MaybeUninit;
///
/// let path = std::env::temp_dir().join(format!("baryon-doc-load-{}.npy", std::process::id()));
/// let path = path.to_str().unwrap();
/// let mut values = [1.5_f64, 2.5, 3.5, 4.5, 5.5, 6.5];
/// // A 2 by 3 array: the pointer, the lengths, then the strides.
/// let saved = [values.as_mut_ptr() as i64, 2, 3, 3, 1];
/// let element = ElementLayout { size: 8, align: 8, bool_count: 0, bools: std::ptr::null() };
/// let mut loaded = [0_i64; 5];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the type are the strings' bytes, `saved` describes `values`, the
/// // layout is an `f64`'s, and `loaded` and `error` are locals. The loaded elements are freed
/// // once.
/// unsafe {
///     let failed = baryon_rt_array_save(
///         path.as_ptr(), path.len(), saved.as_ptr(), 2, 8, "<f8".as_ptr(), 3, error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     let failed = baryon_rt_array_load(
///         path.as_ptr(), path.len(), &element, "<f8".as_ptr(), 3, 2, loaded.as_mut_ptr(),
///         error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     assert_eq!(loaded[1..], [2, 3, 3, 1]);
///     let data = loaded[0] as *mut u8;
///     assert_eq!(data.cast::<f64>().add(5).read(), 6.5);
///     baryon_rt_deallocate(data, 48, 8);
/// }
/// std::fs::remove_file(path).unwrap();
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_array_load(
    path: *const u8,
    length: usize,
    element: *const ElementLayout,
    descr: *const u8,
    descr_length: usize,
    rank: usize,
    out: *mut i64,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: and `descr_length` at `descr`.
    let descr = unsafe { text(descr, descr_length) };
    let header = npy::read_header(path).and_then(|header| {
        npy::check(&header, descr, rank)
            .map(|()| header)
            .map_err(|message| OsFailure::new(ErrorKind::InvalidData, message, path))
    });
    let header = match header {
        Ok(header) => header,
        // SAFETY: the caller guarantees `error` is writable.
        Err(failure) => return unsafe { finish(Err::<(), _>(failure), |()| {}, error) },
    };
    let count = header
        .shape
        .iter()
        .try_fold(1_usize, |count, &length| count.checked_mul(length))
        .and_then(|count| i64::try_from(count).ok())
        .unwrap_or(i64::MAX);
    let offset = i64::try_from(header.offset).unwrap_or(i64::MAX);
    // SAFETY: the caller's promise about `element`, passed on.
    let (result, data) = unsafe { read_elements(path, element, count, offset) };
    // SAFETY: the caller guarantees `1 + 2 * rank` writable words at `out`, and `check` made
    // the shape `rank` long.
    let write = |_: usize| unsafe { write_header(out, data, &header.shape) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `Array.save(path, a)`: writes the array whose header is at `array`, of `rank` dimensions and
/// elements of `element_size` bytes that `NumPy` spells as the `descr_length` bytes at `descr`,
/// to the file whose path is the `length` bytes at `path`, as a `.npy` file: `NumPy`'s header,
/// then every element in row-major order. Makes the file or empties the one there. Returns
/// `true`, with the error at `error`, when the operating system refuses or a write fails.
///
/// Cost: the header, then one pass over the bytes, as
/// [`baryon_rt_array_to_file`](super::baryon_rt_array_to_file) writes them.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8 and `descr` to `descr_length`; `array`
/// points to a readable array header of `rank` dimensions, 8 bytes each for the pointer, every
/// length and every stride; every element its lengths and strides reach is `element_size`
/// readable bytes that nothing writes during the call; `error` points to a writable
/// [`RaisedError`].
///
/// # Example
///
/// See [`baryon_rt_array_load`], which reads back what this writes.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_array_save(
    path: *const u8,
    length: usize,
    array: *const i64,
    rank: usize,
    element_size: usize,
    descr: *const u8,
    descr_length: usize,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: and `descr_length` at `descr`.
    let descr = unsafe { text(descr, descr_length) };
    // SAFETY: the caller guarantees a header at `array`, whose first word is the pointer.
    let data = unsafe { array.cast::<*const u8>().read() };
    // SAFETY: the header's `rank` lengths follow the pointer.
    let lengths = unsafe { std::slice::from_raw_parts(array.wrapping_add(1), rank) };
    // SAFETY: and its `rank` strides follow the lengths.
    let strides = unsafe { std::slice::from_raw_parts(array.wrapping_add(1 + rank), rank) };
    let shape: Vec<usize> = lengths
        .iter()
        .map(|&length| usize::try_from(length).unwrap_or(0))
        .collect();
    let result = arrays::write(path, |writer: &mut dyn Write| {
        writer.write_all(&npy::header(descr, &shape))?;
        // SAFETY: the caller's promise about the elements, passed on.
        unsafe { runs(writer, data, lengths, strides, element_size) }
    });
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
