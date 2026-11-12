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
//! left it out. No items stand for `var[:]`, the whole variable.

use super::array_files::{runs, write_header};
use super::files::{RaisedError, finish};
use super::fits::pixel;
use super::string_lists::{OwnedList, write_list};
use super::strings::text;
use crate::files::fits::Pixel;
use crate::files::netcdf::{self, Header, Item};
use crate::files::{ErrorKind, OsFailure};
use crate::memory;
use crate::panic::panic;

/// `Dataset(path)`: writes to `out` a new `list[str]` of the lines that describe the header of
/// the `NetCDF` classic file whose path is the `length` bytes at `path`: `P`, a tab and the
/// path, which the library's `Dataset` keeps to read variables later, then the lines
/// `files::netcdf::describe` writes, for its dimensions, attributes and variables. Returns
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
    // SAFETY: the caller guarantees `out` is writable.
    let write = |header: Header| unsafe { describe(path, &header, out) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `ds.variables[name][items]`: reads the values `items` pick of the variable whose name is the
/// `name_length` bytes at `name`, in the `NetCDF` classic file whose path is the `length` bytes
/// at `path`, into a new row-major array of `rank` dimensions and elements of the type numbered
/// `element`, and writes its header to `out`. `items` points to `item_count` items of the
/// subscript, four 64-bit words each: `0` and an index, or `1` and a slice's start, stop and
/// step, each `i64::MIN` when left out; the axes after the last item are read whole, so no
/// items read the whole variable, as `var[:]` does. A float type reads any number variable with
/// `scale_factor` and `add_offset` applied as `netCDF4` applies them; an integer type reads an
/// unscaled integer variable it can hold. Fill values are read as stored. Returns `true`, with
/// the error at `error`, when the operating system refuses, or with the kind `INVALID_DATA`
/// when the file is not a classic `NetCDF` file, has no such variable, the items do not fit its
/// dimensions, the values picked have another rank or do not read as the type, or the file
/// ends inside them.
///
/// Cost: the header, one allocation, and one pass over the values picked, converted from
/// big-endian as they are read, with one seek for each stretch of them that lies together.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type, which the
/// checker never passes, or when the variable does not fit in memory.
///
/// # Safety
///
/// `path` points to `length` and `name` to `name_length` readable bytes of UTF-8, `items` to
/// `4 * item_count` readable 64-bit words, `out` to `1 + 2 * rank` writable 64-bit words and
/// `error` to a writable [`RaisedError`].
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
/// // `lat[1:]`: a slice from 1, its stop and step left out.
/// let items = [1, 1, i64::MIN, i64::MIN];
/// let mut lines = MaybeUninit::<OwnedList>::uninit();
/// let mut loaded = [0_i64; 3];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the name are the strings' bytes, and `lines`, `loaded` and `error`
/// // are locals. The values read are freed once.
/// unsafe {
///     let failed =
///         baryon_rt_netcdf_open(path.as_ptr(), path.len(), lines.as_mut_ptr(), error.as_mut_ptr());
///     assert!(!failed);
///     // The path, three dimensions, three file attributes, five variables and their four
///     // attributes.
///     assert_eq!(lines.assume_init().length, 16);
///     let failed = baryon_rt_netcdf_read(
///         path.as_ptr(), path.len(), name.as_ptr(), name.len(), items.as_ptr(), 1, ELEMENT_F64,
///         1, loaded.as_mut_ptr(), error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     assert_eq!(loaded[1..], [1, 1]);
///     let data = loaded[0] as *mut u8;
///     assert_eq!(data.cast::<f64>().read(), 20.0);
///     baryon_rt_deallocate(data, 8, 8);
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
    items: *const i64,
    item_count: usize,
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
    // SAFETY: the caller guarantees `4 * item_count` readable words at `items`.
    let items = unsafe { subscript(items, item_count) };
    let result = netcdf::read_header(path)
        .and_then(|header| read(path, &header, (name, &items), pixel, rank));
    // SAFETY: the caller guarantees `1 + 2 * rank` writable words at `out`, and `read` gives a
    // shape `rank` long.
    let write = |(data, shape): (*mut u8, Vec<usize>)| unsafe { write_header(out, data, &shape) };
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

/// `ds.variables[name][:] = array`: writes every element of the array of `rank` dimensions and
/// elements of the type numbered `element` whose header is at `array` as all the values of the
/// variable whose name is the `name_length` bytes at `name`, in the `NetCDF` classic file whose
/// path is the `length` bytes at `path`. The values are converted to the variable's type and
/// packed by `scale_factor` and `add_offset` as `netCDF4` does; a record variable takes as many
/// records as the array's first length. Returns `true`, with the error at `error`, when the
/// operating system refuses, or with the kind `INVALID_DATA` when the file has no such
/// variable or the array's rank or a fixed length differs from the variable's.
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
/// reach is readable and unchanged during the call; and `error` to a writable [`RaisedError`].
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
///     let failed = baryon_rt_netcdf_read(
///         path.as_ptr(), path.len(), "v".as_ptr(), 1, std::ptr::null(), 0, ELEMENT_F64, 1,
///         loaded.as_mut_ptr(), error.as_mut_ptr(),
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
        .map_err(|failure| OsFailure::from_io(&failure, path))
        .and_then(|()| netcdf::write(path, (name, &items), pixel, &shape, &values));
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
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

/// Reads the values `items` pick of the variable `name` of `header`, the file at `path`, into
/// new memory as `pixel`s: the memory and the shape. The memory is freed again when the read
/// fails.
#[allow(unsafe_code, reason = "reads into memory it allocates")]
fn read(
    path: &str,
    header: &Header,
    (name, items): (&str, &[Item]),
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
    let names: Vec<&str> = variable
        .dimensions
        .iter()
        .map(|&dimension| header.dimensions[dimension].0.as_str())
        .collect();
    let lengths: Vec<u64> = variable
        .dimensions
        .iter()
        .map(|&dimension| header.length(dimension))
        .collect();
    let record = header.is_record(variable);
    let selection =
        netcdf::select(name, items, (&names, &lengths), record, None).map_err(invalid)?;
    let shape = selection.shape();
    if shape.len() != rank {
        let what = if netcdf::is_whole(items) {
            format!("the NetCDF variable {name} has")
        } else {
            format!("the values picked of the NetCDF variable {name} have")
        };
        return Err(invalid(format!(
            "{what} rank {}, not rank {rank} as the program asks",
            shape.len()
        )));
    }
    let count = i64::try_from(selection.count()).unwrap_or(i64::MAX);
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
    if let Err(failure) = netcdf::read_variable(path, header, (variable, &selection), pixel, memory)
    {
        // SAFETY: `pointer` came from `allocate_zeroed` with this layout just above.
        unsafe { memory::deallocate(pointer, layout) };
        return Err(failure);
    }
    Ok((pointer.as_ptr(), shape))
}

/// The items of a subscript at `words`, `count` of them, four words each.
///
/// # Safety
///
/// `words` points to `4 * count` readable 64-bit words, or `count` is 0.
#[allow(unsafe_code, reason = "reads words compiled code passes")]
unsafe fn subscript(words: *const i64, count: usize) -> Vec<Item> {
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
