//! A dataset's values: read from where its layout puts them, passed back through its filters,
//! and converted into an array of the element type the program asks for.

use super::chunks::{Grid, chunks};
use super::cursor::{Failure, Source, UNDEFINED};
use super::selection::{Axis, count, inside, select};
use super::{Dataset, Datatype, Filter, Hdf5, Layout};
use crate::files::fits::Pixel;
use crate::files::netcdf::{Item, SubscriptFailure};
use crate::files::{BUFFER, ErrorKind, OsFailure};

/// Inflates a deflate (zlib) stream: the standard library's native `zlib.decompress` (decision
/// 0049), which the runtime calls back rather than holding a second decoder.
pub(crate) type Inflate<'a> = dyn FnMut(&[u8]) -> Result<Vec<u8>, String> + 'a;

/// Reads the values `items` pick of the dataset `name` of `file`, the file at `path`, into
/// `memory` as elements of `pixel`, row-major, after checking that `pixel` can hold them; no
/// items read every value. Elements never written read as the dataset's fill value. Only the
/// chunks that hold a value picked are read, and a deflated one is inflated by `inflate`.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when `pixel` cannot hold the
/// values, `memory` is not the size of what is picked, the file is damaged, or a chunk passed
/// through a filter the reader does not reverse; and with h5py's `IndexError` or `ValueError`
/// for a subscript that picks nothing (see [`select`]).
pub(crate) fn read_dataset(
    path: &str,
    file: &Hdf5,
    (name, dataset, items): (&str, &Dataset, &[Item]),
    pixel: Pixel,
    memory: &mut [u8],
    inflate: &mut Inflate<'_>,
) -> Result<(), SubscriptFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    if !fits(dataset.datatype, pixel) {
        return Err(SubscriptFailure::Os(invalid(format!(
            "the HDF5 dataset {name} holds {}, which does not read as `{}`",
            dataset.datatype.name(),
            pixel.name()
        ))));
    }
    let axes = select(items, &dataset.shape)?;
    let width = pixel.size();
    let Some(count) = count(&axes) else {
        return Err(SubscriptFailure::Os(invalid(format!(
            "the HDF5 dataset {name} picks more values than an array can hold"
        ))));
    };
    if u64::try_from(memory.len() / width).ok() != Some(count)
        || !memory.len().is_multiple_of(width)
    {
        return Err(SubscriptFailure::Os(invalid(format!(
            "the HDF5 dataset {name} picks {count} values, which the array does not match"
        ))));
    }
    let mut source = file.source(path)?;
    // At most 8: `fits` let through integers and floats only, whose sizes the datatype message
    // limits to 1, 2, 4 and 8.
    let stored = usize::try_from(dataset.datatype.size(&source)).unwrap_or(usize::MAX);
    let zero = vec![0; stored];
    let blank = dataset
        .fill
        .as_deref()
        .filter(|blank| blank.len() == stored);
    let blank = blank.unwrap_or(&zero);
    for element in memory.chunks_exact_mut(width) {
        convert(blank, dataset.datatype, pixel, element);
    }
    let target = Target {
        axes: &axes,
        datatype: dataset.datatype,
        pixel,
        stored,
    };
    read_values(&mut source, dataset, &target, memory, inflate).map_err(|failure| {
        SubscriptFailure::Os(invalid(format!("{failure}, in the HDF5 dataset {name}")))
    })
}

/// Where values go: the positions picked and the types to convert between.
struct Target<'a> {
    /// The positions picked along each axis.
    axes: &'a [Axis],
    /// The stored type.
    datatype: Datatype,
    /// The array's element type.
    pixel: Pixel,
    /// The size of one stored value in bytes.
    stored: usize,
}

