//! The encodings of the messages that describe values: datatypes and dataspaces, as h5py
//! writes them for `NumPy` numbers and shapes.

/// The element type of numbers being written, always little-endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Element {
    /// An IEEE float of 4 or 8 bytes.
    Float(u8),
    /// A signed integer of 1, 2, 4 or 8 bytes.
    Signed(u8),
    /// An unsigned integer of 1, 2, 4 or 8 bytes.
    Unsigned(u8),
}

impl Element {
    /// The element type `NumPy` names `code`: `<f8`, `<i4`, `|u1` and the rest, little-endian
    /// or of one byte. `None` for any other.
    pub(super) fn parse(code: &str) -> Option<Self> {
        let (order, rest) = code.split_at_checked(1)?;
        let (kind, size) = rest.split_at_checked(1)?;
        let size: u8 = size.parse().ok()?;
        let element = match kind {
            "f" if size == 4 || size == 8 => Self::Float(size),
            "i" if [1, 2, 4, 8].contains(&size) => Self::Signed(size),
            "u" if [1, 2, 4, 8].contains(&size) => Self::Unsigned(size),
            _ => return None,
        };
        (order == "<" || (order == "|" && size == 1)).then_some(element)
    }

    /// The size of one element in bytes.
    pub(super) fn size(self) -> u8 {
        match self {
            Self::Float(size) | Self::Signed(size) | Self::Unsigned(size) => size,
        }
    }
}

/// The datatype message's body for `element`.
pub(super) fn datatype(element: Element) -> Vec<u8> {
    let size = element.size();
    let mut bytes = match element {
        Element::Float(_) => vec![0x11, 0x20, if size == 8 { 0x3F } else { 0x1F }, 0],
        Element::Signed(_) => vec![0x10, 0x08, 0, 0],
        Element::Unsigned(_) => vec![0x10, 0, 0, 0],
    };
    bytes.extend_from_slice(&u32::from(size).to_le_bytes());
    let precision = u16::from(size) * 8;
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&precision.to_le_bytes());
    if let Element::Float(_) = element {
        // Exponent location and size, mantissa location and size, exponent bias.
        let (exponent, fraction, bias): (u8, u8, u32) = if size == 8 {
            (11, 52, 1023)
        } else {
            (8, 23, 127)
        };
        bytes.extend_from_slice(&[fraction, exponent, 0, fraction]);
        bytes.extend_from_slice(&bias.to_le_bytes());
    }
    bytes
}

/// The version 1 dataspace message's body for `shape`: a single value for `[]`, otherwise
/// the lengths twice, as current and maximum.
pub(super) fn dataspace(shape: &[u64]) -> Vec<u8> {
    let rank = u8::try_from(shape.len()).unwrap_or(u8::MAX);
    let mut bytes = vec![1, rank, u8::from(rank > 0), 0, 0, 0, 0, 0];
    for _ in 0..2 {
        for length in shape {
            bytes.extend_from_slice(&length.to_le_bytes());
        }
    }
    bytes
}

/// `bytes` padded with zeros to a multiple of 8.
pub(super) fn pad8(bytes: &[u8]) -> Vec<u8> {
    let mut padded = bytes.to_vec();
    padded.resize(bytes.len().next_multiple_of(8), 0);
    padded
}
