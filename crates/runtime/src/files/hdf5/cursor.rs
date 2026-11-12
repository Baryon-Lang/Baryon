//! Reading the HDF5 file's structures: the file's bytes by address, and a cursor that takes
//! the little-endian fields of one structure in order.
//!
//! Every multi-byte field of an HDF5 structure is little-endian. Addresses ("offsets") and
//! lengths have the sizes the superblock gives, 8 bytes each in every file h5py writes, and an
//! address of all one bits is the undefined address, which says a structure is absent.

use super::super::{ErrorKind, OsFailure};
use std::fs;
use std::io::{Read, Seek, SeekFrom};

/// The undefined address: no structure there.
pub(super) const UNDEFINED: u64 = u64::MAX;

/// A failure while reading the file's structures: the message an `OSError` carries.
pub(super) type Failure = String;

/// The open file, read by address.
pub(super) struct Source {
    /// The file.
    file: fs::File,
    /// Its path, for the messages of an I/O failure.
    path: String,
    /// The size of an address in bytes, from the superblock.
    pub(super) offsets: u8,
    /// The size of a length in bytes, from the superblock.
    pub(super) lengths: u8,
    /// The address every other address counts from, the superblock's own unless it says
    /// otherwise.
    pub(super) base: u64,
}

impl Source {
    /// Opens the file at `path`, whose structures are read with 8-byte addresses and lengths
    /// until the superblock gives other sizes.
    pub(super) fn open(path: &str) -> Result<Self, OsFailure> {
        let file = fs::File::open(path).map_err(|error| OsFailure::from_io(&error, path))?;
        Ok(Self {
            file,
            path: path.to_owned(),
            offsets: 8,
            lengths: 8,
            base: 0,
        })
    }

    /// The `OSError` for the file with the message `failure`.
    pub(super) fn invalid(&self, failure: Failure) -> OsFailure {
        OsFailure {
            kind: ErrorKind::InvalidData,
            errno: 0,
            strerror: failure,
            filename: self.path.clone(),
        }
    }

    /// `length` bytes at `address`, counted from the base address; a file that ends first is
    /// damaged.
    pub(super) fn bytes(&mut self, address: u64, length: u64) -> Result<Vec<u8>, Failure> {
        let at = address
            .checked_add(self.base)
            .ok_or_else(|| format!("the HDF5 file has an address past its end, {address}"))?;
        let size = usize::try_from(length)
            .map_err(|_| format!("the HDF5 file has a structure of {length} bytes"))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| format!("the HDF5 file has a structure of {length} bytes"))?;
        bytes.resize(size, 0);
        self.file
            .seek(SeekFrom::Start(at))
            .and_then(|_| self.file.read_exact(&mut bytes))
            .map_err(|_| format!("the HDF5 file ends inside the structure at address {address}"))?;
        Ok(bytes)
    }

    /// Up to `length` bytes at the absolute position `at`, fewer where the file ends: what the
    /// superblock search reads.
    pub(super) fn some_bytes(&mut self, at: u64, length: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(length);
        if self.file.seek(SeekFrom::Start(at)).is_ok() {
            // A read that fails leaves fewer bytes, which the caller treats as no superblock.
            let _ = (&mut self.file).take(length as u64).read_to_end(&mut bytes);
        }
        bytes
    }

    /// A cursor over `bytes` read with this file's address and length sizes.
    pub(super) fn cursor<'a>(&self, bytes: &'a [u8]) -> Cursor<'a> {
        Cursor {
            bytes,
            at: 0,
            offsets: self.offsets,
            lengths: self.lengths,
        }
    }
}

/// The fields of one structure, taken in order.
pub(super) struct Cursor<'a> {
    /// The structure's bytes.
    bytes: &'a [u8],
    /// Where the next field begins.
    at: usize,
    /// The size of an address in bytes.
    offsets: u8,
    /// The size of a length in bytes.
    lengths: u8,
}