/// Reads the stored values of `dataset` that `target` picks from `source`, over the fill values
/// already in `memory`.
fn read_values(
    source: &mut Source,
    dataset: &Dataset,
    target: &Target,
    memory: &mut [u8],
    inflate: &mut Inflate<'_>,
) -> Result<(), Failure> {
    let stored = target.stored;
    let whole = target
        .axes
        .iter()
        .zip(&dataset.shape)
        .all(|(axis, &length)| axis.count == length);
    match &dataset.layout {
        Layout::Compact(bytes) => {
            place_block(
                bytes,
                (&vec![0; dataset.shape.len()], &dataset.shape),
                target,
                memory,
            );
            Ok(())
        }
        Layout::Contiguous { address, .. } if *address == UNDEFINED => Ok(()),
        Layout::Contiguous { address, .. } if whole => {
            let width = target.pixel.size();
            let mut done = 0_usize;
            let mut at = *address;
            let total = memory.len() / width;
            let per = (BUFFER / stored).max(1);
            while done < total {
                let take = per.min(total - done);
                let bytes = source.bytes(at, (take * stored) as u64)?;
                place_all(
                    &bytes,
                    (target.datatype, target.pixel),
                    &mut memory[done * width..],
                    stored,
                );
                done += take;
                at += (take * stored) as u64;
            }
            Ok(())
        }
        Layout::Contiguous { address, .. } => {
            rows(source, *address, &dataset.shape, target, memory)
        }
        Layout::Chunked { chunk, index } => {
            let grid = Grid {
                shape: &dataset.shape,
                most: &dataset.most,
                chunk,
                element: stored as u64,
                filtered: !dataset.filters.is_empty(),
            };
            for piece in chunks(source, index, &grid)? {
                if inside(target.axes, &piece.offset, chunk).is_none() {
                    continue;
                }
                let raw = source.bytes(piece.address, piece.size)?;
                let bytes = unfilter(raw, &dataset.filters, piece.mask, stored, inflate)?;
                let needed = chunk.iter().product::<u64>() * stored as u64;
                if (bytes.len() as u64) < needed {
                    return Err(format!("the chunk at {} is cut short", piece.address));
                }
                place_block(&bytes, (&piece.offset, chunk), target, memory);
            }
            Ok(())
        }
    }
}

/// Reads the values `target` picks of a contiguous dataset of `shape` stored at `address`, one
/// row at a time: each read spans the positions picked along the last axis of one row.
fn rows(
    source: &mut Source,
    address: u64,
    shape: &[u64],
    target: &Target,
    memory: &mut [u8],
) -> Result<(), Failure> {
    let Some(last) = shape.len().checked_sub(1) else {
        let bytes = source.bytes(address, target.stored as u64)?;
        place_block(&bytes, (&[], &[]), target, memory);
        return Ok(());
    };
    let along = target.axes[last];
    let (first, Some(end)) = (along.first, along.last()) else {
        return Ok(());
    };
    let outer = &target.axes[..last];
    // The caller's array holds every value picked, so the rows fit in a `u64`.
    let rows = count(outer).unwrap_or(0);
    let mut index = vec![0_u64; last];
    for _ in 0..rows {
        let mut origin: Vec<u64> = index
            .iter()
            .zip(outer)
            .map(|(&at, axis)| axis.position(at))
            .collect();
        let row = origin
            .iter()
            .zip(shape)
            .fold(0_u64, |linear, (&at, &length)| linear * length + at);
        let start = (row * shape[last] + first) * target.stored as u64;
        let bytes = source.bytes(address + start, (end - first + 1) * target.stored as u64)?;
        origin.push(first);
        let mut extent = vec![1; last];
        extent.push(end - first + 1);
        place_block(&bytes, (&origin, &extent), target, memory);
        for axis in (0..last).rev() {
            index[axis] += 1;
            if index[axis] < outer[axis].count {
                break;
            }
            index[axis] = 0;
        }
    }
    Ok(())
}

/// Copies the values `target` picks from a block of the dataset that begins at `origin`, spans
/// `extent` along each axis and whose stored values, row-major, are `bytes`, into their places
/// in `memory`.
fn place_block(
    bytes: &[u8],
    (origin, extent): (&[u64], &[u64]),
    target: &Target,
    memory: &mut [u8],
) {
    let Some(found) = inside(target.axes, origin, extent) else {
        return;
    };
    let counts: Vec<u64> = target.axes.iter().map(|axis| axis.count).collect();
    let width = target.pixel.size();
    let total: usize = found.iter().map(Vec::len).product();
    let mut index = vec![0_usize; found.len()];
    for _ in 0..total {
        let (mut out, mut from) = (0_u64, 0_u64);
        for (axis, &at) in index.iter().enumerate() {
            let (result, offset) = found[axis][at];
            out = out * counts[axis] + result;
            from = from * extent[axis] + offset;
        }
        let from = usize::try_from(from).unwrap_or(usize::MAX) * target.stored;
        let out = usize::try_from(out).unwrap_or(usize::MAX) * width;
        if let (Some(element), Some(slot)) = (
            bytes.get(from..from + target.stored),
            memory.get_mut(out..out + width),
        ) {
            convert(element, target.datatype, target.pixel, slot);
        }
        for axis in (0..index.len()).rev() {
            index[axis] += 1;
            if index[axis] < found[axis].len() {
                break;
            }
            index[axis] = 0;
        }
    }
}

