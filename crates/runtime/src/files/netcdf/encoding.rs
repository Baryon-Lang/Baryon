//! The bytes of a `NetCDF` classic file's header and of its values, as the specification lays
//! them out: big-endian numbers, and names and lists of values padded to a multiple of 4 bytes.

use super::header::{ATTRIBUTES, DIMENSIONS, VARIABLES};
use super::{Attribute, Header, Kind, Scale, Values};
use crate::files::fits::Pixel;

/// The big-endian bytes of `value` as a value of `kind`, a number type, converted as `NumPy`'s
/// `astype` converts: an integer wraps, a float rounds to nearest.
pub(super) fn encode(value: f64, kind: Kind) -> Vec<u8> {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "wrapping and rounding are NumPy's conversions"
    )]
    match kind {
        Kind::Byte | Kind::Char => (value as i64 as i8).to_be_bytes().to_vec(),
        Kind::Short => (value as i64 as i16).to_be_bytes().to_vec(),
        Kind::Int => (value as i64 as i32).to_be_bytes().to_vec(),
        Kind::Float => (value as f32).to_be_bytes().to_vec(),
        Kind::Double => value.to_be_bytes().to_vec(),
    }
}

/// The bytes of `header`, as the specification lays them out.
pub(super) fn encode_header(header: &Header) -> Vec<u8> {
    let mut bytes = vec![b'C', b'D', b'F', if header.wide { 2 } else { 1 }];
    push_u32(
        &mut bytes,
        u32::try_from(header.records).unwrap_or(u32::MAX),
    );
    if header.dimensions.is_empty() {
        bytes.extend_from_slice(&[0; 8]);
    } else {
        push_u32(&mut bytes, DIMENSIONS);
        push_u32(&mut bytes, count(header.dimensions.len()));
        for (name, length) in &header.dimensions {
            push_name(&mut bytes, name);
            push_u32(&mut bytes, u32::try_from(*length).unwrap_or(u32::MAX));
        }
    }
    push_attributes(&mut bytes, &header.attributes);
    if header.variables.is_empty() {
        bytes.extend_from_slice(&[0; 8]);
    } else {
        push_u32(&mut bytes, VARIABLES);
        push_u32(&mut bytes, count(header.variables.len()));
        for variable in &header.variables {
            push_name(&mut bytes, &variable.name);
            push_u32(&mut bytes, count(variable.dimensions.len()));
            for &dimension in &variable.dimensions {
                push_u32(&mut bytes, count(dimension));
            }
            push_attributes(&mut bytes, &variable.attributes);
            push_u32(&mut bytes, kind_number(variable.kind));
            push_u32(&mut bytes, u32::try_from(variable.size).unwrap_or(u32::MAX));
            if header.wide {
                bytes.extend_from_slice(&variable.begin.to_be_bytes());
            } else {
                push_u32(
                    &mut bytes,
                    u32::try_from(variable.begin).unwrap_or(u32::MAX),
                );
            }
        }
    }
    bytes
}

/// Appends a list of attributes, or the two zero words of an absent one.
fn push_attributes(bytes: &mut Vec<u8>, attributes: &[Attribute]) {
    if attributes.is_empty() {
        bytes.extend_from_slice(&[0; 8]);
        return;
    }
    push_u32(bytes, ATTRIBUTES);
    push_u32(bytes, count(attributes.len()));
    for attribute in attributes {
        push_name(bytes, &attribute.name);
        push_u32(bytes, kind_number(attribute.kind));
        let values: Vec<u8> = match &attribute.values {
            Values::Text(text) => text.as_bytes().to_vec(),
            Values::Numbers(numbers) => numbers
                .iter()
                .flat_map(|&number| encode(number, attribute.kind))
                .collect(),
        };
        push_u32(bytes, count(values.len() / attribute.kind.size()));
        push_padded(bytes, &values);
    }
}

/// The number the format gives `kind`.
fn kind_number(kind: Kind) -> u32 {
    match kind {
        Kind::Byte => 1,
        Kind::Char => 2,
        Kind::Short => 3,
        Kind::Int => 4,
        Kind::Float => 5,
        Kind::Double => 6,
    }
}

/// `count` as a 32-bit word, as the format stores a count.
fn count(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Appends `value`, big-endian.
fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

/// Appends a name: its length, then its bytes, padded.
fn push_name(bytes: &mut Vec<u8>, name: &str) {
    push_u32(bytes, count(name.len()));
    push_padded(bytes, name.as_bytes());
}

/// Appends `values` and zeros to a multiple of 4 bytes.
fn push_padded(bytes: &mut Vec<u8>, values: &[u8]) {
    bytes.extend_from_slice(values);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
}

/// The stored bytes of one element `element` of `pixel`, in the machine's byte order, as a
/// value of `kind`, packed by `scale` as `netCDF4` packs: `(value - add_offset) / scale_factor`,
/// rounded half to even for an integer type.
pub(super) fn store(element: &[u8], pixel: Pixel, kind: Kind, scale: Option<Scale>) -> Vec<u8> {
    let mut bytes = [0_u8; 8];
    bytes[..element.len()].copy_from_slice(element);
    let integer = match pixel {
        Pixel::I8 => Some(i64::from(i8::from_ne_bytes([bytes[0]]))),
        Pixel::U8 => Some(i64::from(bytes[0])),
        Pixel::I16 => Some(i64::from(i16::from_ne_bytes([bytes[0], bytes[1]]))),
        Pixel::U16 => Some(i64::from(u16::from_ne_bytes([bytes[0], bytes[1]]))),
        Pixel::I32 => Some(i64::from(i32::from_ne_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))),
        Pixel::U32 => Some(i64::from(u32::from_ne_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))),
        Pixel::I64 => Some(i64::from_ne_bytes(bytes)),
        #[allow(clippy::cast_possible_wrap, reason = "NumPy's astype wraps")]
        Pixel::U64 => Some(u64::from_ne_bytes(bytes) as i64),
        Pixel::F32 | Pixel::F64 => None,
    };
    let float = match pixel {
        Pixel::F32 => f64::from(f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        #[allow(clippy::cast_precision_loss, reason = "NumPy's astype rounds")]
        _ => integer.map_or_else(|| f64::from_ne_bytes(bytes), |value| value as f64),
    };
    if let Some(scale) = scale {
        let mut packed = float;
        if let Some(offset) = scale.offset {
            packed -= offset;
        }
        if let Some(factor) = scale.factor {
            packed /= factor;
        }
        if !matches!(kind, Kind::Float | Kind::Double) {
            packed = packed.round_ties_even();
        }
        return encode(packed, kind);
    }
    match (integer, kind) {
        #[allow(clippy::cast_possible_truncation, reason = "NumPy's astype wraps")]
        (Some(value), Kind::Byte | Kind::Char) => (value as i8).to_be_bytes().to_vec(),
        #[allow(clippy::cast_possible_truncation, reason = "NumPy's astype wraps")]
        (Some(value), Kind::Short) => (value as i16).to_be_bytes().to_vec(),
        #[allow(clippy::cast_possible_truncation, reason = "NumPy's astype wraps")]
        (Some(value), Kind::Int) => (value as i32).to_be_bytes().to_vec(),
        _ => encode(float, kind),
    }
}
