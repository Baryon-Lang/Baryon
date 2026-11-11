//! `NumPy`'s `.npy` files (decision 0043): the header `Array.save` writes before an array's
//! elements and `Array.load` reads before them, in safe code the exported functions in
//! `abi::npy` call. The elements themselves are read and written as array files are (the
//! `arrays` module), straight into and out of the array's memory.
//!
//! A `.npy` file is the six bytes `\x93NUMPY`, a major and a minor version byte, the length of
//! the header, then the header: a Python dictionary literal in ASCII that names the element
//! type in `NumPy`'s spelling (`'descr': '<f8'`), whether the elements are in Fortran order, and
//! the shape, padded with spaces and a newline so the elements start on a multiple of 64 bytes.
//! Version 1.0 keeps the header's length in 2 bytes; 2.0 and 3.0 keep it in 4, and 3.0 allows
//! UTF-8 in the header, which no header of numbers needs.
//!
//! See `NumPy`'s `numpy/lib/format.py`, whose `_write_array_header` this module's [`header`]
//! follows byte for byte.

use super::{ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Read};

/// The bytes every `.npy` file starts with.
const MAGIC: &[u8; 6] = b"\x93NUMPY";

/// The multiple of bytes the elements start at, `NumPy`'s `ARRAY_ALIGN`.
const ALIGN: usize = 64;

/// How many digits `NumPy` leaves room for in the length of the first dimension, so the file
/// can grow along it without rewriting the elements: `NumPy`'s `GROWTH_AXIS_MAX_DIGITS`.
const GROWTH_DIGITS: usize = 21;

/// What a `.npy` file's header says about the array that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    /// The element type in `NumPy`'s spelling, such as `<f8`.
    pub(crate) descr: String,
    /// Whether the elements are in Fortran (column-major) order.
    pub(crate) fortran_order: bool,
    /// The length of each dimension.
    pub(crate) shape: Vec<usize>,
    /// Where the elements start, in bytes from the start of the file.
    pub(crate) offset: u64,
}

/// The header `numpy.save` writes for an array of the element type `descr` and the shape
/// `shape`, in row-major order: the magic bytes, the version, the length, and the dictionary
/// padded so the elements start on a multiple of 64 bytes. The version is 1.0, or 2.0 when the
/// header would not fit 1.0's 65535 bytes, which takes thousands of dimensions, as `NumPy`
/// chooses.
pub(crate) fn header(descr: &str, shape: &[usize]) -> Vec<u8> {
    let tuple = match shape {
        [] => "()".to_owned(),
        [length] => format!("({length},)"),
        _ => {
            let lengths: Vec<String> = shape.iter().map(usize::to_string).collect();
            format!("({})", lengths.join(", "))
        }
    };
    let mut dictionary =
        format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {tuple}, }}");
    if let Some(first) = shape.first() {
        let digits = first.to_string().len();
        dictionary.push_str(&" ".repeat(GROWTH_DIGITS.saturating_sub(digits)));
    }
    // The dictionary, its padding and the newline end on a multiple of `ALIGN` bytes, counting
    // the magic bytes, the version and the length before it.
    let padded = |length_bytes: usize| {
        let unpadded = MAGIC.len() + 2 + length_bytes + dictionary.len() + 1;
        dictionary.len() + (ALIGN - unpadded % ALIGN) % ALIGN + 1
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    let length = padded(2);
    if let Ok(short) = u16::try_from(length) {
        bytes.extend_from_slice(&[1, 0]);
        bytes.extend_from_slice(&short.to_le_bytes());
    } else {
        let long = u32::try_from(padded(4)).unwrap_or(u32::MAX);
        bytes.extend_from_slice(&[2, 0]);
        bytes.extend_from_slice(&long.to_le_bytes());
    }
    let end = bytes.len() + if bytes[6] == 1 { length } else { padded(4) };
    bytes.extend_from_slice(dictionary.as_bytes());
    bytes.resize(end - 1, b' ');
    bytes.push(b'\n');
    bytes
}

