//! The superblock, which begins an HDF5 file and points to the root group.
//!
//! The superblock begins with the eight signature bytes `\x89HDF\r\n\x1a\n`, at byte 0 or, in a
//! file with a user block in front, at 512, 1024, 2048 or the next power of two. Versions 0 and
//! 1, which h5py writes by default, give the sizes of addresses and lengths and end with the
//! root group's symbol table entry. Versions 2 and 3, which h5py writes with
//! `libver="latest"`, give the sizes, then the root group's object header address, and end with
//! a checksum.

use super::cursor::{Failure, Source, checked};

/// The signature that begins a superblock.
const SIGNATURE: [u8; 8] = *b"\x89HDF\r\n\x1a\n";

/// Finds and reads the superblock: sets the file's address and length sizes and its base
/// address on `source`, and gives the address of the root group's object header.
///
/// # Errors
///
/// A file with no signature at any place the format allows is not an HDF5 file; a superblock
/// of a version the format does not define, or one whose checksum does not match, is damaged.
pub(super) fn read_superblock(source: &mut Source) -> Result<u64, Failure> {
    let mut at = 0_u64;
    let bytes = loop {
        let bytes = source.some_bytes(at, 96);
        if bytes.len() < SIGNATURE.len() {
            return Err("the file is not an HDF5 file".to_owned());
        }
        if bytes[..SIGNATURE.len()] == SIGNATURE {
            break bytes;
        }
        at = if at == 0 { 512 } else { at * 2 };
    };
    let version = bytes[8];
    match version {
        0 | 1 => early(source, &bytes, version, at),
        2 | 3 => late(source, &bytes, at),
        _ => Err(format!(
            "the HDF5 file has a superblock of version {version}, which the format does not define"
        )),
    }
}

/// The rest of a version 0 or 1 superblock, found at `at`: its sizes, then the root group's
/// symbol table entry.
fn early(source: &mut Source, bytes: &[u8], version: u8, at: u64) -> Result<u64, Failure> {
    let mut cursor = source.cursor(bytes);
    // The signature, the version, and the versions of the free-space storage, the root
    // group's symbol table entry and the shared header message format, with a reserved byte.
    cursor.skip(13)?;
    let offsets = cursor.u8()?;
    let lengths = cursor.u8()?;
    sizes(offsets, lengths)?;
    // A reserved byte, the group leaf and internal node sizes and the consistency flags; then,
    // in version 1, the indexed storage internal node size and two reserved bytes.
    cursor.skip(1 + 2 + 2 + 4)?;
    if version == 1 {
        cursor.skip(4)?;
    }
    source.offsets = offsets;
    source.lengths = lengths;
    let mut cursor = source.cursor(cursor.rest());
    let base = cursor.address()?;
    // The free-space, end-of-file and driver information addresses, then the root entry's
    // link name offset.
    for _ in 0..4 {
        cursor.address()?;
    }
    let root = cursor.address()?;
    source.base = if base == 0 { at } else { base };
    Ok(root)
}

/// The rest of a version 2 or 3 superblock, found at `at`: its sizes and the root group's
/// object header address, checked against the checksum.
fn late(source: &mut Source, bytes: &[u8], at: u64) -> Result<u64, Failure> {
    let offsets = bytes.get(9).copied().unwrap_or(0);
    let lengths = bytes.get(10).copied().unwrap_or(0);
    sizes(offsets, lengths)?;
    let size = 12 + 4 * usize::from(offsets) + 4;
    let structure = bytes
        .get(..size)
        .ok_or_else(|| "the HDF5 file ends inside its superblock".to_owned())?;
    checked(structure, "superblock")?;
    source.offsets = offsets;
    source.lengths = lengths;
    let mut cursor = source.cursor(&structure[12..]);
    let base = cursor.address()?;
    // The superblock extension and end-of-file addresses.
    cursor.address()?;
    cursor.address()?;
    let root = cursor.address()?;
    source.base = if base == 0 { at } else { base };
    Ok(root)
}

/// Checks the sizes of addresses and lengths a superblock gives: 2, 4 or 8 bytes.
fn sizes(offsets: u8, lengths: u8) -> Result<(), Failure> {
    for size in [offsets, lengths] {
        if !matches!(size, 2 | 4 | 8) {
            return Err(format!(
                "the HDF5 file gives its addresses or lengths {size} bytes, not 2, 4 or 8"
            ));
        }
    }
    Ok(())
}
