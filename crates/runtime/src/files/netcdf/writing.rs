//! Writing a `NetCDF` classic file as `netCDF4` writes it (decision 0047). The file is the state:
//! each change a program makes, a dimension, a variable, an attribute or a variable's values,
//! reads the whole file, changes it in memory, and writes it again, laid out as the C library
//! `netCDF4` calls lays it out, so the bytes match `netCDF4`'s for the same calls.
//!
//! The layout: the header, with no free space after it; then each fixed-size variable's values,
//! in the order they were defined, each padded to a multiple of 4 bytes; then the records, each
//! holding one slot of every record variable in the order they were defined. A slot is padded
//! to a multiple of 4 bytes, unless there is only one record variable. Values not written hold
//! the type's fill value, padding included: a write that adds records fills them for every
//! record variable before it writes its values, as the C library does. The `storage` module
//! reads a file into memory and writes it out again in this layout.
//!
//! Cost: each change reads and writes the whole file. A program defines its dimensions and
//! variables while the file is small, then writes each variable once, so each variable's
//! values are copied about as many times as variables are written after it.

use super::encoding::{encode, store};
use super::storage::{Contents, load, save, slot};
use super::{Attribute, Header, Kind, Values, Variable, scaling};
use crate::files::fits::Pixel;
use crate::files::{ErrorKind, OsFailure};

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
        };
        return save(
            path,
            &Contents {
                header,
                slots: Vec::new(),
            },
        );
    }
    let mut contents = load(path)?;
    match fields[..] {
        ["D", name, length] => {
            let length: u64 = length
                .parse()
                .map_err(|_| invalid("a dimension's length"))?;
            add_dimension(&mut contents.header, name, length)
                .map_err(|message| invalid(&message))?;
        }
        ["V", name, kind, spans] => {
            add_variable(&mut contents, name, kind, spans).map_err(|message| invalid(&message))?;
        }
        ["A", owner, kind, name, value] => {
            let attribute = attribute(kind, name, value).map_err(|message| invalid(&message))?;
            set_attribute(&mut contents.header, owner, attribute)
                .map_err(|message| invalid(&message))?;
        }
        _ => return Err(invalid("not a change to a NetCDF file")),
    }
    save(path, &contents)
}

/// Writes `values`, every element of an array of `pixel`s with the lengths `shape`, in
/// row-major order and the machine's byte order, as all the values of the variable `name` of
/// the file at `path`, converted to the variable's type as `NumPy`'s `astype` converts them and
/// packed by `scale_factor` and `add_offset` as `netCDF4` packs them. A record variable takes
/// as many records as the array's first length, adding records to the file when it has fewer.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the file has no variable
/// `name`, the array's rank or a fixed length differs from the variable's, or the variable
/// holds text.
pub(crate) fn write(
    path: &str,
    name: &str,
    pixel: Pixel,
    shape: &[usize],
    values: &[u8],
) -> Result<(), OsFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let mut contents = load(path)?;
    let header = &contents.header;
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
    let lengths: Vec<u64> = variable
        .dimensions
        .iter()
        .map(|&dimension| header.length(dimension))
        .collect();
    let fits = shape.len() == lengths.len()
        && shape
            .iter()
            .zip(&lengths)
            .enumerate()
            .all(|(axis, (&given, &length))| (record && axis == 0) || given as u64 == length);
    if !fits {
        let wanted: Vec<String> = lengths.iter().map(u64::to_string).collect();
        let given: Vec<String> = shape.iter().map(usize::to_string).collect();
        return Err(invalid(format!(
            "the NetCDF variable {name} has the shape ({}), which an array of shape ({}) does \
             not fill",
            wanted.join(", "),
            given.join(", ")
        )));
    }
    let scale = scaling(variable);
    let kind = variable.kind;
    let stored: Vec<u8> = values
        .chunks_exact(pixel.size())
        .flat_map(|element| store(element, pixel, kind, scale))
        .collect();
    if !record {
        contents.slots[index][..stored.len()].copy_from_slice(&stored);
        return save(path, &contents);
    }
    let per_record = usize::try_from(header.values(variable))
        .unwrap_or(usize::MAX)
        .saturating_mul(kind.size())
        .max(1);
    // What a new record holds for each record variable before any value is written.
    let blank: Vec<(usize, Vec<u8>, usize)> = header
        .variables
        .iter()
        .enumerate()
        .filter(|(_, other)| header.is_record(other))
        .map(|(other, variable)| (other, fill(variable), slot(header, variable)))
        .collect();
    let records = header.records;
    let size = slot(header, variable);
    for (at, values) in stored.chunks(per_record).enumerate() {
        // A new record is filled for every record variable, padding and all, before its values
        // are written, as the C library fills the records a write adds.
        if (at as u64) >= records {
            for (other, filling, other_size) in &blank {
                contents.slots[*other].extend(filled(filling, *other_size));
            }
        }
        let start = at * size;
        contents.slots[index][start..start + values.len()].copy_from_slice(values);
    }
    let written = (stored.len() / per_record) as u64;
    contents.header.records = records.max(written);
    save(path, &contents)
}

