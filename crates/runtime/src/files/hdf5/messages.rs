//! The object header messages a reader needs: the dataspace (shape), the datatype, the data
//! layout, the filter pipeline, attributes, and the messages that list a group's links.

use super::cursor::{Cursor, Failure, Source, name};
use super::{Datatype, Filter, Layout};

/// A dataspace message.
pub(super) const DATASPACE: u16 = 0x01;
/// A link info message: where a group keeps its links when they are many.
pub(super) const LINK_INFO: u16 = 0x02;
/// A datatype message.
pub(super) const DATATYPE: u16 = 0x03;
/// An old fill value message.
pub(super) const FILL_OLD: u16 = 0x04;
/// A fill value message.
pub(super) const FILL: u16 = 0x05;
/// A link message: one link of a group.
pub(super) const LINK: u16 = 0x06;
/// A data layout message.
pub(super) const LAYOUT: u16 = 0x08;
/// A filter pipeline message.
pub(super) const FILTERS: u16 = 0x0B;
/// An attribute message.
pub(super) const ATTRIBUTE: u16 = 0x0C;
/// A symbol table message: the B-tree and heap of an older group.
pub(super) const SYMBOL_TABLE: u16 = 0x11;
/// An attribute info message: where an object keeps its attributes when they are many.
pub(super) const ATTRIBUTE_INFO: u16 = 0x15;

