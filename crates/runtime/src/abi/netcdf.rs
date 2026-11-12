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

use super::array_files::write_header;
use super::files::{RaisedError, finish};
use super::fits::pixel;
use super::string_lists::{OwnedList, write_list};
use super::strings::text;
use crate::files::fits::Pixel;
use crate::files::netcdf::{self, Header};
use crate::files::{ErrorKind, OsFailure};
use crate::memory;
use crate::panic::panic;

/// `Dataset(path)`: writes to `out` a new `list[str]` of the lines that describe the header of
/// the `NetCDF` classic file whose path is the `length` bytes at `path`, as
/// `files::netcdf::describe` writes them: its dimensions, attributes and variables. Returns
/// `true`, with the error at `error`, when the operating system refuses, or with the kind
/// `INVALID_DATA` when the file is not a classic `NetCDF` file or its header is cut short.
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
    let write = |header: Header| {
        let lines = netcdf::describe(&header);
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        // SAFETY: the caller guarantees `out` is writable.
        unsafe { write_list(&lines, out) };
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `ds.variables[name][:]`: reads every value of the variable whose name is the `name_length`
/// bytes at `name`, in the `NetCDF` classic file whose path is the `length` bytes at `path`, into
/// a new row-major array of `rank` dimensions and elements of the type numbered `element`, and
/// writes its header to `out`. A float type reads any number variable with `scale_factor` and
/// `add_offset` applied as `netCDF4` applies them; an integer type reads an unscaled integer
/// variable it can hold. Fill values are read as stored. Returns `true`, with the error at
/// `error`, when the operating system refuses, or with the kind `INVALID_DATA` when the file is
/// not a classic `NetCDF` file, has no such variable, the variable has another rank or does not
/// read as the type, or the file ends inside it.
///
/// Cost: the header, one allocation, and one pass over the values, converted from big-endian
/// as they are read.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type, which the
/// checker never passes, or when the variable does not fit in memory.
///
/// # Safety
///
/// `path` points to `length` and `name` to `name_length` readable bytes of UTF-8, `out` to
/// `1 + 2 * rank` writable 64-bit words and `error` to a writable [`RaisedError`].
///
/// # Example
///
/// ```
/// use runtime::{
///     ELEMENT_F64, OwnedList, baryon_rt_deallocate, baryon_rt_netcdf_open,
///     baryon_rt_netcdf_read,
/// };
/// use std::mem::MaybeUninit;
///
/// // A file `netCDF4` wrote, with a variable `lat` of two `f8` values, 10 and 20.
/// let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../driver/tests/fixtures/netcdf/classic.nc");
/// let name = "lat";
/// let mut lines = MaybeUninit::<OwnedList>::uninit();
/// let mut loaded = [0_i64; 3];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the name are the strings' bytes, and `lines`, `loaded` and `error`
/// // are locals. The values read are freed once.
/// unsafe {
///     let failed =
///         baryon_rt_netcdf_open(path.as_ptr(), path.len(), lines.as_mut_ptr(), error.as_mut_ptr());
///     assert!(!failed);
///     // Three dimensions, three file attributes, five variables and their four attributes.
///     assert_eq!(lines.assume_init().length, 15);
///     let failed = baryon_rt_netcdf_read(
///         path.as_ptr(), path.len(), name.as_ptr(), name.len(), ELEMENT_F64, 1,
///         loaded.as_mut_ptr(), error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     assert_eq!(loaded[1..], [2, 1]);
///     let data = loaded[0] as *mut u8;
///     assert_eq!(data.cast::<f64>().add(1).read(), 20.0);
///     baryon_rt_deallocate(data, 16, 8);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_netcdf_read(
    path: *const u8,
    length: usize,
    name: *const u8,
    name_length: usize,
    element: u32,
    rank: usize,
    out: *mut i64,
    error: *mut RaisedError,
) -> bool {
    let pixel = pixel(element);
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees `name_length` readable bytes of UTF-8 at `name`.
    let name = unsafe { text(name, name_length) };
    let result =
        netcdf::read_header(path).and_then(|header| read(path, &header, name, pixel, rank));
    // SAFETY: the caller guarantees `1 + 2 * rank` writable words at `out`, and `read` gives a
    // shape `rank` long.
    let write = |(data, shape): (*mut u8, Vec<usize>)| unsafe { write_header(out, data, &shape) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// Reads the variable `name` of `header`, the file at `path`, into new memory as `pixel`s: the
/// memory and the shape. The memory is freed again when the read fails.
#[allow(unsafe_code, reason = "reads into memory it allocates")]
fn read(
    path: &str,
    header: &Header,
    name: &str,
    pixel: Pixel,
    rank: usize,
) -> Result<(*mut u8, Vec<usize>), OsFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let Some(variable) = header
        .variables
        .iter()
        .find(|variable| variable.name == name)
    else {
        return Err(invalid(format!("the NetCDF file has no variable {name}")));
    };
    if variable.dimensions.len() != rank {
        return Err(invalid(format!(
            "the NetCDF variable {name} has rank {}, not rank {rank} as the program asks",
            variable.dimensions.len()
        )));
    }
    let shape: Vec<usize> = variable
        .dimensions
        .iter()
        .map(|&dimension| usize::try_from(header.length(dimension)).unwrap_or(usize::MAX))
        .collect();
    let count = shape
        .iter()
        .try_fold(1_i64, |count, &length| {
            count.checked_mul(i64::try_from(length).ok()?)
        })
        .unwrap_or(i64::MAX);
    let size = pixel.size();
    let (pointer, layout) = match memory::array_layout(count, size, size)
        .and_then(|layout| Ok((memory::allocate_zeroed(layout)?, layout)))
    {
        Ok(memory) => memory,
        Err(failure) => panic(failure, None),
    };
    // SAFETY: the allocation holds `layout.size()` zeroed bytes that nothing else refers to; an
    // empty one is an aligned pointer that is never read through.
    let memory = unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), layout.size()) };
    if let Err(failure) = netcdf::read_variable(path, header, variable, pixel, memory) {
        // SAFETY: `pointer` came from `allocate_zeroed` with this layout just above.
        unsafe { memory::deallocate(pointer, layout) };
        return Err(failure);
    }
    Ok((pointer.as_ptr(), shape))
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
