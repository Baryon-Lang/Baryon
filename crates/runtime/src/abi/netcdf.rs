//! The functions compiled code calls for `NetCDF` classic files (decision 0047): the standard
//! library's `netCDF4` module calls them, and parses the header's description lines in Baryon.
//!
//! Each raises as the array file functions do (the `array_files` module): the last parameter is
//! the address of the caller's `Raises[None, OSError]`, and the result is `true` when the call
//! failed and wrote its error there. An element type is one of the `ELEMENT_*` numbers the
//! array printer uses: a signed integer or a float type, as the classic formats store only
//! those.
//!
//! An `Array[T, rank]` is its header, as compiled code lays it out: a pointer to the elements,
//! then `rank` lengths, then `rank` strides in elements, each 64 bits. A variable's axes are in
//! the file's order, slowest first, as `netCDF4` gives them.
//!
//! A subscript of a variable, `var[0, :, ::2]`, is passed as its items, four 64-bit words each:
//! `0` and an index, or `1` and a slice's start, stop and step, each `i64::MIN` when the program
//! left it out. No items stand for `var[:]`, the whole variable. A read or write with items can
//! also raise `IndexError` and `ValueError`, as `netCDF4` does, so its error value is a
//! `Raises[None, OSError | IndexError | ValueError]`: tag 1 or 2, then the error's `message`
//! where an `OSError` would start.

use super::array_files::runs;
use super::files::{RaisedError, finish, raise};
use super::fits::pixel;
use super::string_lists::{OwnedList, write_list};
use super::strings::text;
use super::text::{OwnedText, owned};
use crate::files::OsFailure;
use crate::files::netcdf::{self, Header, Item, SubscriptFailure};

mod reading;

pub use reading::{baryon_rt_netcdf_chunks, baryon_rt_netcdf_read};

