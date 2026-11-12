//! Dense storage: the links or attributes of an object that has more than eight, kept as
//! messages in a fractal heap and found through a version 2 B-tree of their names.
//!
//! A fractal heap (`FRHP`) stores objects in blocks laid out as a doubling table: rows of
//! `width` blocks, the first two rows of the starting block size and each later row twice the
//! size of the one before. A heap ID names an object by its offset in that table and its
//! length, or, for a tiny object, holds the object itself. The root is one direct block
//! (`FHDB`) of objects, or an indirect block (`FHIB`) that lists the direct blocks.

use super::btree2;
use super::cursor::{Failure, Source, UNDEFINED, checked};

/// The objects the B-tree at `index` names in the fractal heap at `heap`: link messages when
/// `links` is set, else attribute messages.
///
/// # Errors
///
/// A damaged heap or tree, or a heap object stored as the reader does not read yet (a huge
/// object, or a block below a second level of indirect blocks), is reported.
pub(super) fn records(
    source: &mut Source,
    heap: u64,
    index: u64,
    links: bool,
) -> Result<Vec<Vec<u8>>, Failure> {
    let heap = Heap::read(source, heap)?;
    btree2::records(source, index)?
        .iter()
        .map(|record| {
            // A link's record is a 4-byte hash and then the heap ID; an attribute's is the
            // heap ID first.
            let id = if links {
                record.get(4..)
            } else {
                record.get(..heap.id_length)
            };
            let id = id.ok_or_else(|| "the HDF5 file has a B-tree record cut short".to_owned())?;
            heap.object(source, id)
        })
        .collect()
}

/// What a fractal heap's header says.
struct Heap {
    /// The length of a heap ID in bytes.
    id_length: usize,
    /// The number of blocks in each row.
    width: u64,
    /// The size of the blocks of the first two rows.
    start: u64,
    /// The largest direct block.
    largest: u64,
    /// The bytes of a block's offset in the heap's address space.
    offset_size: usize,
    /// The bytes of an object's length in a heap ID.
    length_size: usize,
    /// The root block.
    root: u64,
    /// The rows of the root indirect block; 0 when the root is a direct block.
    rows: u64,
}

impl Heap {
    /// Reads the header at `address`.
    fn read(source: &mut Source, address: u64) -> Result<Self, Failure> {
        let (offsets, lengths) = (u64::from(source.offsets), u64::from(source.lengths));
        let size = 4
            + 1
            + 2
            + 2
            + 1
            + 4
            + lengths
            + offsets
            + lengths
            + offsets
            + 8 * lengths
            + 2
            + 2 * lengths
            + 2
            + 2
            + offsets
            + 2
            + 4;
        let bytes = source.bytes(address, size)?;
        let mut cursor = source.cursor(&bytes);
        cursor.signature(*b"FRHP", "fractal heap")?;
        cursor.skip(1)?;
        let id_length = usize::from(cursor.u16()?);
        if cursor.u16()? != 0 {
            return Err("the HDF5 file has a filtered fractal heap, which is not read yet".into());
        }
        cursor.u8()?;
        let most = u64::from(cursor.u32()?);
        cursor.length()?;
        cursor.address()?;
        cursor.length()?;
        cursor.address()?;
        for _ in 0..8 {
            cursor.length()?;
        }
        let width = u64::from(cursor.u16()?);
        let start = cursor.length()?;
        let largest = cursor.length()?;
        let bits = cursor.u16()?;
        cursor.u16()?;
        let root = cursor.address()?;
        let rows = u64::from(cursor.u16()?);
        checked(&bytes[..cursor.position() + 4], "fractal heap")?;
        if width == 0 || start == 0 || !start.is_power_of_two() || largest < start {
            return Err("the HDF5 file has a fractal heap of a shape no file needs".to_owned());
        }
        Ok(Self {
            id_length,
            width,
            start,
            largest,
            offset_size: usize::from(bits).div_ceil(8),
            length_size: (largest.ilog2() as usize).div_ceil(8).min(bytes_for(most)),
            root,
            rows,
        })
    }

    /// The object the heap ID `id` names.
    fn object(&self, source: &mut Source, id: &[u8]) -> Result<Vec<u8>, Failure> {
        let first = *id
            .first()
            .ok_or_else(|| "the HDF5 file has an empty heap ID".to_owned())?;
        match (first >> 4) & 3 {
            0 => {}
            2 => {
                let length = usize::from(first & 0x0F) + 1;
                return id
                    .get(1..=length)
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| "the HDF5 file has a heap ID cut short".to_owned());
            }
            _ => {
                return Err(
                    "the HDF5 file has a huge fractal heap object, which is not read yet"
                        .to_owned(),
                );
            }
        }
        let mut cursor = source.cursor(id);
        cursor.skip(1)?;
        let offset = cursor.sized(self.offset_size)?;
        let length = cursor.sized(self.length_size)?;
        let (block, start) = self.block(source, offset)?;
        let bytes = source.bytes(block, offset - start + length)?;
        let prefix = 5 + usize::from(source.offsets) + self.offset_size;
        if bytes.len() < prefix || bytes[..4] != *b"FHDB" {
            return Err("the HDF5 file has no fractal heap block where one should be".to_owned());
        }
        Ok(bytes[usize::try_from(offset - start).unwrap_or(usize::MAX)..].to_vec())
    }

    /// The address of the direct block that holds the heap offset `offset`, and the offset
    /// where that block begins.
    fn block(&self, source: &mut Source, offset: u64) -> Result<(u64, u64), Failure> {
        if self.root == UNDEFINED {
            return Err("the HDF5 file has an empty fractal heap".to_owned());
        }
        if self.rows == 0 {
            return Ok((self.root, 0));
        }
        let direct_rows = self.largest.ilog2() - self.start.ilog2() + 2;
        let direct = self.rows.min(u64::from(direct_rows));
        let (mut row_start, mut index) = (0_u64, 0_u64);
        for row in 0..direct {
            let size = if row < 2 {
                self.start
            } else {
                self.start << (row - 1)
            };
            let row_size = size * self.width;
            if offset < row_start + row_size {
                index += (offset - row_start) / size;
                let start = row_start + (offset - row_start) / size * size;
                return Ok((self.entry(source, index)?, start));
            }
            row_start += row_size;
            index += self.width;
        }
        Err(
            "the HDF5 file has a fractal heap object below a nested indirect block, which is not \
             read yet"
                .to_owned(),
        )
    }

    /// The address of the `index`th direct block of the root indirect block.
    fn entry(&self, source: &mut Source, index: u64) -> Result<u64, Failure> {
        let offsets = u64::from(source.offsets);
        let at = self.root + 5 + offsets + self.offset_size as u64 + index * offsets;
        let bytes = source.bytes(at, offsets)?;
        let address = source.cursor(&bytes).address()?;
        if address == UNDEFINED {
            return Err("the HDF5 file names a fractal heap block that is not there".to_owned());
        }
        Ok(address)
    }
}

/// The bytes needed for any number up to `limit`, as the HDF5 library counts them.
fn bytes_for(limit: u64) -> usize {
    (limit.max(1).ilog2() / 8 + 1) as usize
}
