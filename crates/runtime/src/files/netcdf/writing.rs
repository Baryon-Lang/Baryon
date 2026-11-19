//! Writing a `NetCDF` classic file as `netCDF4` writes it (decision 0047). The file is the state:
//! each change a program makes, a dimension, a variable, an attribute or a variable's values,
//! is written to the file at once, laid out as the C library `netCDF4` calls lays it out, so
//! the bytes match `netCDF4`'s for the same calls.
//!
//! The layout: the header, with no free space after it; then each fixed-size variable's values,
//! in the order they were defined, each padded to a multiple of 4 bytes; then the records, each
//! holding one slot of every record variable in the order they were defined. A slot is padded
//! to a multiple of 4 bytes, unless there is only one record variable. Values not written hold
//! the type's fill value, padding included: a write that adds records fills them for every
//! record variable before it writes its values, as the C library does.
//!
//! Cost (row 3.14.7): a definition changes the header's length, which moves every variable, so
//! the `streaming` module copies the file into its new layout through a fixed buffer: one pass
//! over the file and a few megabytes of memory, written beside it and renamed over it. A write
//! of values moves nothing, so it is made in place: only the bytes it changes are written, the
//! records it adds and the header's record count, whatever the file's size. A program should
//! still define everything first and write values after.

use super::encoding::store;
use super::storage::{BUFFER, fill, slot};
use super::streaming::{check, rewrite};
use super::{
    Attribute, Header, Item, Kind, SubscriptFailure, Values, Variable, is_whole, read_header,
    scaling, select,
};
use crate::files::fits::Pixel;
use crate::files::{ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};

/// One change to a file, as a line of text the standard library's `netCDF4` module writes,
/// a tag and fields separated by tabs.
///
/// - `C`, then `1` or `2`: make a new, empty file of that version, replacing any there;
/// - `D`, a dimension's name and its length, `0` for the unlimited one;
/// - `V`, a variable's name, its type as `netCDF4` names it (`f4`), and its dimensions' names
///   separated by commas;
/// - `A`, the variable an attribute belongs to (empty for the file's own), its kind (`str`,
///   `int` or `float`), its name, and its value: the text, the rest of the line, tabs and all,
///   or the number as Python prints it. An `int` is stored as an `NC_INT` and a `float` as an
///   `NC_DOUBLE`, as `netCDF4` stores a Python `int` and `float`; an attribute that is there
///   already is changed in its place.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the file is not a classic
/// `NetCDF` file, the line is not one of these, or the change is one the format refuses, with
/// the message `netCDF4` gives where it has one.
pub(crate) fn define(path: &str, line: &str) -> Result<(), OsFailure> {
    let invalid = |message: &str| OsFailure::new(ErrorKind::InvalidData, message, path);
    let fields: Vec<&str> = line.splitn(5, '\t').collect();
    if let ["C", version] = fields[..] {
        let header = Header {
            wide: version == "2",
            records: 0,
            dimensions: Vec::new(),
            attributes: Vec::new(),
            variables: Vec::new(),
            netcdf4: None,
        };
        return rewrite(path, None, &header);
    }
    let old = classic(path)?;
    let mut header = old.clone();
    match fields[..] {
        ["D", name, length] => {
            let length: u64 = length
                .parse()
                .map_err(|_| invalid("a dimension's length"))?;
            add_dimension(&mut header, name, length).map_err(|message| invalid(&message))?;
        }
        ["V", name, kind, spans] => {
            add_variable(&mut header, name, kind, spans).map_err(|message| invalid(&message))?;
        }
        ["A", owner, kind, name, value] => {
            let attribute = attribute(kind, name, value).map_err(|message| invalid(&message))?;
            set_attribute(&mut header, owner, attribute).map_err(|message| invalid(&message))?;
        }
        _ => return Err(invalid("not a change to a NetCDF file")),
    }
    rewrite(path, Some(&old), &header)
}

