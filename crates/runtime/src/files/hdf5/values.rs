//! A dataset's values: read from where its layout puts them, passed back through its filters,
//! and converted into an array of the element type the program asks for.

use super::chunks::{Grid, chunks};
use super::cursor::{Failure, Source, UNDEFINED};
use super::{Chunk, Dataset, Datatype, Filter, Hdf5, Layout};
use crate::files::fits::Pixel;
use crate::files::{BUFFER, ErrorKind, OsFailure};

/// Inflates a deflate (zlib) stream: the standard library's native `zlib.decompress` (decision
/// 0049), which the runtime calls back rather than holding a second decoder.
pub(crate) type Inflate<'a> = dyn FnMut(&[u8]) -> Result<Vec<u8>, String> + 'a;

/// Reads every value of the dataset `name` of `file`, the file at `path`, into `memory` as
/// elements of `pixel`, row-major, after checking that `pixel` can hold them. Elements never
/// written read as the dataset's fill value. A deflated chunk is inflated by `inflate`.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when `pixel` cannot hold the
/// values, `memory` is not the dataset's size, the file is damaged, or a chunk passed through
/// a filter the reader does not reverse.
pub(crate) fn read_dataset(
    path: &str,
    file: &Hdf5,
    (name, dataset): (&str, &Dataset),
    pixel: Pixel,
    memory: &mut [u8],
    inflate: &mut Inflate<'_>,
) -> Result<(), OsFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    if !fits(dataset.datatype, pixel) {
        return Err(invalid(format!(
            "the HDF5 dataset {name} holds {}, which does not read as `{}`",
            dataset.datatype.name(),
            pixel.name()
        )));
    }
    let count = dataset.shape.iter().product::<u64>();
    let width = pixel.size();
    if u64::try_from(memory.len() / width).ok() != Some(count)
        || !memory.len().is_multiple_of(width)
    {
        return Err(invalid(format!(
            "the HDF5 dataset {name} holds {count} values, which the array does not match"
        )));
    }
    let mut source = file.source(path)?;
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
    read_values(&mut source, dataset, pixel, memory, inflate)
        .map_err(|failure| invalid(format!("{failure}, in the HDF5 dataset {name}")))
}

/// Reads the stored values of `dataset` from `source` over the fill values already in
/// `memory`.
fn read_values(
    source: &mut Source,
    dataset: &Dataset,
    pixel: Pixel,
    memory: &mut [u8],
    inflate: &mut Inflate<'_>,
) -> Result<(), Failure> {
    let width = pixel.size();
    let stored = usize::try_from(dataset.datatype.size(source)).unwrap_or(usize::MAX);
    match &dataset.layout {
        Layout::Compact(bytes) => {
            place_all(bytes, dataset.datatype, pixel, memory, stored);
            Ok(())
        }
        Layout::Contiguous { address, .. } if *address == UNDEFINED => Ok(()),
        Layout::Contiguous { address, .. } => {
            let mut done = 0_usize;
            let mut at = *address;
            let total = memory.len() / width;
            let per = (BUFFER / stored).max(1);
            while done < total {
                let take = per.min(total - done);
                let bytes = source.bytes(at, (take * stored) as u64)?;
                place_all(
                    &bytes,
                    dataset.datatype,
                    pixel,
                    &mut memory[done * width..],
                    stored,
                );
                done += take;
                at += (take * stored) as u64;
            }
            Ok(())
        }
        Layout::Chunked { chunk, index } => {
            let grid = Grid {
                shape: &dataset.shape,
                most: &dataset.most,
                chunk,
                element: stored as u64,
                filtered: !dataset.filters.is_empty(),
            };
            let found = chunks(source, index, &grid)?;
            for piece in found {
                let raw = source.bytes(piece.address, piece.size)?;
                let bytes = unfilter(raw, &dataset.filters, piece.mask, stored, inflate)?;
                place_chunk(&bytes, &piece, &grid, (dataset.datatype, pixel), memory)?;
            }
            Ok(())
        }
    }
}

/// The address and stored size of each chunk of the dataset `name` of `file`, the file at
/// `path`, that passed through deflate, in the order [`read_dataset`] hands them to its
/// inflater: the chunks a caller inflates for it.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the chunk index is
/// damaged.
pub(crate) fn deflated_chunks(
    path: &str,
    file: &Hdf5,
    (name, dataset): (&str, &Dataset),
) -> Result<Vec<(u64, u64)>, OsFailure> {
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
fn place_all(bytes: &[u8], datatype: Datatype, pixel: Pixel, memory: &mut [u8], stored: usize) {
    for (element, target) in bytes
        .chunks_exact(stored)
        .zip(memory.chunks_exact_mut(pixel.size()))
    {
        convert(element, datatype, pixel, target);
    }
}

/// Copies the part of the chunk `piece`, whose unfiltered values are `bytes`, that lies inside
/// the dataset into its place in `memory`.
fn place_chunk(
    bytes: &[u8],
    piece: &Chunk,
    grid: &Grid,
    (datatype, pixel): (Datatype, Pixel),
    memory: &mut [u8],
) -> Result<(), Failure> {
    let stored = usize::try_from(grid.element).unwrap_or(usize::MAX);
    let needed = grid.chunk.iter().product::<u64>() * grid.element;
    if (bytes.len() as u64) < needed || piece.offset.len() != grid.shape.len() {
        return Err(format!("the chunk at {} is cut short", piece.address));
    }
    let rank = grid.shape.len();
    let Some(last) = rank.checked_sub(1) else {
        place_all(bytes, datatype, pixel, memory, stored);
        return Ok(());
    };
    if piece
        .offset
        .iter()
        .zip(grid.shape)
        .any(|(&at, &length)| at >= length)
    {
        return Ok(());
    }
    let run = grid.chunk[last].min(grid.shape[last] - piece.offset[last]);
    let rows: u64 = grid.chunk[..last].iter().product();
    let width = pixel.size();
    for row in 0..rows {
        // The row's position inside the chunk, slowest axis first.
        let mut rest = row;
        let mut inside = vec![0; last];
        for axis in (0..last).rev() {
            inside[axis] = rest % grid.chunk[axis];
            rest /= grid.chunk[axis];
        }
        let global: Vec<u64> = inside
            .iter()
            .zip(&piece.offset)
            .map(|(&inside, &offset)| inside + offset)
            .collect();
        if global
            .iter()
            .zip(grid.shape)
            .any(|(&at, &length)| at >= length)
        {
            continue;
        }
        let target = global
            .iter()
            .chain(std::iter::once(&piece.offset[last]))
            .zip(grid.shape)
            .fold(0, |linear, (&at, &length)| linear * length + at);
        let from = usize::try_from(row * grid.chunk[last]).unwrap_or(usize::MAX) * stored;
        let to = usize::try_from(target).unwrap_or(usize::MAX) * width;
        let count = usize::try_from(run).unwrap_or(usize::MAX);
        place_all(
            &bytes[from..from + count * stored],
            datatype,
            pixel,
            &mut memory[to..to + count * width],
            stored,
        );
    }
    Ok(())
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
