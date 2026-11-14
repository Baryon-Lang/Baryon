//! Writing HDF5 files as h5py 3.16 writes them by default (decision 0052): the same calls give
//! the same bytes. h5py's default is the "earliest" layout, which every HDF5 library since
//! 1.8 reads: a version 0 superblock, version 1 object headers, symbol table groups and
//! version 1 B-trees.
//!
//! Matching h5py byte for byte means placing every structure where the HDF5 library places it,
//! and the library's choices depend on everything done to the file since it was created: how
//! its allocator handed out and took back space, which object headers grew. So a file is
//! described by its *log*, a list of lines, one per change, which the standard library keeps
//! in the `h5by.File` and passes with each change. [`write()`] replays the whole log through an
//! emulation of the library (the submodules), closes the file as the library closes it, and
//! writes every structure again, with the new dataset's values. Values written earlier stay
//! where they are, untouched; everything else in the file is rewritten, zeros included. After
//! each change the file on disk is complete and closed, so nothing is lost when a program ends
//! without closing it.
//!
//! The log's lines, fields separated by tabs:
//!
//! - `G`, a path: `create_group(path)`;
//! - `D`, a path, the element type as `NumPy` names it (`<f8`), and the lengths separated by
//!   commas, none for a single value: `create_dataset(path, data=values)`, stored in one block;
//! - the same with two more fields, the chunk's lengths, none to take h5py's guess, and the
//!   filters separated by commas, `shuffle` and `fletcher32`, possibly none:
//!   `create_dataset(path, data=values, chunks=..., shuffle=..., fletcher32=...)`, stored in
//!   chunks ([`chunked`]);
//! - `A`, the object's path, the attribute's name, then `str` and the text, the rest of the
//!   line, or the element type, the lengths and the values' bytes in hexadecimal:
//!   `attrs[name] = value`.
//!
//! A path's names are separated by `/`; the root group's path is empty. Groups on the way to a
//! new name that are not there yet are made first, as h5py makes them.
//!
//! Cost: each change replays the whole log, which builds only metadata, a few hundred bytes
//! per object, and writes the file's metadata and free space again. A program that makes `n`
//! objects does work in proportion to `n²`, small next to its values for any real dataset.

use super::OsFailure;
use crate::files::ErrorKind;
use attributes::Value;
use chunked::Chunked;
use encode::Element;
use filters::Filters;
use header::to_usize;
use objects::{Kind, Writer};
use std::borrow::Cow;
use std::fs;
use std::io::{Seek, SeekFrom, Write};

mod attributes;
mod btree;
mod chunked;
mod chunks;
mod condense;
mod encode;
mod filters;
mod groups;
mod header;
mod objects;
mod space;

/// Applies the change that is the last line of `log` to the file at `path`, whose earlier
/// lines are the changes already made; `values` are the bytes of a new dataset's values,
/// little-endian in C order, and empty for any other change. An empty `log` makes a new,
/// empty file, replacing any there.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when a line is not one the
/// module writes, the change is one h5py refuses, with h5py's message where it has one, or
/// `values` does not fill the new dataset.
pub(crate) fn write(path: &str, log: &[&str], values: &[u8]) -> Result<(), OsFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, &message, path);
    let mut writer = Writer::new();
    let mut new = None;
    for (number, line) in log.iter().enumerate() {
        let made = apply(&mut writer, line).map_err(invalid)?;
        if number + 1 == log.len() {
            new = made;
        }
    }
    if let Some((_, size)) = new
        && size != values.len() as u64
    {
        return Err(invalid(format!(
            "the dataset holds {size} bytes of values, not {}",
            values.len()
        )));
    }
    let placed = new.map_or_else(Vec::new, |(index, _)| {
        place(&writer.objects[index].kind, values)
    });
    let raw = writer.raw();
    let (end, blocks) = writer.encode();
    save(path, (end, blocks), &raw, &placed).map_err(|error| OsFailure::from_io(&error, path))
}