/// The header of the classic file at `path`, checked against the file's length.
///
/// # Errors
///
/// As [`read_header`] fails, and with `INVALID_DATA` for a NetCDF-4 file or a header that
/// claims more than the file holds.
fn classic(path: &str) -> Result<Header, OsFailure> {
    let header = read_header(path)?;
    if header.netcdf4.is_some() {
        return Err(OsFailure::new(
            ErrorKind::InvalidData,
            "the file is a NetCDF-4 (HDF5) file, which is not written yet",
            path,
        ));
    }
    let length = fs::metadata(path)
        .map_err(|error| OsFailure::from_io(&error, path))?
        .len();
    check(path, &header, length)?;
    Ok(header)
}

/// Writes `values`, every element of an array of `pixel`s with the lengths `shape`, in
/// row-major order and the machine's byte order, as the values `items` pick of the variable
/// `name` of the file at `path`: all of them when there are no items, as `var[:] = values`
/// writes. The values are converted to the variable's type as `NumPy`'s `astype` converts them
/// and packed by `scale_factor` and `add_offset` as `netCDF4` packs them. Along the unlimited
/// dimension the items may pick records past the last one, and a slice with no end picks as
/// many as the array has rows: the file gains the records, each filled for every record
/// variable, padding and all, before the values are written, as the C library fills them.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the file has no variable
/// `name`, the array's shape differs from the shape of the values picked, or the variable
/// holds text; with the `IndexError` or `ValueError` of a [`Refusal`](super::selection::Refusal)
/// when the items pick no values, as `netCDF4` raises them.
pub(crate) fn write(
    path: &str,
    (name, items): (&str, &[Item]),
    pixel: Pixel,
    shape: &[usize],
    values: &[u8],
) -> Result<(), SubscriptFailure> {
    let invalid = |message: String| {
        SubscriptFailure::Os(OsFailure::new(ErrorKind::InvalidData, message, path))
    };
    let header = &classic(path)?;
    let Some(index) = header
        .variables
        .iter()
        .position(|variable| variable.name == name)
    else {
        return Err(invalid(format!("the NetCDF file has no variable {name}")));
    };
    let variable = &header.variables[index];
    if variable.kind == Kind::Char {
        return Err(invalid(format!(
            "the NetCDF variable {name} holds text, which is not written yet"
        )));
    }
    let record = header.is_record(variable);
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
    let selection = select(name, items, (&names, &lengths), record, Some(shape))
        .map_err(|refusal| refusal.at(path))?;
    let picked = selection.shape();
    if picked != shape {
        let show = |lengths: &[usize]| {
            let lengths: Vec<String> = lengths.iter().map(usize::to_string).collect();
            lengths.join(", ")
        };
        let what = if is_whole(items) {
            format!("the NetCDF variable {name} has")
        } else {
            format!("the values picked of the NetCDF variable {name} have")
        };
        return Err(invalid(format!(
            "{what} the shape ({}), which an array of shape ({}) does not fill",
            show(&picked),
            show(shape)
        )));
    }
    let scale = scaling(variable);
    let kind = variable.kind;
    let stored: Vec<u8> = values
        .chunks_exact(pixel.size())
        .flat_map(|element| store(element, pixel, kind, scale))
        .collect();
    let width = kind.size() as u64;
    let step = header.record_size();
    let inner = lengths[usize::from(record)..].to_vec();
    let needed = if record {
        selection.axes[0]
            .positions
            .iter()
            .max()
            .map_or(0, |&last| last.saturating_add(1))
    } else {
        0
    };
    // Every run's place is worked out before the file is touched, so a write that does not
    // fit is refused with the file as it was.
    let mut runs: Vec<(u64, std::ops::Range<usize>)> = Vec::new();
    let mut taken: usize = 0;
    selection.runs(record, &inner, |at, offset, count| {
        let too_large = || invalid(format!("the NetCDF variable {name} is too large to write"));
        let place = at
            .unwrap_or(0)
            .checked_mul(step)
            .zip(offset.checked_mul(width))
            .and_then(|(record, inner)| record.checked_add(inner))
            .and_then(|start| start.checked_add(variable.begin));
        let length = count
            .checked_mul(width)
            .and_then(|length| usize::try_from(length).ok());
        let (Some(place), Some(length)) = (place, length) else {
            return Err(too_large());
        };
        let from = taken
            .checked_add(length)
            .filter(|&from| from <= stored.len())
            .ok_or_else(too_large)?;
        match runs.last_mut() {
            // A run that starts where the last one ends joins it: one write, not two.
            Some((start, range)) if *start + (range.len() as u64) == place => range.end = from,
            _ => runs.push((place, taken..from)),
        }
        taken = from;
        Ok(())
    })?;
    put(path, header, needed, &runs, &stored)
        .map_err(|error| SubscriptFailure::Os(OsFailure::from_io(&error, path)))
}

