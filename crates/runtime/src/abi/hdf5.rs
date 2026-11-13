//! The functions compiled code calls for HDF5 files (decision 0052): the standard library's
//! `h5py` module calls them, and parses the description lines in Baryon.
//!
//! Each raises as the array file functions do (the `array_files` module): the last parameter is
//! the address of the caller's `Raises[None, OSError]`, and the result is `true` when the call
//! failed and wrote its error there. An element type is one of the `ELEMENT_*` numbers the
//! array printer uses.
//!
//! Deflate is decoded in Baryon, by the standard library's `zlib` (decision 0049), not here.
//! So a read of a deflated dataset takes two calls: [`baryon_rt_hdf5_chunks`] lists where its
//! deflated chunks are, the library reads and inflates each, and [`baryon_rt_hdf5_read`] takes
//! the inflated bytes, undoes the other filters and converts the values.
//!
//! An `Array[T, rank]` is its header, as compiled code lays it out: a pointer to the elements,
//! then `rank` lengths, then `rank` strides in elements, each 64 bits.

use super::array_files::{runs, write_header};
use super::files::{RaisedError, finish};
use super::fits::pixel;
use super::netcdf::{finish_subscript, subscript};
use super::string_lists::{OwnedList, write_list};
use super::strings::text;
use crate::files::fits::Pixel;
use crate::files::hdf5::{self, Dataset, Hdf5, selection};
use crate::files::netcdf::{Item, SubscriptFailure};
use crate::files::{ErrorKind, OsFailure};
use crate::memory;
use crate::panic::panic;