/// The big-endian bytes of `variable`'s fill value: its `_FillValue` attribute's, or the
/// type's default, as the C library fills.
fn fill(variable: &Variable) -> Vec<u8> {
    let own = variable
        .attributes
        .iter()
        .find(|attribute| attribute.name == "_FillValue" && attribute.kind == variable.kind)
        .and_then(|attribute| match &attribute.values {
            Values::Numbers(numbers) => numbers.first().copied(),
            Values::Text(_) => None,
        });
    match own {
        Some(value) => encode(value, variable.kind),
        None => match variable.kind {
            Kind::Byte => vec![0x81],
            Kind::Char => vec![0],
            Kind::Short => vec![0x80, 0x01],
            Kind::Int => vec![0x80, 0, 0, 0x01],
            Kind::Float => vec![0x7C, 0xF0, 0, 0],
            Kind::Double => vec![0x47, 0x9E, 0, 0, 0, 0, 0, 0],
        },
    }
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
/// named in `spans`, separated by commas, to `contents`, its values all fill values.
fn add_variable(
    contents: &mut Contents,
    name: &str,
    kind: &str,
    spans: &str,
) -> Result<(), String> {
    let header = &mut contents.header;
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
    let variable = Variable {
        name: name.to_owned(),
        dimensions,
        attributes: Vec::new(),
        kind,
        begin: 0,
        size: 0,
    };
    let before: Vec<usize> = header
        .variables
        .iter()
        .map(|known| slot(header, known))
        .collect();
    header.variables.push(variable);
    let added = &header.variables[header.variables.len() - 1];
    let size = slot(header, added);
    // The header stores a variable's size in 32 bits, so a larger one, whose fill values this
    // would also have to hold in memory, is refused before anything is allocated.
    if u32::try_from(size).is_err() {
        header.variables.pop();
        return Err("NetCDF: One or more variable sizes violate format constraints".to_owned());
    }
    let count = if header.is_record(added) {
        header.records
    } else {
        1
    };
    let fill = fill(added);
    // A second record variable pads the first one's slots, which were unpadded while it was
    // the only one. The C library moves each record as a block of the new slot's size from
    // where the old record began, so the padding holds the bytes that followed it; this does
    // the same, with zeros past the end.
    for (index, &old) in before.iter().enumerate() {
        let new = slot(header, &header.variables[index]);
        if header.is_record(&header.variables[index]) && new != old {
            let region = &contents.slots[index];
            let mut moved = Vec::with_capacity(new * region.len() / old.max(1));
            for record in 0..region.len() / old.max(1) {
                let start = record * old;
                let end = (start + new).min(region.len());
                let mut slot = region[start..end].to_vec();
                slot.resize(new, 0);
                moved.extend(slot);
            }
            contents.slots[index] = moved;
        }
    }
    let mut bytes = Vec::new();
    for _ in 0..count {
        bytes.extend(filled(&fill, size));
    }
    contents.slots.push(bytes);
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
