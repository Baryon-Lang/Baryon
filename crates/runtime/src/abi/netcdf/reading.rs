//! Reading a `NetCDF` variable's values: `ds.variables[name][items]`, and the deflated chunks a
//! NetCDF-4 variable's read takes inflated, which the standard library's `zlib` inflates
//! between the two calls (decision 0049), as it does for the `h5by` module.

use super::super::array_files::write_header;
use super::super::files::RaisedError;
use super::super::fits::pixel;
use super::super::hdf5::{chunk_sizes, gather, inflater};
use super::super::string_lists::{OwnedList, write_list};
use super::super::strings::text;
use super::{finish_subscript, subscript};
use crate::files::fits::Pixel;
use crate::files::hdf5::Inflate;
use crate::files::netcdf::{self, Header, Item, Selection, SubscriptFailure, Variable};
use crate::files::{ErrorKind, OsFailure};
use crate::memory;
use crate::panic::panic;

/// Writes to `out` a new `list[str]` with one line for each deflated chunk that holds a value
/// `items` pick of the variable whose name is the `name_length` bytes at `name`, in the
/// `NetCDF` file whose path is the `length` bytes at `path`: the chunk's address and its size in
/// bytes, separated by a tab, in the order [`baryon_rt_netcdf_read`] takes them inflated.
/// `items` are as [`baryon_rt_netcdf_read`] takes them. A classic file, or a NetCDF-4 variable
/// stored without deflate, gives an empty list. Returns `true`, with the error at `error`, as
/// [`baryon_rt_netcdf_read`] does.
///
/// Cost: the file's header and, for a NetCDF-4 file, the variable's chunk index.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `path` points to `length` and `name` to `name_length` readable bytes of UTF-8, `items` to
/// `4 * item_count` readable 64-bit words or `item_count` is 0, `out` to a writable
/// [`OwnedList`], and `error` to a writable [`RaisedError`] that is a
/// `Raises[None, OSError | IndexError | ValueError]` whenever `item_count` is not 0.
///
/// # Example
///
/// ```
/// use runtime::{OwnedList, baryon_rt_netcdf_chunks};
/// use std::mem::MaybeUninit;
///
/// // A classic file, whose variables are never compressed.
/// let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../driver/tests/fixtures/netcdf/classic.nc");
/// let name = "temp";
/// let mut lines = MaybeUninit::<OwnedList>::uninit();
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the name are the strings' bytes; `lines` and `error` are locals.
/// unsafe {
///     let failed = baryon_rt_netcdf_chunks(
///         path.as_ptr(), path.len(), name.as_ptr(), name.len(), std::ptr::null(), 0,
///         lines.as_mut_ptr(), error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     assert_eq!(lines.assume_init().length, 0);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_netcdf_chunks(
    path: *const u8,
    length: usize,
    name: *const u8,
    name_length: usize,
    items: *const i64,
    item_count: usize,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees `name_length` readable bytes of UTF-8 at `name`.
    let name = unsafe { text(name, name_length) };
    // SAFETY: the caller guarantees `4 * item_count` readable words at `items`.
    let items = unsafe { subscript(items, item_count) };
    let result = netcdf::read_header(path)
        .map_err(SubscriptFailure::Os)
        .and_then(|header| {
            let (variable, selection) = selected(path, &header, (name, &items))?;
            netcdf::deflated_chunks(path, &header, (variable, &selection))
                .map_err(SubscriptFailure::Os)
        });
    // SAFETY: the caller guarantees `out` is writable.
    let write = |chunks: Vec<(u64, u64)>| unsafe {
        let lines: Vec<String> = chunks
            .iter()
            .map(|(address, size)| format!("{address}\t{size}"))
            .collect();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        write_list(&lines, out);
    };
    // SAFETY: the caller's promises about `error` and its error types, passed on.
    unsafe { finish_subscript(result, write, error) }
}