/// Reads the header of the `.npy` file at `path`.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the file does not start
/// with a `.npy` header of version 1.0, 2.0 or 3.0 that this module can read.
pub(crate) fn read_header(path: &str) -> Result<Header, OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let mut file = fs::File::open(path).map_err(failed)?;
    let mut start = [0_u8; 8];
    read_fully(&mut file, &mut start).map_err(failed)?;
    if start[..6] != MAGIC[..] {
        return Err(invalid("the file is not a .npy file".to_owned()));
    }
    let (major, minor) = (start[6], start[7]);
    let length = match major {
        1 => {
            let mut bytes = [0_u8; 2];
            read_fully(&mut file, &mut bytes).map_err(failed)?;
            usize::from(u16::from_le_bytes(bytes))
        }
        2 | 3 => {
            let mut bytes = [0_u8; 4];
            read_fully(&mut file, &mut bytes).map_err(failed)?;
            usize::try_from(u32::from_le_bytes(bytes)).unwrap_or(usize::MAX)
        }
        _ => {
            return Err(invalid(format!(
                "the .npy file has format version {major}.{minor}, which is not 1.0, 2.0 or 3.0"
            )));
        }
    };
    let mut text = Vec::new();
    (&mut file)
        .take(length as u64)
        .read_to_end(&mut text)
        .map_err(failed)?;
    if text.len() < length {
        return Err(invalid("the .npy file ends inside its header".to_owned()));
    }
    let text = String::from_utf8(text)
        .map_err(|_| invalid("the .npy file's header is not text".to_owned()))?;
    let length_bytes = if major == 1 { 2 } else { 4 };
    let offset = (start.len() + length_bytes + length) as u64;
    parse(&text, offset).map_err(invalid)
}

/// Fills `buffer` from `reader`, or fails with `UnexpectedEof` when it ends first.
fn read_fully(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<()> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "the file is not a .npy file: it ends before its header",
            )
        } else {
            error
        }
    })
}

/// The header whose dictionary is `text`, with the elements at `offset`. The error is the
/// message of the `INVALID_DATA` failure.
fn parse(text: &str, offset: u64) -> Result<Header, String> {
    let unreadable = || format!("the .npy file's header is not one this reader knows: {text}");
    let descr = value(text, "descr").ok_or_else(unreadable)?;
    let descr = descr
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .ok_or_else(|| {
            format!("the .npy file holds records or another structured type, {descr}")
        })?;
    let fortran_order = match value(text, "fortran_order").ok_or_else(unreadable)? {
        "True" => true,
        "False" => false,
        _ => return Err(unreadable()),
    };
    let shape = value(text, "shape").ok_or_else(unreadable)?;
    let lengths = shape
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(unreadable)?;
    let shape = lengths
        .split(',')
        .map(str::trim)
        .filter(|length| !length.is_empty())
        .map(|length| length.parse::<usize>().map_err(|_| unreadable()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Header {
        descr: descr.to_owned(),
        fortran_order,
        shape,
        offset,
    })
}

/// The text of the value the dictionary `text` gives `key`, from just after `'key':` to the
/// comma that ends it, outside any brackets, or to the closing brace; `None` when the key is
/// missing.
fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let quoted = format!("'{key}':");
    let start = text.find(&quoted)? + quoted.len();
    let rest = &text[start..];
    let mut depth = 0_i32;
    for (at, character) in rest.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' | '}' if depth == 0 => return Some(rest[..at].trim()),
            _ => {}
        }
    }
    None
}

/// Checks that `header` describes an array a program asked for: elements of the type `NumPy`
/// spells `descr`, in row-major order, of `rank` dimensions. A one-byte type matches with any
/// byte-order mark, since `NumPy` writes `|` for one byte and readers accept `<` and `=` too.
/// The error is the message of the `INVALID_DATA` failure.
pub(crate) fn check(header: &Header, descr: &str, rank: usize) -> Result<(), String> {
    let found = &header.descr;
    let same = found == descr || (one_byte(found) && one_byte(descr) && found[1..] == descr[1..]);
    if !same {
        return Err(format!(
            "the .npy file holds elements of type '{found}', not '{descr}' as the program asks"
        ));
    }
    if header.fortran_order {
        return Err(
            "the .npy file holds its elements in Fortran order; save it from a C-ordered array"
                .to_owned(),
        );
    }
    if header.shape.len() != rank {
        return Err(format!(
            "the .npy file holds an array of rank {}, not rank {rank} as the program asks",
            header.shape.len()
        ));
    }
    Ok(())
}

/// Whether `descr` names a one-byte type, such as `|u1` or `|b1`, whose byte order is moot.
fn one_byte(descr: &str) -> bool {
    descr.len() == 3 && matches!(&descr[..1], "|" | "<" | "=") && descr.ends_with('1')
}

#[cfg(test)]
mod tests;