/// `h5py.File(path)`: writes to `out` a new `list[str]` of the lines that describe the HDF5
/// file whose path is the `length` bytes at `path`: `P`, a tab and the path, which the library
/// keeps to read datasets later, then the lines `files::hdf5::describe` writes, one for each
/// group, dataset and attribute. Returns `true`, with the error at `error`, when the operating
/// system refuses, or with the kind `INVALID_DATA` when the file is not HDF5, is damaged, or
/// holds a structure the reader does not read yet.
///
/// Cost: every object header is read once; no dataset's values are.
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
/// See [`baryon_rt_hdf5_read`], which reads the same file.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_hdf5_open(
    path: *const u8,
    length: usize,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let result = hdf5::read_file(path);
    // SAFETY: the caller guarantees `out` is writable.
    let write = |file: Hdf5| unsafe {
        let mut lines = vec![format!("P\t{path}")];
        lines.extend(hdf5::describe(&file));
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        write_list(&lines, out);
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// Writes to `out` a new `list[str]` with one line for each deflated chunk that holds a value
/// `items` pick of the dataset whose path in the file is the `name_length` bytes at `name`, in
/// the HDF5 file whose path is the `length` bytes at `path`: the chunk's address and its size
/// in bytes, separated by a tab, in the order [`baryon_rt_hdf5_read`] takes them inflated.
/// `items` are as [`baryon_rt_hdf5_read`] takes them. A dataset with no deflated chunk gives an
/// empty list. Returns `true`, with the error at `error`, as [`baryon_rt_hdf5_open`] does, with
/// the kind `INVALID_DATA` when the file has no such dataset, or with h5py's `IndexError` or
/// `ValueError` for a subscript that picks nothing, as [`baryon_rt_hdf5_read`] raises them.
///
/// Cost: the file's object headers and the dataset's chunk index.
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
/// See [`baryon_rt_hdf5_read`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_hdf5_chunks(
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
    let result = hdf5::read_file(path)
        .map_err(SubscriptFailure::Os)
        .and_then(|file| {
            let dataset = find(path, &file, name)?;
            hdf5::deflated_chunks(path, &file, (name, dataset, &items))
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

/// `f[name][items]`: reads the values `items` pick of the dataset whose path in the file is the
/// `name_length` bytes at `name`, in the HDF5 file whose path is the `length` bytes at `path`,
/// into a new row-major array of `rank` dimensions and elements of the type numbered
/// `element`, and writes its header to `out`. `items` points to `item_count` items of the
/// subscript, four 64-bit words each, as the `NetCDF` functions take them: `0` and an index, or
/// `1` and a slice's start, stop and step, `i64::MIN` for a part left out. No items read every
/// value, as `f[name][...]` does. An index drops its axis and a slice keeps it, as in h5py,
/// which takes no negative step. `inflated` is the header of an `Array[u8, 1]`
/// holding the dataset's deflated chunks inflated, one after another in the order
/// [`baryon_rt_hdf5_chunks`] lists them, and `sizes` the header of an `Array[i64, 1]` of their
/// sizes; both are empty for a dataset with no deflated chunk. Elements never written read as
/// the dataset's fill value. Returns `true`, with the error at `error`, when the operating
/// system refuses, with the kind `INVALID_DATA` when the file has no such dataset, what is
/// picked has another rank or its values do not read as the type, a chunk's checksum does not
/// match, the inflated chunks run out, or a chunk passed through a filter the reader does not
/// reverse; or with h5py's `IndexError` for an index past its axis, or `ValueError` for more
/// items than axes or a step that is not positive, with h5py's messages.
///
/// Cost: the file's object headers, one allocation, and one pass over the stored values
/// picked; a chunked dataset reads only the chunks that hold a value picked, each once.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type, which the
/// checker never passes, or when the dataset does not fit in memory.
///
/// # Safety
///
/// `path` points to `length` and `name` to `name_length` readable bytes of UTF-8; `items` to
/// `4 * item_count` readable 64-bit words, or `item_count` is 0; `inflated` and `sizes` to
/// readable rank-1 array headers whose elements are readable and unchanged during the call;
/// `out` to `1 + 2 * rank` writable 64-bit words; and `error` to a writable [`RaisedError`]
/// that is a `Raises[None, OSError | IndexError | ValueError]` whenever `item_count` is not 0.
///
/// # Example
///
/// ```
/// use runtime::{ELEMENT_F32, baryon_rt_deallocate, baryon_rt_hdf5_read};
/// use std::mem::MaybeUninit;
///
/// // A file h5py wrote, with a dataset `run1/temp` of 2 by 3 by 4 `f4` values from 270.5.
/// let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../driver/tests/fixtures/hdf5/latest.h5");
/// let name = "run1/temp";
/// // `f["run1/temp"][1, ::2]`: an index, then a slice with only a step.
/// let items = [0, 1, i64::MIN, i64::MIN, 1, i64::MIN, i64::MIN, 2];
/// // No deflated chunks: two empty arrays.
/// let empty = [8_i64, 0, 1];
/// let mut loaded = [0_i64; 5];
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the name are the strings' bytes, `items` holds two items, the empty
/// // arrays are never read, and `loaded` and `error` are locals. The values read are freed
/// // once.
/// unsafe {
///     let failed = baryon_rt_hdf5_read(
///         path.as_ptr(), path.len(), name.as_ptr(), name.len(), items.as_ptr(), 2,
///         empty.as_ptr(), empty.as_ptr(), ELEMENT_F32, 2, loaded.as_mut_ptr(),
///         error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     // Rows 0 and 2 of the second block, four values each.
///     assert_eq!(loaded[1..], [2, 4, 4, 1]);
///     let data = loaded[0] as *mut u8;
///     assert_eq!(data.cast::<f32>().read(), 282.5);
///     baryon_rt_deallocate(data, 8 * 4, 4);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_hdf5_read(
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
    let sizes: Vec<usize> = unsafe { gather(sizes, 8) }
        .chunks_exact(8)
        .map(|word| {
            let word = i64::from_ne_bytes(word.try_into().unwrap_or_default());
            usize::try_from(word).unwrap_or(usize::MAX)
        })
        .collect();
    let result = hdf5::read_file(path)
        .map_err(SubscriptFailure::Os)
        .and_then(|file| {
            read(
                path,
                &file,
                (name, &items),
                (&inflated, &sizes),
                pixel,
                rank,
            )
        });
    // SAFETY: the caller guarantees `1 + 2 * rank` writable words at `out`, and `read` gives a
    // shape `rank` long.
    let write = |(data, shape): (*mut u8, Vec<usize>)| unsafe { write_header(out, data, &shape) };
    // SAFETY: the caller's promises about `error` and its error types, passed on.
    unsafe { finish_subscript(result, write, error) }
}

/// The elements of the rank-1 array whose header is at `array`, `size` bytes each, in order.
///
/// # Safety
///
/// `array` points to a readable rank-1 array header whose elements are readable.
#[allow(unsafe_code, reason = "reads an array compiled code owns")]
unsafe fn gather(array: *const i64, size: usize) -> Vec<u8> {
    // SAFETY: the caller guarantees a header at `array`, whose first word is the pointer.
    let data = unsafe { array.cast::<*const u8>().read() };
    // SAFETY: its length follows the pointer.
    let lengths = unsafe { std::slice::from_raw_parts(array.wrapping_add(1), 1) };
    // SAFETY: and its stride follows the length.
    let strides = unsafe { std::slice::from_raw_parts(array.wrapping_add(2), 1) };
    let mut bytes = Vec::new();
    // SAFETY: the caller's promise about the elements, passed on; writing to a `Vec` cannot
    // fail, so the result is always `Ok`.
    let _ = unsafe { runs(&mut bytes, data, lengths, strides, size) };
    bytes
}

/// The dataset `name` of `file`, the file at `path`.
fn find<'a>(path: &str, file: &'a Hdf5, name: &str) -> Result<&'a Dataset, SubscriptFailure> {
    file.find(name)
        .and_then(|object| object.dataset.as_ref())
        .ok_or_else(|| {
            SubscriptFailure::Os(OsFailure::new(
                ErrorKind::InvalidData,
                format!("the HDF5 file has no dataset {name}"),
                path,
            ))
        })
}

