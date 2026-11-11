//! Array files (decision 0036, part 4): `Array.fromfile(path, dtype)` and `a.tofile(path)`, in
//! safe code the exported functions in `abi::array_files` call.
//!
//! An array file is the array's elements and nothing else: each element's bytes as they lie in
//! memory, in row-major order, with no header, as `numpy.fromfile` reads and
//! `numpy.ndarray.tofile` writes. Every platform Baryon supports is little-endian, so a file
//! written on one reads on any other, and in `NumPy`.
//!
//! Cost: one pass over the bytes. A read goes straight into the array's own memory and a write
//! comes straight from it, a system call per large block and no work per element. The two
//! exceptions are an array with `bool`s in its elements, which a read visits once more to make
//! each one 0 or 1, and a file that is not a regular file, such as a pipe, whose length is not
//! known until its end, so it is read into a buffer first and copied.

use super::{BUFFER, EISDIR, ErrorKind, OsFailure};
use std::fs;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};

/// `EINVAL`, the error number Linux and macOS both give for an argument that makes no sense,
/// such as a negative offset.
const EINVAL: i32 = 22;

/// What a read needs to know about one element of the array.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Element<'a> {
    /// The size of an element in bytes, padding included.
    pub(crate) size: usize,
    /// The offset in bytes of every `bool` in an element. A file may hold any byte where the
    /// array holds a `bool`; compiled code reads only 0 and 1, so each is made 1 unless it is 0,
    /// as `NumPy` reads any byte but 0 as `True`.
    pub(crate) bools: &'a [usize],
}

/// Reads elements from the file at `path`, `Array.fromfile(path, dtype, count, offset)`: skips
/// `offset` bytes, then reads `count` elements, or every element to the end of the file when
/// `count` is negative. `allocate` is called once, with the number of elements, and returns the
/// zeroed memory for them, that many times `element.size` bytes, which the read fills. Returns
/// the number of elements.
///
/// # Errors
///
/// Fails as the operating system does (no such file, no permission, a directory), with
/// `INVALID_DATA` when the bytes after `offset` are not a whole number of elements or are fewer
/// than `count` elements (Collins, 2026-11-11: never a shorter array, as `NumPy` gives), and
/// with the system's "Invalid argument" for a negative `offset`.
pub(crate) fn read<'a>(
    path: &str,
    element: Element<'_>,
    count: i64,
    offset: i64,
    allocate: impl FnOnce(usize) -> &'a mut [u8],
) -> Result<usize, OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let Ok(offset) = u64::try_from(offset) else {
        return Err(failed(io::Error::from_raw_os_error(EINVAL)));
    };
    let wanted = usize::try_from(count).ok();
    let mut file = fs::File::open(path).map_err(failed)?;
    let metadata = file.metadata().map_err(failed)?;
    if metadata.is_dir() {
        return Err(failed(io::Error::from_raw_os_error(EISDIR)));
    }
    if element.size == 0 {
        // An element with no bytes: the file has nothing to say about how many there are.
        let elements = wanted.unwrap_or(0);
        allocate(elements);
        return Ok(elements);
    }
    // A regular file says how long it is, so the array is allocated once and read into. Anything
    // else, such as a pipe, is read to its end first.
    let piped = if metadata.is_file() {
        file.seek(SeekFrom::Start(offset)).map_err(failed)?;
        None
    } else {
        io::copy(&mut (&file).take(offset), &mut io::sink()).map_err(failed)?;
        let limit = wanted.map_or(u64::MAX, |wanted| bytes_of(wanted, element.size));
        let mut bytes = Vec::new();
        (&file)
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(failed)?;
        Some(bytes)
    };
    let available = piped.as_ref().map_or_else(
        || metadata.len().saturating_sub(offset),
        |bytes| bytes.len() as u64,
    );
    let elements = elements(available, element.size, wanted)
        .map_err(|message| OsFailure::new(ErrorKind::InvalidData, message, path))?;
    let memory = allocate(elements);
    match piped {
        Some(bytes) => memory.copy_from_slice(&bytes[..memory.len()]),
        None => file.read_exact(memory).map_err(failed)?,
    }
    for item in memory.chunks_exact_mut(element.size) {
        for &at in element.bools {
            item[at] = u8::from(item[at] != 0);
        }
    }
    Ok(elements)
}

/// The size in bytes of `count` elements of `size` bytes, or the largest number when it does not
/// fit, which no file reaches.
fn bytes_of(count: usize, size: usize) -> u64 {
    (count as u64).saturating_mul(size as u64)
}

/// How many elements of `size` bytes to read from a file with `available` bytes left: `wanted`,
/// or all of them when it is `None`. The error is the message of the `INVALID_DATA` failure.
fn elements(available: u64, size: usize, wanted: Option<usize>) -> Result<usize, String> {
    let whole = available / size as u64;
    match wanted {
        Some(wanted) if whole >= wanted as u64 => Ok(wanted),
        Some(wanted) => Err(format!(
            "the file holds {whole} elements of {size} bytes after the offset, fewer than the \
             {wanted} asked for"
        )),
        None if !available.is_multiple_of(size as u64) => Err(format!(
            "the file holds {available} bytes after the offset, not a whole number of elements \
             of {size} bytes"
        )),
        None => usize::try_from(whole).map_err(|_| "the file is too large for memory".to_owned()),
    }
}

/// Writes an array to the file at `path`, `a.tofile(path)`, making the file or emptying the one
/// there. `emit` writes the array's bytes, in row-major order, to the writer it is given; a
/// write of 64 KiB or more goes straight to the operating system, and smaller ones, the rows of
/// a view, are gathered 64 KiB at a time.
///
/// # Errors
///
/// Fails as the operating system does: no permission, a directory, a full disk. A file that
/// failed part way holds what was written before the failure.
pub(crate) fn write(
    path: &str,
    emit: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<(), OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let file = fs::File::create(path).map_err(failed)?;
    let mut writer = BufWriter::with_capacity(BUFFER, file);
    emit(&mut writer).map_err(failed)?;
    writer.flush().map_err(failed)
}

#[cfg(test)]
mod tests;
