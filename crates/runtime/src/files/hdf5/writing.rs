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
use encode::Element;
use header::to_usize;
use objects::{Kind, Writer};
use std::fs;
use std::io::{Seek, SeekFrom, Write};

mod attributes;
mod btree;
mod condense;
mod encode;
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
        let block = apply(&mut writer, line).map_err(invalid)?;
        if number + 1 == log.len() {
            new = block;
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
    let raw: Vec<(u64, u64)> = writer
        .objects
        .iter()
        .filter_map(|object| match object.kind {
            Kind::Contiguous(address, size) if size > 0 => Some((address, size)),
            _ => None,
        })
        .collect();
    let (end, blocks) = writer.encode();
    save(
        path,
        (end, blocks),
        &raw,
        new.map(|(address, _)| (address, values)),
    )
    .map_err(|error| OsFailure::from_io(&error, path))
}

/// Applies one line of the log to `writer`. Returns the address and size of a new dataset's
/// values.
fn apply(writer: &mut Writer, line: &str) -> Result<Option<(u64, u64)>, String> {
    let fields: Vec<&str> = line.splitn(5, '\t').collect();
    match fields[..] {
        ["G", path] => writer.create_group(path).map(|()| None),
        ["D", path, element, shape] => {
            let element = Element::parse(element).ok_or("an element type")?;
            writer
                .create_dataset(path, element, &lengths(shape)?)
                .map(Some)
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
/// except the values in `raw`, which are left as they are, and `new` values at their address.
fn save(
    path: &str,
    (end, mut blocks): (u64, Vec<(u64, Vec<u8>)>),
    raw: &[(u64, u64)],
    new: Option<(u64, &[u8])>,
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
    if let Some((address, values)) = new {
        file.seek(SeekFrom::Start(address))?;
        file.write_all(values)?;
    }
    file.set_len(end)?;
    file.sync_all()
}

#[cfg(test)]
mod tests;