/// `ds.variables[name][items]`: reads the values `items` pick of the variable whose name is the
/// `name_length` bytes at `name`, in the `NetCDF` file, classic or NetCDF-4, whose path is the
/// `length` bytes at `path`, into a new row-major array of `rank` dimensions and elements of the type numbered
/// `element`, and writes its header to `out`. `items` points to `item_count` items of the
/// subscript, four 64-bit words each: `0` and an index, or `1` and a slice's start, stop and
/// step, each `i64::MIN` when left out; the axes after the last item are read whole, so no
/// items read the whole variable, as `var[:]` does. `inflated` is the header of an
/// `Array[u8, 1]` holding a NetCDF-4 variable's deflated chunks inflated, one after another in
/// the order [`baryon_rt_netcdf_chunks`] lists them, and `sizes` the header of an
/// `Array[i64, 1]` of their sizes; both are empty for a classic file. A float type reads any number variable with
/// `scale_factor` and `add_offset` applied as `netCDF4` applies them; an integer type reads an
/// unscaled integer variable it can hold. Fill values are read as stored. Returns `true`, with
/// the error at `error`, when the operating system refuses, or with the kind `INVALID_DATA`
/// when the file is not a `NetCDF` file, has no such variable, the inflated chunks run out, the values picked have
/// another rank or do not read as the type, or the file ends inside them; and with an
/// `IndexError` for an index past its axis or more items than axes, or a `ValueError` for a
/// step of 0, as `netCDF4` raises them.
///
/// Cost: the header, one allocation, and one pass over the values picked: in a classic file
/// converted from big-endian as they are read, with one seek for each stretch of them that lies
/// together; in a NetCDF-4 file read as the HDF5 reader reads the block that holds them.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type, which the
/// checker never passes, or when the variable does not fit in memory.
///
/// # Safety
///
/// `path` points to `length` and `name` to `name_length` readable bytes of UTF-8, `items` to
/// `4 * item_count` readable 64-bit words, `inflated` and `sizes` to readable rank-1 array
/// headers of bytes and of 64-bit integers, `out` to `1 + 2 * rank` writable 64-bit words and
/// `error` to a writable [`RaisedError`], a `Raises[None, OSError | IndexError | ValueError]`
/// in that order when `item_count` is not 0.
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
/// // No inflated chunks: a classic file has none.
/// let empty = [8_i64, 0, 1];
/// let mut lines = MaybeUninit::<OwnedList>::uninit();
/// let mut loaded = [0_i64; 3];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the name are the strings' bytes, the empty arrays are never read,
/// // and `lines`, `loaded` and `error` are locals. The values read are freed once.
/// unsafe {
///     let failed =
///         baryon_rt_netcdf_open(path.as_ptr(), path.len(), lines.as_mut_ptr(), error.as_mut_ptr());
///     assert!(!failed);
///     // The path, three dimensions, three file attributes, five variables and their four
///     // attributes.
///     assert_eq!(lines.assume_init().length, 16);
///     let failed = baryon_rt_netcdf_read(
///         path.as_ptr(), path.len(), name.as_ptr(), name.len(), items.as_ptr(), 1,
///         empty.as_ptr(), empty.as_ptr(), ELEMENT_F64, 1, loaded.as_mut_ptr(),
///         error.as_mut_ptr(),
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
    inflated: *const i64,
    sizes: *const i64,
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
    // SAFETY: the caller guarantees a readable rank-1 array of bytes at `inflated`.
    let inflated = unsafe { gather(inflated, 1) };
    // SAFETY: the caller guarantees a readable rank-1 array of 64-bit integers at `sizes`.
    let sizes = unsafe { chunk_sizes(sizes) };
    let result = netcdf::read_header(path)
        .map_err(SubscriptFailure::Os)
        .and_then(|header| {
            let mut inflate = inflater(&inflated, &sizes);
            read(path, &header, (name, &items), (pixel, rank), &mut inflate)
        });
    // SAFETY: the caller guarantees `1 + 2 * rank` writable words at `out`, and `read` gives a
    // shape `rank` long.
    let write = |(data, shape): (*mut u8, Vec<usize>)| unsafe { write_header(out, data, &shape) };
    // SAFETY: the caller's promises about `error` and its error types, passed on.
    unsafe { finish_subscript(result, write, error) }
}

/// Reads the values `items` pick of the variable `name` of `header`, the file at `path`, into
/// new memory as `pixel`s, a deflated chunk inflated by `inflate`: the memory and the shape.
/// The memory is freed again when the read fails.
#[allow(unsafe_code, reason = "reads into memory it allocates")]
fn read(
    path: &str,
    header: &Header,
    (name, items): (&str, &[Item]),
    (pixel, rank): (Pixel, usize),
    inflate: &mut Inflate<'_>,
) -> Result<(*mut u8, Vec<usize>), SubscriptFailure> {
    let invalid = |message: String| {
        SubscriptFailure::Os(OsFailure::new(ErrorKind::InvalidData, message, path))
    };
    let (variable, selection) = selected(path, header, (name, items))?;
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
    if let Err(failure) =
        netcdf::read_variable(path, header, (variable, &selection), pixel, memory, inflate)
    {
        // SAFETY: `pointer` came from `allocate_zeroed` with this layout just above.
        unsafe { memory::deallocate(pointer, layout) };
        return Err(SubscriptFailure::Os(failure));
    }
    Ok((pointer.as_ptr(), shape))
}

/// The variable `name` of `header`, the file at `path`, and the values `items` pick of it.
fn selected<'a>(
    path: &str,
    header: &'a Header,
    (name, items): (&str, &[Item]),
) -> Result<(&'a Variable, Selection), SubscriptFailure> {
    let Some(variable) = header
        .variables
        .iter()
        .find(|variable| variable.name == name)
    else {
        return Err(SubscriptFailure::Os(OsFailure::new(
            ErrorKind::InvalidData,
            format!("the NetCDF file has no variable {name}"),
            path,
        )));
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
    let selection = netcdf::select(name, items, (&names, &lengths), record, None)
        .map_err(|refusal| refusal.at(path))?;
    Ok((variable, selection))
}
