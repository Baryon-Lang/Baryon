//! The functions compiled code calls for array files (decision 0036, part 4):
//! `Array.fromfile(path, dtype)` and `a.tofile(path)`.
//!
//! Both can fail with `OSError`, and raise as the text file functions do (the `files` module):
//! the last parameter is the address of the caller's `Raises[None, OSError]`, and the result is
//! `true` when the call failed and wrote its error there.
//!
//! An `Array[T, rank]` is its header, as compiled code lays it out: a pointer to the elements,
//! then `rank` lengths, then `rank` strides in elements, each 64 bits. A read builds the header
//! of a new rank-1 array ([`ArrayHeader`]); a write is given the address of any array's header.

use super::files::{RaisedError, finish};
use super::strings::text;
use crate::files::arrays::{self, Element};
use crate::memory;
use crate::panic::panic;
use std::io::{self, Write};

/// The header of an `Array[T, 1]`, as compiled code lays it out.
#[repr(C)]
#[derive(Debug)]
pub struct ArrayHeader {
    /// The elements, freed with [`baryon_rt_deallocate`](crate::baryon_rt_deallocate).
    pub data: *mut u8,
    /// How many elements there are.
    pub length: i64,
    /// The distance between elements, in elements: 1 for a new array.
    pub stride: i64,
}

/// What a read needs to know about the array's element type, which the compiler works out.
#[repr(C)]
#[derive(Debug)]
pub struct ElementLayout {
    /// The size of an element in bytes, padding included.
    pub size: usize,
    /// The alignment of an element in bytes, a power of two.
    pub align: usize,
    /// How many `bool`s an element holds.
    pub bool_count: usize,
    /// The offset in bytes of each `bool` in an element: `bool_count` of them, or any pointer
    /// when there are none.
    pub bools: *const usize,
}