/// A dataspace: the shape, slowest axis first, empty for a scalar, and the largest shape it
/// may grow to, `u64::MAX` along a dimension without limit. `None` for the null dataspace,
/// which holds no value.
pub(super) fn dataspace(source: &Source, data: &[u8]) -> Result<Option<Shape>, Failure> {
    let mut cursor = source.cursor(data);
    let version = cursor.u8()?;
    let rank = cursor.u8()?;
    let flags = cursor.u8()?;
    match version {
        1 => cursor.skip(5)?,
        2 => {
            if cursor.u8()? == 2 {
                return Ok(None);
            }
        }
        _ => return Err(unknown("dataspace", version)),
    }
    let shape = (0..rank)
        .map(|_| cursor.length())
        .collect::<Result<Vec<_>, _>>()?;
    let all = if source.lengths >= 8 {
        u64::MAX
    } else {
        (1 << (8 * u32::from(source.lengths))) - 1
    };
    let most = if flags & 1 == 0 {
        shape.clone()
    } else {
        (0..rank)
            .map(|_| {
                cursor
                    .length()
                    .map(|most| if most == all { u64::MAX } else { most })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(Some((shape, most)))
}

/// A shape and the largest shape it may grow to.
pub(super) type Shape = (Vec<u64>, Vec<u64>);

/// A fill value message, new (type 5) or old (type 4): the bytes of the value unwritten
/// elements read as, `None` when the file leaves it to the library's default, zero.
pub(super) fn fill(kind: u16, data: &[u8]) -> Result<Option<Vec<u8>>, Failure> {
    let mut cursor = Cursor::plain(data);
    if kind == FILL_OLD {
        let size = cursor.u32()?;
        return Ok(Some(
            cursor
                .take(usize::try_from(size).unwrap_or(usize::MAX))?
                .to_vec(),
        ));
    }
    let version = cursor.u8()?;
    let defined = match version {
        1 | 2 => {
            cursor.skip(2)?;
            cursor.u8()? != 0
        }
        3 => cursor.u8()? & 0x20 != 0,
        _ => return Err(unknown("fill value", version)),
    };
    if !defined {
        return Ok(None);
    }
    let size = cursor.u32()?;
    let value = cursor
        .take(usize::try_from(size).unwrap_or(usize::MAX))?
        .to_vec();
    Ok((!value.is_empty()).then_some(value))
}

/// A datatype.
pub(super) fn datatype(data: &[u8]) -> Result<Datatype, Failure> {
    let mut cursor = Cursor::plain(data);
    let class_and_version = cursor.u8()?;
    let bits = cursor.take(3)?;
    let size = cursor.u32()?;
    let big = bits[0] & 1 != 0;
    Ok(match class_and_version & 0x0F {
        0 => Datatype::Integer {
            size,
            signed: bits[0] & 0x08 != 0,
            big,
        },
        // A float with bit 6 set as well is VAX order, which no reader converts.
        1 if bits[0] & 0x40 == 0 && matches!(size, 4 | 8) => Datatype::Float { size, big },
        3 => Datatype::Text { size },
        9 if bits[0] & 0x0F == 1 => Datatype::VariableText,
        class => Datatype::Other { class, size },
    })
}

/// A data layout message.
pub(super) fn layout(source: &Source, data: &[u8]) -> Result<Layout, Failure> {
    let mut cursor = source.cursor(data);
    let version = cursor.u8()?;
    // Version 5, which HDF5 2.0 writes, is encoded as version 4 is.
    if !matches!(version, 3..=5) {
        return Err(unknown("data layout", version));
    }
    match cursor.u8()? {
        0 => {
            let size = cursor.u16()?;
            Ok(Layout::Compact(cursor.take(usize::from(size))?.to_vec()))
        }
        1 => Ok(Layout::Contiguous {
            address: cursor.address()?,
            size: cursor.length()?,
        }),
        2 if version == 3 => {
            let rank = cursor.u8()?;
            let address = cursor.address()?;
            let mut chunk = (0..rank)
                .map(|_| cursor.u32().map(u64::from))
                .collect::<Result<Vec<_>, _>>()?;
            chunk.pop();
            Ok(Layout::Chunked {
                chunk,
                index: super::Index::Tree(address),
            })
        }
        2 => late_chunks(&mut cursor),
        3 => Err("the HDF5 file has a virtual dataset, which is not read yet".to_owned()),
        class => Err(format!(
            "the HDF5 file has a data layout of class {class}, which the format does not define"
        )),
    }
}

/// The chunked layout of a version 4 layout message, after its class.
fn late_chunks(cursor: &mut Cursor<'_>) -> Result<Layout, Failure> {
    let flags = cursor.u8()?;
    let rank = cursor.u8()?;
    let width = usize::from(cursor.u8()?);
    let mut chunk = (0..rank)
        .map(|_| cursor.sized(width))
        .collect::<Result<Vec<_>, _>>()?;
    chunk.pop();
    let index = match cursor.u8()? {
        1 => {
            let filtered = if flags & 2 == 0 {
                None
            } else {
                Some((cursor.length()?, cursor.u32()?))
            };
            super::Index::Single {
                address: cursor.address()?,
                filtered,
            }
        }
        2 => super::Index::Implicit(cursor.address()?),
        3 => {
            cursor.skip(1)?;
            super::Index::Fixed(cursor.address()?)
        }
        4 => {
            cursor.skip(5)?;
            super::Index::Extensible(cursor.address()?)
        }
        5 => {
            cursor.skip(6)?;
            super::Index::Tree2(cursor.address()?)
        }
        kind => {
            return Err(format!(
                "the HDF5 file has a chunk index of type {kind}, which the format does not define"
            ));
        }
    };
    Ok(Layout::Chunked { chunk, index })
}

/// A filter pipeline: each filter's number and parameters, in the order they were applied
/// when writing.
pub(super) fn filters(data: &[u8]) -> Result<Vec<Filter>, Failure> {
    let mut cursor = Cursor::plain(data);
    let version = cursor.u8()?;
    let count = cursor.u8()?;
    if version == 1 {
        cursor.skip(6)?;
    } else if version != 2 {
        return Err(unknown("filter pipeline", version));
    }
    let mut filters = Vec::new();
    for _ in 0..count {
        let id = cursor.u16()?;
        let named = version == 1 || id >= 256;
        let length = if named { cursor.u16()? } else { 0 };
        let flags = cursor.u16()?;
        let values = cursor.u16()?;
        let mut label = String::new();
        if named {
            label = name(cursor.take(usize::from(length))?)?;
            if version == 1 {
                cursor.align(8)?;
            }
        }
        let parameters = (0..values)
            .map(|_| cursor.u32())
            .collect::<Result<Vec<_>, _>>()?;
        if version == 1 && values % 2 == 1 {
            cursor.skip(4)?;
        }
        filters.push(Filter {
            id,
            name: label,
            optional: flags & 1 != 0,
            parameters,
        });
    }
    Ok(filters)
}

/// An attribute message: its name, datatype, shape and the bytes of its values.
pub(super) fn attribute(
    source: &Source,
    data: &[u8],
) -> Result<(String, Datatype, Option<Shape>, Vec<u8>), Failure> {
    let mut cursor = source.cursor(data);
    let version = cursor.u8()?;
    cursor.u8()?;
    let name_size = usize::from(cursor.u16()?);
    let type_size = usize::from(cursor.u16()?);
    let space_size = usize::from(cursor.u16()?);
    match version {
        1 | 2 => {}
        3 => {
            cursor.u8()?;
        }
        _ => return Err(unknown("attribute", version)),
    }
    let padded = |size: usize| {
        if version == 1 {
            size.div_ceil(8) * 8
        } else {
            size
        }
    };
    let label = name(cursor.take(padded(name_size))?)?;
    let kind = datatype(&cursor.take(padded(type_size))?[..type_size])?;
    let shape = dataspace(source, &cursor.take(padded(space_size))?[..space_size])?;
    Ok((label, kind, shape, cursor.rest().to_vec()))
}

/// A link message: the link's name and, for a hard link, the address of the object's header;
/// `None` for a soft or external link, which names a path instead.
pub(super) fn link(source: &Source, data: &[u8]) -> Result<(String, Option<u64>), Failure> {
    let mut cursor = source.cursor(data);
    let version = cursor.u8()?;
    if version != 1 {
        return Err(unknown("link", version));
    }
    let flags = cursor.u8()?;
    let kind = if flags & 0x08 != 0 { cursor.u8()? } else { 0 };
    if flags & 0x04 != 0 {
        cursor.skip(8)?;
    }
    if flags & 0x10 != 0 {
        cursor.u8()?;
    }
    let length = cursor.sized(1 << (flags & 3))?;
    let length = usize::try_from(length).map_err(|_| "a link name too long".to_owned())?;
    let label = String::from_utf8(cursor.take(length)?.to_vec())
        .map_err(|_| "the HDF5 file has a name that is not UTF-8".to_owned())?;
    let address = if kind == 0 {
        Some(cursor.address()?)
    } else {
        None
    };
    Ok((label, address))
}

/// A link info or attribute info message: the address of the fractal heap that holds the
/// links or attributes when they are stored densely, and of the B-tree that indexes their
/// names; the heap's address is undefined when they are in the object header instead.
pub(super) fn dense(source: &Source, data: &[u8], links: bool) -> Result<(u64, u64), Failure> {
    let mut cursor = source.cursor(data);
    cursor.u8()?;
    let flags = cursor.u8()?;
    if flags & 1 != 0 {
        cursor.skip(if links { 8 } else { 2 })?;
    }
    Ok((cursor.address()?, cursor.address()?))
}

/// The message for a structure `what` of a version the format does not define.
fn unknown(what: &str, version: u8) -> Failure {
    format!(
        "the HDF5 file has a {what} message of version {version}, which the format does not define"
    )
}