/// `Dataset(path)`: writes to `out` a new `list[str]` of the lines that describe the header of
/// the `NetCDF` file, classic or NetCDF-4, whose path is the `length` bytes at `path`: `P`, a
/// tab and the path, which the library's `Dataset` keeps to read variables later, then the
/// lines `files::netcdf::describe` writes, for its dimensions, attributes and variables.
/// Returns `true`, with the error at `error`, when the operating system refuses, or with the kind
/// `INVALID_DATA` when the file is not a `NetCDF` file, its header is cut short, or a NetCDF-4
/// file holds what is not read yet.
///
/// Cost: the header only; no variable's values are read.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, `out` to a writable [`OwnedList`] and
/// `error` to a writable [`RaisedError`].
///
/// # Example
///
/// See [`baryon_rt_netcdf_read`], which reads the same file.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_netcdf_open(
    path: *const u8,
    length: usize,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let result = netcdf::read_header(path);
    // SAFETY: the caller guarantees `out` is writable.
    let write = |header: Header| unsafe { describe(path, &header, out) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `Dataset(path, "w")`, `ds.createDimension(...)`, `ds.createVariable(...)` and
/// `setncattr(...)`: applies one change, the `line_length` bytes at `line`, to the `NetCDF`
/// classic file whose path is the `length` bytes at `path`, as `files::netcdf::define`
/// describes the lines, then writes to `out` the lines that describe the file's new header, as
/// [`baryon_rt_netcdf_open`] does. The file is the only state: each change reads it, changes
/// it, and writes it again. Returns `true`, with the error at `error`, when the operating
/// system refuses, or with the kind `INVALID_DATA` when the change is one the format refuses.
///
/// Cost: one read and one write of the whole file, then a read of its header, which a program
/// pays once for each dimension, variable and attribute it defines.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `path` points to `length` and `line` to `line_length` readable bytes of UTF-8, `out` to a
/// writable [`OwnedList`] and `error` to a writable [`RaisedError`].
///
/// # Example
///
/// See [`baryon_rt_netcdf_write`], which defines a file this way.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_netcdf_define(
    path: *const u8,
    length: usize,
    line: *const u8,
    line_length: usize,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees `line_length` readable bytes of UTF-8 at `line`.
    let line = unsafe { text(line, line_length) };
    let result = netcdf::define(path, line).and_then(|()| netcdf::read_header(path));
    // SAFETY: the caller guarantees `out` is writable.
    let write = |header: Header| unsafe { describe(path, &header, out) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `ds.variables[name][items] = array`: writes every element of the array of `rank` dimensions
/// and elements of the type numbered `element` whose header is at `array` as the values
/// `items` pick of the variable whose name is the `name_length` bytes at `name`, in the
/// `NetCDF` classic file whose path is the `length` bytes at `path`; `items` are as
/// [`baryon_rt_netcdf_read`] takes them, and none write all the values, as `var[:] = array`
/// does. The values are converted to the variable's type and packed by `scale_factor` and
/// `add_offset` as `netCDF4` does; along the unlimited dimension the items may pick records
/// past the last one, which the file gains. Returns `true`, with the error at `error`, when
/// the operating system refuses, or with the kind `INVALID_DATA` when the file has no such
/// variable, the array's shape differs from the values picked, or the file would hold more
/// records than the format or memory allows; and with an `IndexError` or a `ValueError` as
/// [`baryon_rt_netcdf_read`] raises them.
///
/// Cost: the array's elements gathered once, then one read and one write of the whole file.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type, which the
/// checker never passes.
///
/// # Safety
///
/// `path` points to `length` and `name` to `name_length` readable bytes of UTF-8; `array` to a
/// readable array header of `rank` dimensions, whose every element its lengths and strides
/// reach is readable and unchanged during the call; `items` to `4 * item_count` readable 64-bit
/// words; and `error` to a writable [`RaisedError`], a
/// `Raises[None, OSError | IndexError | ValueError]` in that order when `item_count` is not 0.
///
/// # Example
///
/// ```
/// use runtime::{
///     ELEMENT_F64, OwnedList, baryon_rt_deallocate, baryon_rt_netcdf_define,
///     baryon_rt_netcdf_read, baryon_rt_netcdf_write,
/// };
/// use std::mem::MaybeUninit;
///
/// let path = std::env::temp_dir().join(format!("baryon-doc-netcdf-{}.nc", std::process::id()));
/// let path = path.to_str().unwrap();
/// let mut values = [1.5_f64, 2.5, 3.5];
/// // A rank-1 array of three: the pointer, the length, then the stride.
/// let array = [values.as_mut_ptr() as i64, 3, 1];
/// let mut lines = MaybeUninit::<OwnedList>::uninit();
/// let mut loaded = [0_i64; 3];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path, the lines and the name are the strings' bytes, `array` describes
/// // `values`, and `lines`, `loaded` and `error` are locals. The values read are freed once;
/// // the description lines are left to the process's end.
/// unsafe {
///     for line in ["C\t1", "D\tx\t3", "V\tv\tf8\tx"] {
///         let failed = baryon_rt_netcdf_define(
///             path.as_ptr(), path.len(), line.as_ptr(), line.len(), lines.as_mut_ptr(),
///             error.as_mut_ptr(),
///         );
///         assert!(!failed);
///     }
///     // The path, the dimension and the variable.
///     assert_eq!(lines.assume_init().length, 3);
///     // No items: `var[:] = values`.
///     let failed = baryon_rt_netcdf_write(
///         path.as_ptr(), path.len(), "v".as_ptr(), 1, std::ptr::null(), 0, array.as_ptr(), 1,
///         ELEMENT_F64, error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     // No inflated chunks: a classic file has none.
///     let empty = [8_i64, 0, 1];
///     let failed = baryon_rt_netcdf_read(
///         path.as_ptr(), path.len(), "v".as_ptr(), 1, std::ptr::null(), 0, empty.as_ptr(),
///         empty.as_ptr(), ELEMENT_F64, 1, loaded.as_mut_ptr(), error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     let data = loaded[0] as *mut u8;
///     assert_eq!(data.cast::<f64>().add(2).read(), 3.5);
///     baryon_rt_deallocate(data, 24, 8);
/// }
/// std::fs::remove_file(path).unwrap();
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_netcdf_write(
    path: *const u8,
    length: usize,
    name: *const u8,
    name_length: usize,
    items: *const i64,
    item_count: usize,
    array: *const i64,
    rank: usize,
    element: u32,
    error: *mut RaisedError,
) -> bool {
    let pixel = pixel(element);
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees `name_length` readable bytes of UTF-8 at `name`.
    let name = unsafe { text(name, name_length) };
    // SAFETY: the caller guarantees `4 * item_count` readable words at `items`.
    let items = unsafe { subscript(items, item_count) };
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
    let mut values = Vec::new();
    // SAFETY: the caller's promise about the elements, passed on; writing to a `Vec` cannot
    // fail.
    let gathered = unsafe { runs(&mut values, data, lengths, strides, pixel.size()) };
    let result = gathered
        .map_err(|failure| SubscriptFailure::Os(OsFailure::from_io(&failure, path)))
        .and_then(|()| netcdf::write(path, (name, &items), pixel, &shape, &values));
    // SAFETY: the caller's promises about `error` and its error types, passed on.
    unsafe { finish_subscript(result, |()| {}, error) }
}

/// Writes `result`'s value with `write` and returns `false`, or raises its failure at `error`
/// and returns `true`, the failure flag: an `OSError` as every file function raises it, tag 0,
/// or the `IndexError`, tag 1, or `ValueError`, tag 2, of a subscript that picks no values.
/// Either of those two is one owned `str`, its `message`, where the `OSError` would start.
///
/// # Safety
///
/// `error` points to a writable `Raises[None, OSError | IndexError | ValueError]`, its error
/// types in that order, whenever the result can be an `IndexError` or a `ValueError`: the
/// checker types a read or write with subscript items so, and only one with items fails so.
#[allow(unsafe_code, reason = "writes an error compiled code owns")]
pub(super) unsafe fn finish_subscript<T>(
    result: Result<T, SubscriptFailure>,
    write: impl FnOnce(T),
    error: *mut RaisedError,
) -> bool {
    let (tag, message) = match result {
        Ok(value) => {
            write(value);
            return false;
        }
        Err(SubscriptFailure::Os(failure)) => {
            // SAFETY: the caller's promise about `error`, passed on.
            return unsafe { raise(&failure, error) };
        }
        Err(SubscriptFailure::Index(message)) => (1, message),
        Err(SubscriptFailure::Value(message)) => (2, message),
    };
    let raised = RaisedMessage {
        tag,
        message: owned(&message),
    };
    // SAFETY: the caller guarantees `error` is writable and holds this error type, which is
    // laid out as a `RaisedMessage`: an `OSError` and an `OwnedText` both start at offset 8.
    unsafe { error.cast::<RaisedMessage>().write(raised) };
    true
}

/// An `IndexError` or a `ValueError` raised, as compiled code lays out the error value of a
/// `Raises[None, OSError | IndexError | ValueError]` that holds one: the tag, then the error's
/// one field, its `message`, an owned `str`, where an `OSError` would start.
#[repr(C)]
pub(super) struct RaisedMessage {
    /// Which of the error types it is: 1, `IndexError`, or 2, `ValueError`.
    pub(super) tag: u8,
    /// The error's `message`.
    pub(super) message: OwnedText,
}

/// Writes to `out` a new `list[str]` of the lines that describe `header`, the file at `path`:
/// `P`, a tab and the path, then the lines `files::netcdf::describe` writes.
///
/// # Safety
///
/// `out` points to a writable [`OwnedList`].
#[allow(unsafe_code, reason = "writes a list compiled code owns")]
unsafe fn describe(path: &str, header: &Header, out: *mut OwnedList) {
    let mut lines = vec![format!("P\t{path}")];
    lines.extend(netcdf::describe(header));
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { write_list(&lines, out) };
}

/// The items of a subscript at `words`, `count` of them, four words each.
///
/// # Safety
///
/// `words` points to `4 * count` readable 64-bit words, or `count` is 0.
#[allow(unsafe_code, reason = "reads words compiled code passes")]
pub(super) unsafe fn subscript(words: *const i64, count: usize) -> Vec<Item> {
    if count == 0 {
        return Vec::new();
    }
    // SAFETY: the caller guarantees `4 * count` readable words.
    Item::from_words(unsafe { std::slice::from_raw_parts(words, 4 * count) })
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