impl<'a> Cursor<'a> {
    /// A cursor over `bytes` for a structure that holds no address or length.
    pub(super) fn plain(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            offsets: 8,
            lengths: 8,
        }
    }

    /// The next `count` bytes.
    pub(super) fn take(&mut self, count: usize) -> Result<&'a [u8], Failure> {
        let end = self
            .at
            .checked_add(count)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| "the HDF5 file has a structure cut short".to_owned())?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    /// The next `size` bytes as a little-endian unsigned number, `size` at most 8.
    pub(super) fn sized(&mut self, size: usize) -> Result<u64, Failure> {
        let bytes = self.take(size)?;
        Ok(bytes
            .iter()
            .rev()
            .fold(0, |value, &byte| (value << 8) | u64::from(byte)))
    }

    /// The next byte.
    pub(super) fn u8(&mut self) -> Result<u8, Failure> {
        Ok(self.take(1)?[0])
    }

    /// The next two bytes as a number.
    pub(super) fn u16(&mut self) -> Result<u16, Failure> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// The next four bytes as a number.
    pub(super) fn u32(&mut self) -> Result<u32, Failure> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// The next eight bytes as a number.
    pub(super) fn u64(&mut self) -> Result<u64, Failure> {
        self.sized(8)
    }

    /// The next address, [`UNDEFINED`] when all its bits are set.
    pub(super) fn address(&mut self) -> Result<u64, Failure> {
        let size = self.offsets;
        let value = self.sized(usize::from(size))?;
        let all = if size >= 8 {
            u64::MAX
        } else {
            (1 << (8 * u32::from(size))) - 1
        };
        Ok(if value == all { UNDEFINED } else { value })
    }

    /// The next length.
    pub(super) fn length(&mut self) -> Result<u64, Failure> {
        self.sized(usize::from(self.lengths))
    }

    /// Skips `count` bytes.
    pub(super) fn skip(&mut self, count: usize) -> Result<(), Failure> {
        self.take(count).map(|_| ())
    }

    /// Skips to the next multiple of `alignment` bytes from the structure's start.
    pub(super) fn align(&mut self, alignment: usize) -> Result<(), Failure> {
        let past = self.at % alignment;
        if past == 0 {
            return Ok(());
        }
        self.skip(alignment - past)
    }

    /// Checks that the next bytes are the signature `signature` of the structure `what`.
    pub(super) fn signature(&mut self, signature: [u8; 4], what: &str) -> Result<(), Failure> {
        if self.take(4)? == signature {
            Ok(())
        } else {
            Err(format!("the HDF5 file has no {what} where one should be"))
        }
    }

    /// Where the next field begins.
    pub(super) fn position(&self) -> usize {
        self.at
    }

    /// The bytes not yet taken.
    pub(super) fn rest(&self) -> &'a [u8] {
        &self.bytes[self.at..]
    }

    /// The size of an address in bytes.
    pub(super) fn offsets(&self) -> u8 {
        self.offsets
    }
}

/// The text of a name ending at its first zero byte, or at the end of `bytes`.
pub(super) fn name(bytes: &[u8]) -> Result<String, Failure> {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8(bytes[..end].to_vec())
        .map_err(|_| "the HDF5 file has a name that is not UTF-8".to_owned())
}

/// Jenkins's `lookup3` hash of `bytes`, `hashlittle` with the initial value 0, which checks
/// every structure of the newer format versions.
pub(super) fn checksum(bytes: &[u8]) -> u32 {
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let mut a = 0xdead_beef_u32.wrapping_add(length);
    let mut b = a;
    let mut c = a;
    let word = |chunk: &[u8]| {
        chunk
            .iter()
            .enumerate()
            .fold(0_u32, |value, (index, &byte)| {
                value | (u32::from(byte) << (8 * index))
            })
    };
    let mut rest = bytes;
    while rest.len() > 12 {
        a = a.wrapping_add(word(&rest[0..4]));
        b = b.wrapping_add(word(&rest[4..8]));
        c = c.wrapping_add(word(&rest[8..12]));
        (a, b, c) = mix(a, b, c);
        rest = &rest[12..];
    }
    if rest.is_empty() {
        return c;
    }
    let mut tail = [0_u8; 12];
    tail[..rest.len()].copy_from_slice(rest);
    a = a.wrapping_add(word(&tail[0..4]));
    b = b.wrapping_add(word(&tail[4..8]));
    c = c.wrapping_add(word(&tail[8..12]));
    finish(a, b, c)
}

/// `lookup3`'s `mix` of three words.
fn mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    a = a.wrapping_sub(c);
    a ^= c.rotate_left(4);
    c = c.wrapping_add(b);
    b = b.wrapping_sub(a);
    b ^= a.rotate_left(6);
    a = a.wrapping_add(c);
    c = c.wrapping_sub(b);
    c ^= b.rotate_left(8);
    b = b.wrapping_add(a);
    a = a.wrapping_sub(c);
    a ^= c.rotate_left(16);
    c = c.wrapping_add(b);
    b = b.wrapping_sub(a);
    b ^= a.rotate_left(19);
    a = a.wrapping_add(c);
    c = c.wrapping_sub(b);
    c ^= b.rotate_left(4);
    b = b.wrapping_add(a);
    (a, b, c)
}

/// `lookup3`'s `final` mix, which gives the hash in `c`.
fn finish(mut a: u32, mut b: u32, mut c: u32) -> u32 {
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(14));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(11));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(25));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(16));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(4));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(14));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(24));
    c
}

/// Checks the checksum in the last four bytes of `bytes`, the structure `what`.
pub(super) fn checked(bytes: &[u8], what: &str) -> Result<(), Failure> {
    let Some(split) = bytes.len().checked_sub(4) else {
        return Err(format!("the HDF5 file has a {what} cut short"));
    };
    let (body, stored) = bytes.split_at(split);
    if checksum(body).to_le_bytes() == stored {
        Ok(())
    } else {
        Err(format!(
            "the HDF5 file has a {what} whose checksum does not match"
        ))
    }
}