/// Where a new dataset's `values` go: one block, or each chunk as it is stored.
fn place<'a>(kind: &Kind, values: &'a [u8]) -> Vec<(u64, Cow<'a, [u8]>)> {
    match kind {
        Kind::Contiguous(address, size) if *size > 0 => vec![(*address, Cow::Borrowed(values))],
        Kind::Chunked(chunked) => chunked
            .chunks(values)
            .into_iter()
            .map(|(address, bytes)| (address, Cow::Owned(bytes)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Applies one line of the log to `writer`. Returns a new dataset's index and the size of
/// its values in bytes.
fn apply(writer: &mut Writer, line: &str) -> Result<Option<(usize, u64)>, String> {
    let fields: Vec<&str> = line.splitn(5, '\t').collect();
    match fields[..] {
        ["G", path] => writer.create_group(path).map(|()| None),
        ["D", path, element, shape] => dataset(writer, path, element, shape, None).map(Some),
        ["D", path, element, shape, rest] => {
            let (chunk, filters) = rest.split_once('\t').ok_or("a dataset's filters")?;
            dataset(writer, path, element, shape, Some((chunk, filters))).map(Some)
        }
        ["A", path, name, "str", text] => writer
            .set_attribute(path, name, &Value::Text(text.to_owned()))
            .map(|()| None),
        ["A", path, name, element, rest] => {
            let element = Element::parse(element).ok_or("an element type")?;
            let (shape, hex) = rest.split_once('\t').ok_or("an attribute's values")?;
            let value = Value::Numbers(element, lengths(shape)?, from_hex(hex)?);
            writer.set_attribute(path, name, &value).map(|()| None)
        }
        _ => Err(format!("not a line of an HDF5 file's log: {line:?}")),
    }
}

/// Applies a `D` line: its path, element type and shape as the log spells them, and for a
/// chunked dataset the chunk's lengths, none to let h5py's guess pick them, and the filters.
/// Refuses what h5py refuses, with its messages.
fn dataset(
    writer: &mut Writer,
    path: &str,
    element: &str,
    shape: &str,
    chunked: Option<(&str, &str)>,
) -> Result<(usize, u64), String> {
    let element = Element::parse(element).ok_or("an element type")?;
    let shape = lengths(shape)?;
    let Some((chunk, filters)) = chunked else {
        return writer.create_dataset(path, element, &shape, None);
    };
    let filters = Filters::parse(filters)?;
    if shape.is_empty() {
        return Err("Scalar datasets don't support chunk/filter options".to_owned());
    }
    let chunk = if chunk.is_empty() {
        filters::guess(&shape, element.size())
    } else {
        let chunk = lengths(chunk)?;
        if chunk.len() != shape.len() {
            return Err("'chunks' must have same rank as dataset shape".to_owned());
        }
        if chunk
            .iter()
            .zip(&shape)
            .any(|(length, total)| length > total)
        {
            return Err(format!(
                "Chunk shape must not be greater than data shape in any dimension. \
                 {} is not compatible with {}",
                python_tuple(&chunk),
                python_tuple(&shape)
            ));
        }
        if chunk.contains(&0) {
            return Err("All chunk dimensions must be positive \
                        (all chunk dimensions must be positive)"
                .to_owned());
        }
        chunk
    };
    let chunked = Chunked::new(&shape, &chunk, element.size(), filters);
    writer.create_dataset(path, element, &shape, Some(chunked))
}

/// `lengths` as Python prints a tuple of them: `(4,)`, `(2, 3)`.
fn python_tuple(lengths: &[u64]) -> String {
    let items: Vec<String> = lengths.iter().map(u64::to_string).collect();
    if items.len() == 1 {
        format!("({},)", items[0])
    } else {
        format!("({})", items.join(", "))
    }
}

/// The lengths `text` lists, separated by commas.
fn lengths(text: &str) -> Result<Vec<u64>, String> {
    text.split(',')
        .filter(|length| !length.is_empty())
        .map(|length| {
            length
                .parse()
                .map_err(|_| format!("not a length: {length:?}"))
        })
        .collect()
}

/// The bytes the hexadecimal `text` spells.
fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let digits = text.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return Err("an odd number of hexadecimal digits".to_owned());
    }
    digits
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| "not hexadecimal".to_owned())
        })
        .collect()
}

/// Writes the file at `path`: `end` bytes long, the structures `blocks` and zeros everywhere
/// except the values in `raw`, which are left as they are, and the `new` values at their
/// addresses.
fn save(
    path: &str,
    (end, mut blocks): (u64, Vec<(u64, Vec<u8>)>),
    raw: &[(u64, u64)],
    new: &[(u64, Cow<[u8]>)],
) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    blocks.sort_by_key(|block| block.0);
    let mut raw = raw.to_vec();
    raw.sort_unstable();
    let mut at = 0;
    let mut next = 0;
    // Every stretch between values is written whole: its structures, and zeros between them.
    for stretch in raw.iter().copied().chain([(end, 0)]) {
        if stretch.0 > at {
            let mut bytes = vec![0; to_usize(stretch.0 - at)];
            while let Some((address, block)) = blocks.get(next)
                && *address < stretch.0
            {
                let start = to_usize(address - at);
                bytes[start..start + block.len()].copy_from_slice(block);
                next += 1;
            }
            file.seek(SeekFrom::Start(at))?;
            file.write_all(&bytes)?;
        }
        at = at.max(stretch.0 + stretch.1);
    }
    for (address, values) in new {
        file.seek(SeekFrom::Start(*address))?;
        file.write_all(values)?;
    }
    file.set_len(end)?;
    file.sync_all()
}

#[cfg(test)]
mod tests;