/// Writes the values of `runs`, each a place in the file at `path` and the range of `stored`
/// that goes there, in place, after adding records until the file has `needed`. Only the
/// bytes a write changes are written: the records it adds, filled for every record variable,
/// padding and all, as the C library fills them; its values; and the header's record count.
///
/// The order keeps the file valid whatever stops the write (decision 0047, row 3.14.7): the
/// new records go past the end, where a reader that trusts the record count does not look,
/// and are flushed to the disk; then the values; then the count, flushed again, so the count
/// never claims a record that is not there. A crash in the middle may leave some of the values
/// written and others not, but never a file cut short. Records that fail to be added are cut
/// off again, so a full disk leaves the file as it was.
///
/// # Errors
///
/// The first error the operating system gives.
fn put(
    path: &str,
    header: &Header,
    needed: u64,
    runs: &[(u64, std::ops::Range<usize>)],
    stored: &[u8],
) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let grows = needed > header.records;
    if grows {
        let length = file.metadata()?.len();
        let added = add_records(&mut file, header, needed);
        if added.is_err() {
            // The error to report is the first; the file keeps its records either way.
            let _ = file.set_len(length);
        }
        added?;
        file.sync_data()?;
    }
    for (place, range) in runs {
        file.seek(SeekFrom::Start(*place))?;
        file.write_all(&stored[range.clone()])?;
    }
    file.sync_data()?;
    if grows {
        let count = u32::try_from(needed).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "too many records for the header",
            )
        })?;
        file.seek(SeekFrom::Start(4))?;
        file.write_all(&count.to_be_bytes())?;
        file.sync_data()?;
    }
    Ok(())
}

/// Writes the records from `header`'s last one up to `needed` at the end of the records in
/// `file`, each filled for every record variable, through a buffer of about a megabyte.
///
/// # Errors
///
/// The first error the operating system gives.
fn add_records(file: &mut fs::File, header: &Header, needed: u64) -> io::Result<()> {
    let step = header.record_size();
    let records: Vec<&Variable> = header
        .variables
        .iter()
        .filter(|variable| header.is_record(variable))
        .collect();
    let Some(first) = records.iter().map(|variable| variable.begin).min() else {
        return Ok(());
    };
    let too_large = || io::Error::new(io::ErrorKind::InvalidData, "a record past 2^64 bytes");
    let one = usize::try_from(step).map_err(|_| too_large())?;
    // One record, each variable's fill value at its place in it.
    let mut record = vec![0_u8; one];
    for variable in &records {
        let size = slot(header, variable);
        let at = usize::try_from(variable.begin - first).map_err(|_| too_large())?;
        let place = record.get_mut(at..at + size).ok_or_else(too_large)?;
        place.copy_from_slice(&filled(&fill(variable), size));
    }
    let start = header
        .records
        .checked_mul(step)
        .and_then(|offset| offset.checked_add(first))
        .ok_or_else(too_large)?;
    file.seek(SeekFrom::Start(start))?;
    let per_block = (BUFFER / one.max(1)).max(1);
    let block = record.repeat(per_block);
    let mut left = needed - header.records;
    while left > 0 {
        let count = left.min(per_block as u64);
        // At most `per_block`, which is a `usize`.
        let count = usize::try_from(count).unwrap_or(per_block);
        file.write_all(&block[..count * one])?;
        left -= count as u64;
    }
    Ok(())
}

/// `size` bytes of the fill value `fill` repeated.
fn filled(fill: &[u8], size: usize) -> Vec<u8> {
    fill.iter().copied().cycle().take(size).collect()
}

