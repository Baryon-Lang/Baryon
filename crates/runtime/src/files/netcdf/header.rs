//! The header of a `NetCDF` classic file, read field by field: the magic bytes and version, the
//! number of records, then the lists of dimensions, global attributes and variables, as
//! Unidata's specification lays them out. A count in a corrupt header cannot make the reader
//! ask for more than [`LARGEST_HEADER`] bytes.

use super::{Attribute, Header, Kind, Values, Variable, number};
use crate::files::{BUFFER, ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Read};

/// The tag before a list of dimensions.
const DIMENSIONS: u32 = 0x0A;

/// The tag before a list of variables.
const VARIABLES: u32 = 0x0B;

/// The tag before a list of attributes.
const ATTRIBUTES: u32 = 0x0C;

/// The largest header this reader follows, in bytes: a guard against a corrupt count that
/// would otherwise ask for more memory than a file can describe.
const LARGEST_HEADER: u64 = 1 << 30;

/// Reads the header of the classic file at `path`.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the file is not a classic
/// `NetCDF` file or its header is cut short or names what does not exist.
pub(crate) fn read_header(path: &str) -> Result<Header, OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let file = fs::File::open(path).map_err(failed)?;
    let mut reader = Reader {
        bytes: io::BufReader::with_capacity(BUFFER, file),
        read: 0,
    };
    parse(&mut reader).map_err(|failure| match failure {
        Failure::Io(error) if error.kind() == io::ErrorKind::UnexpectedEof => OsFailure::new(
            ErrorKind::InvalidData,
            "the file ends inside a NetCDF header",
            path,
        ),
        Failure::Io(error) => failed(error),
        Failure::Invalid(message) => OsFailure::new(ErrorKind::InvalidData, message, path),
    })
}

/// Why reading a header failed: the operating system, or the bytes.
enum Failure {
    /// A read failed or the file ended.
    Io(io::Error),
    /// The bytes are not a header this reader follows; the message says why.
    Invalid(String),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A reader of a header's big-endian fields that counts the bytes it has read.
struct Reader<R> {
    /// Where the bytes come from.
    bytes: R,
    /// How many bytes have been read.
    read: u64,
}

impl<R: Read> Reader<R> {
    /// The next `count` bytes.
    fn take(&mut self, count: u64) -> Result<Vec<u8>, Failure> {
        self.read = self.read.saturating_add(count);
        if self.read > LARGEST_HEADER {
            return Err(Failure::Invalid(
                "the NetCDF header is larger than this reader follows".to_owned(),
            ));
        }
        let mut bytes = vec![0_u8; usize::try_from(count).unwrap_or(usize::MAX)];
        self.bytes.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    /// The next 32-bit number.
    fn u32(&mut self) -> Result<u32, Failure> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// The next 64-bit number.
    fn u64(&mut self) -> Result<u64, Failure> {
        let bytes = self.take(8)?;
        let mut word = [0_u8; 8];
        word.copy_from_slice(&bytes);
        Ok(u64::from_be_bytes(word))
    }

    /// The next `count` bytes and the padding after them to a multiple of 4.
    fn padded(&mut self, count: u64) -> Result<Vec<u8>, Failure> {
        let mut bytes = self.take(count)?;
        self.take(count.next_multiple_of(4) - count)?;
        bytes.truncate(usize::try_from(count).unwrap_or(usize::MAX));
        Ok(bytes)
    }

    /// The next name: its length, then its UTF-8 bytes, padded.
    fn name(&mut self) -> Result<String, Failure> {
        let length = self.u32()?;
        String::from_utf8(self.padded(u64::from(length))?)
            .map_err(|_| Failure::Invalid("a NetCDF name is not UTF-8 text".to_owned()))
    }

    /// The count of a list tagged `tag`: 0 for an absent list, written as two zero words.
    fn list(&mut self, tag: u32, what: &str) -> Result<u32, Failure> {
        let found = self.u32()?;
        let count = self.u32()?;
        if found == tag || (found == 0 && count == 0) {
            Ok(count)
        } else {
            Err(Failure::Invalid(format!(
                "the NetCDF header has no list of {what} where one belongs"
            )))
        }
    }

    /// A list of attributes.
    fn attributes(&mut self) -> Result<Vec<Attribute>, Failure> {
        let count = self.list(ATTRIBUTES, "attributes")?;
        (0..count).map(|_| self.attribute()).collect()
    }

    /// One attribute.
    fn attribute(&mut self) -> Result<Attribute, Failure> {
        let name = self.name()?;
        let kind = self.kind()?;
        let count = u64::from(self.u32()?);
        let bytes = self.padded(count.saturating_mul(kind.size() as u64))?;
        let values = if kind == Kind::Char {
            let end = bytes
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(bytes.len());
            Values::Text(String::from_utf8_lossy(&bytes[..end]).into_owned())
        } else {
            Values::Numbers(
                bytes
                    .chunks_exact(kind.size())
                    .map(|value| number(value, kind))
                    .collect(),
            )
        };
        Ok(Attribute { name, kind, values })
    }

    /// A value type.
    fn kind(&mut self) -> Result<Kind, Failure> {
        let number = self.u32()?;
        Kind::from_number(number).ok_or_else(|| {
            Failure::Invalid(format!("the NetCDF type {number} is not a classic one"))
        })
    }
}

/// Parses a header from its first byte.
fn parse(reader: &mut Reader<impl Read>) -> Result<Header, Failure> {
    let magic = reader.take(4)?;
    let wide = match magic[..] {
        [b'C', b'D', b'F', 1] => false,
        [b'C', b'D', b'F', 2] => true,
        [b'C', b'D', b'F', 5] => {
            return Err(Failure::Invalid(
                "the file is a 64-bit data NetCDF file (CDF-5), which is not read yet".to_owned(),
            ));
        }
        [0x89, b'H', b'D', b'F'] => {
            return Err(Failure::Invalid(
                "the file is a NetCDF-4 (HDF5) file, which is not read yet".to_owned(),
            ));
        }
        _ => {
            return Err(Failure::Invalid(
                "the file is not a NetCDF file: it does not start with CDF".to_owned(),
            ));
        }
    };
    let records = match reader.u32()? {
        u32::MAX => {
            return Err(Failure::Invalid(
                "the NetCDF file is still being written (its number of records is not set), \
                 which is not read yet"
                    .to_owned(),
            ));
        }
        records => u64::from(records),
    };
    let count = reader.list(DIMENSIONS, "dimensions")?;
    let mut dimensions = Vec::new();
    for _ in 0..count {
        let name = reader.name()?;
        dimensions.push((name, u64::from(reader.u32()?)));
    }
    let attributes = reader.attributes()?;
    let count = reader.list(VARIABLES, "variables")?;
    let mut variables = Vec::new();
    for _ in 0..count {
        let name = reader.name()?;
        let rank = reader.u32()?;
        let mut spans = Vec::new();
        for _ in 0..rank {
            let index = usize::try_from(reader.u32()?).unwrap_or(usize::MAX);
            if index >= dimensions.len() {
                return Err(Failure::Invalid(format!(
                    "the NetCDF variable {name} names a dimension the file does not have"
                )));
            }
            spans.push(index);
        }
        let attributes = reader.attributes()?;
        let kind = reader.kind()?;
        let size = u64::from(reader.u32()?);
        let begin = if wide {
            reader.u64()?
        } else {
            u64::from(reader.u32()?)
        };
        variables.push(Variable {
            name,
            dimensions: spans,
            attributes,
            kind,
            begin,
            size,
        });
    }
    Ok(Header {
        records,
        dimensions,
        attributes,
        variables,
    })
}