/// The address and stored size of each chunk of the dataset `name` of `file`, the file at
/// `path`, that passed through deflate and holds a value `items` pick, in the order
/// [`read_dataset`] hands them to its inflater: the chunks a caller inflates for it.
///
/// # Errors
///
/// Fails as the operating system does, with `INVALID_DATA` when the chunk index is damaged,
/// and as [`read_dataset`] does for a subscript that picks nothing.
pub(crate) fn deflated_chunks(
    path: &str,
    file: &Hdf5,
    (name, dataset, items): (&str, &Dataset, &[Item]),
) -> Result<Vec<(u64, u64)>, SubscriptFailure> {
    let axes = select(items, &dataset.shape)?;
    let Layout::Chunked { chunk, index } = &dataset.layout else {
        return Ok(Vec::new());
    };
    let Some(position) = dataset.filters.iter().position(|filter| filter.id == 1) else {
        return Ok(Vec::new());
    };
    let mut source = file.source(path)?;
    let grid = Grid {
        shape: &dataset.shape,
        most: &dataset.most,
        chunk,
        element: dataset.datatype.size(&source),
        filtered: true,
    };
    let found = chunks(&mut source, index, &grid).map_err(|failure| {
        OsFailure::new(
            ErrorKind::InvalidData,
            format!("{failure}, in the HDF5 dataset {name}"),
            path,
        )
    })?;
    Ok(found
        .iter()
        .filter(|piece| position >= 32 || piece.mask & (1 << position) == 0)
        .filter(|piece| inside(&axes, &piece.offset, chunk).is_some())
        .map(|piece| (piece.address, piece.size))
        .collect())
}

/// Whether an array of `pixel` holds every value of `datatype`: an integer type at least as
/// wide, signed if the stored type is, or any float type for any number.
fn fits(datatype: Datatype, pixel: Pixel) -> bool {
    let (signed_target, width) = match pixel {
        Pixel::F32 | Pixel::F64 => {
            return matches!(datatype, Datatype::Integer { .. } | Datatype::Float { .. });
        }
        Pixel::I8 | Pixel::I16 | Pixel::I32 | Pixel::I64 => (true, pixel.size()),
        Pixel::U8 | Pixel::U16 | Pixel::U32 | Pixel::U64 => (false, pixel.size()),
    };
    let Datatype::Integer { size, signed, .. } = datatype else {
        return false;
    };
    let size = size as usize;
    match (signed, signed_target) {
        (true, true) | (false, false) => size <= width,
        (false, true) => size < width,
        (true, false) => false,
    }
}

/// Converts the elements of `bytes`, each `stored` bytes of `datatype`, into the elements of
/// `memory` in order, as many as both hold.
fn place_all(bytes: &[u8], (datatype, pixel): (Datatype, Pixel), memory: &mut [u8], stored: usize) {
    for (element, target) in bytes
        .chunks_exact(stored)
        .zip(memory.chunks_exact_mut(pixel.size()))
    {
        convert(element, datatype, pixel, target);
    }
}

/// The bytes of a chunk stored as `raw`, passed back through `filters` in reverse, skipping
/// those `mask` marks, for elements of `stored` bytes.
fn unfilter(
    raw: Vec<u8>,
    filters: &[Filter],
    mask: u32,
    stored: usize,
    inflate: &mut Inflate<'_>,
) -> Result<Vec<u8>, Failure> {
    let mut bytes = raw;
    for (position, filter) in filters.iter().enumerate().rev() {
        if position < 32 && mask & (1 << position) != 0 {
            continue;
        }
        bytes = match filter.id {
            1 => inflate(&bytes)?,
            2 => unshuffle(
                &bytes,
                filter
                    .parameters
                    .first()
                    .map_or(stored, |&size| size as usize),
            ),
            3 => fletcher(bytes)?,
            _ => {
                let name = if filter.name.is_empty() {
                    format!("number {}", filter.id)
                } else {
                    filter.name.clone()
                };
                return Err(format!(
                    "the chunks are compressed with the filter {name}, which is not read yet"
                ));
            }
        };
    }
    Ok(bytes)
}