/// Adds the dimension `name` of `length`, 0 for the unlimited one, to `header`.
fn add_dimension(header: &mut Header, name: &str, length: u64) -> Result<(), String> {
    if header.dimensions.iter().any(|(known, _)| known == name) {
        return Err("NetCDF: String match to name in use".to_owned());
    }
    if length == 0 && header.dimensions.iter().any(|&(_, known)| known == 0) {
        return Err("NetCDF: NC_UNLIMITED size already in use".to_owned());
    }
    // The header stores a length in 32 bits.
    if u32::try_from(length).is_err() {
        return Err("NetCDF: Invalid dimension size".to_owned());
    }
    header.dimensions.push((name.to_owned(), length));
    Ok(())
}

/// Adds the variable `name` of the type `kind`, as `netCDF4` names it, over the dimensions
/// named in `spans`, separated by commas, to `header`. Its values are filled when the file is
/// written again.
fn add_variable(header: &mut Header, name: &str, kind: &str, spans: &str) -> Result<(), String> {
    if header
        .variables
        .iter()
        .any(|variable| variable.name == name)
    {
        return Err("NetCDF: String match to name in use".to_owned());
    }
    let kind = match kind {
        "i1" => Kind::Byte,
        "S1" => Kind::Char,
        "i2" => Kind::Short,
        "i4" => Kind::Int,
        "f4" => Kind::Float,
        "f8" => Kind::Double,
        _ => return Err(format!("{kind} is not a type a classic NetCDF file holds")),
    };
    let mut dimensions = Vec::new();
    for span in spans.split(',').filter(|span| !span.is_empty()) {
        let Some(index) = header
            .dimensions
            .iter()
            .position(|(known, _)| known == span)
        else {
            return Err(format!("the NetCDF file has no dimension {span}"));
        };
        if header.dimensions[index].1 == 0 && !dimensions.is_empty() {
            return Err("NetCDF: NC_UNLIMITED in the wrong index".to_owned());
        }
        dimensions.push(index);
    }
    header.variables.push(Variable {
        name: name.to_owned(),
        dimensions,
        attributes: Vec::new(),
        kind,
        begin: 0,
        size: 0,
    });
    let added = &header.variables[header.variables.len() - 1];
    // The header stores a variable's size in 32 bits.
    if u32::try_from(slot(header, added)).is_err() {
        header.variables.pop();
        return Err("NetCDF: One or more variable sizes violate format constraints".to_owned());
    }
    Ok(())
}

/// The attribute `name` of the kind `kind`, `str`, `int` or `float`, whose value is `value`.
fn attribute(kind: &str, name: &str, value: &str) -> Result<Attribute, String> {
    let (kind, values) = match kind {
        "str" => (Kind::Char, Values::Text(value.to_owned())),
        "int" => {
            let number: i64 = value
                .parse()
                .map_err(|_| "an attribute's integer".to_owned())?;
            let number = i32::try_from(number).map_err(|_| {
                format!("{number} does not fit the 32-bit integer of a classic file")
            })?;
            (Kind::Int, Values::Numbers(vec![f64::from(number)]))
        }
        "float" => {
            let number: f64 = value
                .parse()
                .map_err(|_| "an attribute's number".to_owned())?;
            (Kind::Double, Values::Numbers(vec![number]))
        }
        _ => return Err(format!("{kind} is not an attribute's kind")),
    };
    Ok(Attribute {
        name: name.to_owned(),
        kind,
        values,
    })
}

/// Sets `attribute` on the variable named `owner` of `header`, or on the file when `owner` is
/// empty: in its place when there is one of its name, else after the others.
fn set_attribute(header: &mut Header, owner: &str, attribute: Attribute) -> Result<(), String> {
    let attributes = if owner.is_empty() {
        &mut header.attributes
    } else {
        let Some(variable) = header
            .variables
            .iter_mut()
            .find(|variable| variable.name == owner)
        else {
            return Err(format!("the NetCDF file has no variable {owner}"));
        };
        &mut variable.attributes
    };
    match attributes
        .iter_mut()
        .find(|known| known.name == attribute.name)
    {
        Some(known) => *known = attribute,
        None => attributes.push(attribute),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