/// Reads the values `items` pick of the dataset `name` of `file`, the file at `path`, into new
/// memory as `pixel`s, the deflated chunks taken from `inflated` in pieces of `sizes`: the
/// memory and the shape. The memory is freed again when the read fails.
#[allow(unsafe_code, reason = "reads into memory it allocates")]
fn read(
    path: &str,
    file: &Hdf5,
    (name, items): (&str, &[Item]),
    (inflated, sizes): (&[u8], &[usize]),
    pixel: Pixel,
    rank: usize,
) -> Result<(*mut u8, Vec<usize>), SubscriptFailure> {
    let dataset = find(path, file, name)?;
    let axes = selection::select(items, &dataset.shape)?;
    // A damaged file's shape can claim more values than memory has bytes: refused here, before
    // anything is allocated, rather than ending the program.
    let too_many = selection::count(&axes)
        .and_then(|count| count.checked_mul(pixel.size() as u64))
        .is_none_or(|bytes| bytes > isize::MAX as u64);
    if too_many {
        return Err(SubscriptFailure::Os(OsFailure::new(
            ErrorKind::InvalidData,
            format!("the HDF5 dataset {name} picks more values than an array can hold"),
            path,
        )));
    }
    let picked = selection::shape(&axes);
    if picked.len() != rank {
        let what = if items.is_empty() {
            format!("the HDF5 dataset {name} has")
        } else {
            format!("the values picked of the HDF5 dataset {name} have")
        };
        return Err(SubscriptFailure::Os(OsFailure::new(
            ErrorKind::InvalidData,
            format!(
                "{what} rank {}, not rank {rank} as the program asks",
                picked.len()
            ),
            path,
        )));
    }
    let shape: Vec<usize> = picked
        .iter()
        .map(|&length| usize::try_from(length).unwrap_or(usize::MAX))
        .collect();
    let count = shape.iter().fold(1_i64, |count, &length| {
        count.saturating_mul(i64::try_from(length).unwrap_or(i64::MAX))
    });
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
    let (mut at, mut next) = (0_usize, 0_usize);
    let mut inflate = |_: &[u8]| -> Result<Vec<u8>, String> {
        let size = *sizes
            .get(next)
            .ok_or_else(|| "the inflated chunks ran out".to_owned())?;
        let end = at
            .checked_add(size)
            .filter(|&end| end <= inflated.len())
            .ok_or_else(|| "the inflated chunks ran out".to_owned())?;
        let chunk = inflated[at..end].to_vec();
        (at, next) = (end, next + 1);
        Ok(chunk)
    };
    if let Err(failure) = hdf5::read_dataset(
        path,
        file,
        (name, dataset, items),
        pixel,
        memory,
        &mut inflate,
    ) {
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