/// Undoes the shuffle filter, which stores the first byte of every element, then every second
/// byte, and so on; bytes past the last whole element are left in place.
fn unshuffle(bytes: &[u8], size: usize) -> Vec<u8> {
    if size <= 1 {
        return bytes.to_vec();
    }
    let count = bytes.len() / size;
    let mut out = bytes.to_vec();
    for byte in 0..size {
        for element in 0..count {
            out[element * size + byte] = bytes[byte * count + element];
        }
    }
    out
}

/// Checks and removes the Fletcher-32 checksum that ends `bytes`. Older libraries stored it
/// with each 16-bit half's bytes swapped, which is accepted too.
fn fletcher(mut bytes: Vec<u8>) -> Result<Vec<u8>, Failure> {
    let Some(split) = bytes.len().checked_sub(4) else {
        return Err("a chunk is too short for its Fletcher-32 checksum".to_owned());
    };
    let stored = [
        bytes[split],
        bytes[split + 1],
        bytes[split + 2],
        bytes[split + 3],
    ];
    bytes.truncate(split);
    let (mut first, mut second) = (0_u32, 0_u32);
    for pair in bytes.chunks(2) {
        let word = (u32::from(pair[0]) << 8) | pair.get(1).map_or(0, |&low| u32::from(low));
        first = (first + word) % 65535;
        second = (second + first) % 65535;
    }
    let sum = ((second << 16) | first).to_le_bytes();
    let swapped = [sum[1], sum[0], sum[3], sum[2]];
    if stored == sum || stored == swapped {
        Ok(bytes)
    } else {
        Err("a chunk's Fletcher-32 checksum does not match".to_owned())
    }
}

/// Converts one stored element of `datatype` into one element of `pixel`.
fn convert(stored: &[u8], datatype: Datatype, pixel: Pixel, element: &mut [u8]) {
    let little = |big: bool, index: usize| {
        if big {
            stored[stored.len() - 1 - index]
        } else {
            stored[index]
        }
    };
    match datatype {
        Datatype::Integer { signed, big, .. } => {
            let negative = signed && little(big, stored.len() - 1) & 0x80 != 0;
            let mut value: i128 = if negative { -1 } else { 0 };
            for index in (0..stored.len().min(16)).rev() {
                value = (value << 8) | i128::from(little(big, index));
            }
            write_integer(value, pixel, element);
        }
        Datatype::Float { big, .. } => {
            let mut bytes = [0_u8; 8];
            for (index, byte) in bytes.iter_mut().enumerate().take(stored.len().min(8)) {
                *byte = little(big, index);
            }
            let value = if stored.len() == 4 {
                f64::from(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            } else {
                f64::from_le_bytes(bytes)
            };
            write_float(value, pixel, element);
        }
        Datatype::Text { .. } | Datatype::VariableText | Datatype::Other { .. } => {}
    }
}

/// Writes the integer `value`, which `pixel` holds, as an element of `pixel`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "the value fits the element type: `fits` checked the stored type against it"
)]
fn write_integer(value: i128, pixel: Pixel, element: &mut [u8]) {
    match pixel {
        Pixel::U8 => element.copy_from_slice(&(value as u8).to_ne_bytes()),
        Pixel::I8 => element.copy_from_slice(&(value as i8).to_ne_bytes()),
        Pixel::I16 => element.copy_from_slice(&(value as i16).to_ne_bytes()),
        Pixel::U16 => element.copy_from_slice(&(value as u16).to_ne_bytes()),
        Pixel::I32 => element.copy_from_slice(&(value as i32).to_ne_bytes()),
        Pixel::U32 => element.copy_from_slice(&(value as u32).to_ne_bytes()),
        Pixel::I64 => element.copy_from_slice(&(value as i64).to_ne_bytes()),
        Pixel::U64 => element.copy_from_slice(&(value as u64).to_ne_bytes()),
        Pixel::F32 => element.copy_from_slice(&(value as f32).to_ne_bytes()),
        Pixel::F64 => element.copy_from_slice(&(value as f64).to_ne_bytes()),
    }
}

/// Writes the float `value` as an element of the float type `pixel`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a float array of `f32` holds a stored `f8` rounded, as NumPy's `astype` does"
)]
fn write_float(value: f64, pixel: Pixel, element: &mut [u8]) {
    match pixel {
        Pixel::F32 => element.copy_from_slice(&(value as f32).to_ne_bytes()),
        Pixel::F64 => element.copy_from_slice(&value.to_ne_bytes()),
        _ => {}
    }
}