/// `Array.fromfile(path, dtype, count, offset)`: reads elements of the layout at `element` from
/// the file whose path is the `length` bytes at `path`, and writes the header of a new rank-1
/// array holding them to `*out`. It skips `offset` bytes and reads `count` elements, or all that
/// follow when `count` is negative. A byte read where the element has a `bool` becomes 1 unless
/// it is 0. Returns `true`, with the error at `error`, when the operating system refuses, when
/// the bytes are not a whole number of elements, or when there are fewer than `count` elements.
///
/// Cost: one allocation and one pass over the bytes, read straight into the array.
///
/// # Panics
///
/// Ends the program with a panic when the array does not fit in memory, as any allocation does.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8; `element` points to a readable
/// [`ElementLayout`] whose `bools` points to `bool_count` readable offsets, each below `size`;
/// `out` points to a writable [`ArrayHeader`] and `error` to a writable [`RaisedError`].
///
/// # Example
///
/// ```
/// use runtime::{ArrayHeader, ElementLayout, baryon_rt_array_from_file, baryon_rt_deallocate};
/// use std::mem::MaybeUninit;
///
/// let path = std::env::temp_dir().join(format!("baryon-doc-fromfile-{}", std::process::id()));
/// std::fs::write(&path, [1.5_f64.to_le_bytes(), 2.5_f64.to_le_bytes()].concat()).unwrap();
/// let path = path.to_str().unwrap();
/// let element = ElementLayout { size: 8, align: 8, bool_count: 0, bools: std::ptr::null() };
/// let mut array = ArrayHeader { data: std::ptr::null_mut(), length: 0, stride: 0 };
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path is the string's bytes, the layout is an `f64`'s, and `array` and `error`
/// // are locals. The elements are read as the `f64`s the file holds and freed once.
/// unsafe {
///     let failed = baryon_rt_array_from_file(
///         path.as_ptr(), path.len(), &element, -1, 0, &mut array, error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     assert_eq!((array.length, array.stride), (2, 1));
///     assert_eq!(array.data.cast::<f64>().add(1).read(), 2.5);
///     baryon_rt_deallocate(array.data, 16, 8);
/// }
/// std::fs::remove_file(path).unwrap();
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_array_from_file(
    path: *const u8,
    length: usize,
    element: *const ElementLayout,
    count: i64,
    offset: i64,
    out: *mut ArrayHeader,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees a readable layout at `element`.
    let layout = unsafe { &*element };
    let bools = if layout.bool_count == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `bool_count` readable offsets at `bools`.
        unsafe { std::slice::from_raw_parts(layout.bools, layout.bool_count) }
    };
    let described = Element {
        size: layout.size,
        bools,
    };
    let mut allocated = None;
    let allocate = |elements: usize| {
        let count = i64::try_from(elements).unwrap_or(i64::MAX);
        let memory = memory::array_layout(count, layout.size, layout.align)
            .and_then(|array| Ok((memory::allocate_zeroed(array)?, array)));
        let (pointer, array) = match memory {
            Ok(memory) => memory,
            Err(failure) => panic(failure, None),
        };
        allocated = Some((pointer, array));
        // SAFETY: the allocation holds `array.size()` zeroed bytes that nothing else refers to;
        // an empty one is an aligned pointer that is never read through.
        unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), array.size()) }
    };
    let result = arrays::read(path, described, count, offset, allocate);
    let data = match (allocated, &result) {
        (Some((pointer, _)), Ok(_)) => pointer.as_ptr(),
        (Some((pointer, array)), Err(_)) => {
            // The read failed after the array was allocated, as when the disk fails.
            // SAFETY: `pointer` came from `allocate_zeroed` with this layout just above.
            unsafe { memory::deallocate(pointer, array) };
            std::ptr::null_mut()
        }
        (None, _) => std::ptr::null_mut(),
    };
    let write = |elements: usize| {
        let header = ArrayHeader {
            data,
            length: i64::try_from(elements).unwrap_or(i64::MAX),
            stride: 1,
        };
        // SAFETY: the caller guarantees `out` is writable.
        unsafe { out.write(header) };
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `a.tofile(path)`: writes every element of the array whose header is at `array`, of `rank`
/// dimensions and elements of `element_size` bytes, to the file whose path is the `length` bytes
/// at `path`, making the file or emptying the one there. The elements are written as they lie in
/// memory, in row-major order. Returns `true`, with the error at `error`, when the operating
/// system refuses or a write fails.
///
/// Cost: one pass over the bytes. An array that is not a view is one write; a view is written
/// one run of neighbouring elements at a time, through a 64 KiB buffer.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8; `array` points to a readable array header
/// of `rank` dimensions, 8 bytes each for the pointer, every length and every stride; every element its lengths and strides reach is `element_size` readable
/// bytes that nothing writes during the call; `error` points to a writable [`RaisedError`].
///
/// # Example
///
/// ```
/// use runtime::{ArrayHeader, baryon_rt_array_to_file};
/// use std::mem::MaybeUninit;
///
/// let path = std::env::temp_dir().join(format!("baryon-doc-tofile-{}", std::process::id()));
/// let mut values = [1.5_f64, 2.5, 3.5, 4.5];
/// // Every second element: a view with a stride of 2.
/// let array = ArrayHeader { data: values.as_mut_ptr().cast(), length: 2, stride: 2 };
/// let mut error = MaybeUninit::uninit();
/// let text = path.to_str().unwrap();
/// // SAFETY: the path is the string's bytes, the header describes `values`, and `error` is a
/// // local.
/// let failed = unsafe {
///     baryon_rt_array_to_file(
///         text.as_ptr(), text.len(), (&raw const array).cast(), 1, 8, error.as_mut_ptr(),
///     )
/// };
/// assert!(!failed);
/// let expected = [1.5_f64.to_le_bytes(), 3.5_f64.to_le_bytes()].concat();
/// assert_eq!(std::fs::read(&path).unwrap(), expected);
/// std::fs::remove_file(path).unwrap();
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_array_to_file(
    path: *const u8,
    length: usize,
    array: *const i64,
    rank: usize,
    element_size: usize,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    // SAFETY: the caller guarantees a header at `array`, whose first word is the pointer.
    let data = unsafe { array.cast::<*const u8>().read() };
    // SAFETY: the header's `rank` lengths follow the pointer.
    let lengths = unsafe { std::slice::from_raw_parts(array.wrapping_add(1), rank) };
    // SAFETY: and its `rank` strides follow the lengths.
    let strides = unsafe { std::slice::from_raw_parts(array.wrapping_add(1 + rank), rank) };
    let result = arrays::write(path, |writer| {
        // SAFETY: the caller's promise about the elements, passed on.
        unsafe { runs(writer, data, lengths, strides, element_size) }
    });
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
}

/// Writes the elements of an array to `writer` in row-major order, as few runs of neighbouring
/// elements as its strides allow: the whole array at once unless it is a view that skips
/// elements, then one write for each stretch of the innermost dimensions that lies together.
///
/// # Safety
///
/// `data`, `lengths` and `strides` describe an array whose every element is `element_size`
/// readable bytes that nothing writes during the call.
#[allow(
    unsafe_code,
    reason = "reads the elements of an array compiled code owns"
)]
unsafe fn runs(
    writer: &mut dyn Write,
    data: *const u8,
    lengths: &[i64],
    strides: &[i64],
    element_size: usize,
) -> io::Result<()> {
    let lengths: Vec<usize> = lengths
        .iter()
        .map(|&length| usize::try_from(length).unwrap_or(0))
        .collect();
    if element_size == 0 || lengths.contains(&0) {
        return Ok(());
    }
    // The dimensions from `outer` on lie together in memory: each one's stride is the number of
    // elements in the dimensions after it. `run` counts the elements of one such stretch.
    let mut outer = lengths.len();
    let mut run = 1_usize;
    while outer > 0 && usize::try_from(strides[outer - 1]) == Ok(run) {
        run *= lengths[outer - 1];
        outer -= 1;
    }
    let mut index = vec![0_usize; outer];
    loop {
        let offset: isize = index
            .iter()
            .zip(strides)
            .map(|(&at, &stride)| at.cast_signed() * isize::try_from(stride).unwrap_or(0))
            .sum();
        let first = data.wrapping_offset(offset * element_size.cast_signed());
        // SAFETY: `index` is within the lengths, so the caller guarantees `run` elements of
        // `element_size` readable bytes from this element on.
        let bytes = unsafe { std::slice::from_raw_parts(first, run * element_size) };
        writer.write_all(bytes)?;
        // The next index in row-major order: the last dimension moves fastest.
        let mut dimension = outer;
        loop {
            if dimension == 0 {
                return Ok(());
            }
            dimension -= 1;
            index[dimension] += 1;
            if index[dimension] < lengths[dimension] {
                break;
            }
            index[dimension] = 0;
        }
    }
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
