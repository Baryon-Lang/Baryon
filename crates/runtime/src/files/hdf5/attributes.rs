//! Attributes: the small named values on a group or a dataset, and the text of variable-length
//! strings, which h5py stores in the global heap.
//!
//! An object keeps up to eight attributes as messages in its header; past that, it keeps them
//! in a fractal heap that an attribute info message names (dense storage). A variable-length
//! value is stored as its length, the address of a global heap collection (`GCOL`) and the
//! index of the object in it.

use super::cursor::{Failure, Source, UNDEFINED};
use super::header::Message;
use super::messages::{ATTRIBUTE, ATTRIBUTE_INFO, attribute, dense as dense_info};
use super::{Attribute, Datatype, Values, dense, joined};
use crate::files::netcdf::python_float;

/// The attributes of the object whose header holds `messages`, those in the header first.
///
/// # Errors
///
/// A damaged attribute, heap or index is reported.
pub(super) fn attributes(
    source: &mut Source,
    messages: &[Message],
) -> Result<Vec<Attribute>, Failure> {
    let mut stored: Vec<Vec<u8>> = messages
        .iter()
        .filter(|message| message.kind == ATTRIBUTE)
        .map(|message| message.data.clone())
        .collect();
    if let Some(info) = messages
        .iter()
        .find(|message| message.kind == ATTRIBUTE_INFO)
    {
        let (heap, index) = dense_info(source, &info.data, false)?;
        if heap != UNDEFINED {
            stored.extend(dense::records(source, heap, index, false)?);
        }
    }
    stored
        .iter()
        .map(|data| {
            let (name, datatype, shape, bytes) = attribute(source, data)?;
            let shape = shape.map(|(shape, _)| shape).unwrap_or_default();
            let count = shape.iter().product::<u64>();
            let values = values(source, datatype, count, &bytes)?;
            Ok(Attribute {
                name,
                datatype,
                shape,
                values,
            })
        })
        .collect()
}

/// The `count` values of `datatype` stored in `bytes`.
fn values(
    source: &mut Source,
    datatype: Datatype,
    count: u64,
    bytes: &[u8],
) -> Result<Values, Failure> {
    let size = usize::try_from(datatype.size(source)).unwrap_or(usize::MAX);
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    if size == 0 || bytes.len() / size < count {
        return Err("the HDF5 file has an attribute whose values are cut short".to_owned());
    }
    let elements = bytes.chunks_exact(size).take(count);
    Ok(match datatype {
        Datatype::Integer { signed, big, .. } => Values::Integers(
            elements
                .map(|element| integer(element, signed, big))
                .collect(),
        ),
        Datatype::Float { big, .. } => {
            Values::Floats(elements.map(|element| float(element, big)).collect())
        }
        Datatype::Text { .. } => Values::Text(
            elements
                .map(|element| text(element.to_vec()))
                .collect::<Result<_, _>>()?,
        ),
        Datatype::VariableText => {
            let mut found = Vec::new();
            for element in elements {
                found.push(text(heap_object(source, element)?)?);
            }
            Values::Text(found)
        }
        Datatype::Other { .. } => Values::Unread,
    })
}

/// The integer stored in `bytes`.
pub(super) fn integer(bytes: &[u8], signed: bool, big: bool) -> i128 {
    let mut ordered = bytes.to_vec();
    if big {
        ordered.reverse();
    }
    let negative = signed && ordered.last().is_some_and(|&top| top & 0x80 != 0);
    let mut wide = [if negative { 0xFF } else { 0 }; 16];
    let length = ordered.len().min(16);
    wide[..length].copy_from_slice(&ordered[..length]);
    i128::from_le_bytes(wide)
}

/// The float of 4 or 8 bytes stored in `bytes`.
pub(super) fn float(bytes: &[u8], big: bool) -> f64 {
    let mut ordered = bytes.to_vec();
    if big {
        ordered.reverse();
    }
    if ordered.len() == 4 {
        f64::from(f32::from_le_bytes([
            ordered[0], ordered[1], ordered[2], ordered[3],
        ]))
    } else {
        let mut eight = [0; 8];
        let length = ordered.len().min(8);
        eight[..length].copy_from_slice(&ordered[..length]);
        f64::from_le_bytes(eight)
    }
}

/// The text of `bytes`, without the zero bytes that may end it.
fn text(mut bytes: Vec<u8>) -> Result<String, Failure> {
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(|_| "the HDF5 file has text that is not UTF-8".to_owned())
}

/// The bytes of the global heap object a variable-length value `element` refers to.
fn heap_object(source: &mut Source, element: &[u8]) -> Result<Vec<u8>, Failure> {
    let mut cursor = source.cursor(element);
    let length = u64::from(cursor.u32()?);
    let collection = cursor.address()?;
    let index = cursor.u32()?;
    if collection == UNDEFINED || length == 0 {
        return Ok(Vec::new());
    }
    let head = source.bytes(collection, 8 + u64::from(source.lengths))?;
    let mut cursor = source.cursor(&head);
    cursor.signature(*b"GCOL", "global heap collection")?;
    cursor.skip(4)?;
    let size = cursor.length()?;
    let bytes = source.bytes(collection, size)?;
    let mut cursor = source.cursor(&bytes);
    cursor.skip(head.len())?;
    while cursor.rest().len() >= 8 + usize::from(source.lengths) {
        let at = cursor.u16()?;
        cursor.skip(6)?;
        let size = cursor.length()?;
        if at == 0 {
            break;
        }
        let size = usize::try_from(size).unwrap_or(usize::MAX);
        let data = cursor.take(size)?;
        if u32::from(at) == index {
            return Ok(data[..data
                .len()
                .min(usize::try_from(length).unwrap_or(usize::MAX))]
                .to_vec());
        }
        cursor.align(8)?;
    }
    Err(format!(
        "the HDF5 file's global heap at {collection} has no object {index}"
    ))
}

/// The description line of `attribute` of the object `owner`: see [`super::describe`].
pub(super) fn line(owner: &str, attribute: &Attribute) -> String {
    let (kind, values): (&str, Vec<String>) = match &attribute.values {
        Values::Text(texts) => ("str", texts.clone()),
        Values::Integers(numbers) => ("int", numbers.iter().map(i128::to_string).collect()),
        Values::Floats(numbers) => (
            "float",
            numbers.iter().map(|&value| python_float(value)).collect(),
        ),
        Values::Unread => ("other", Vec::new()),
    };
    let mut line = format!(
        "A\t{owner}\t{kind}\t{}\t{}",
        joined(&attribute.shape),
        attribute.name
    );
    for value in values {
        line.push('\t');
        line.push_str(&value);
    }
    line
}
