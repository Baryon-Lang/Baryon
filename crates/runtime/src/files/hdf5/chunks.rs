//! The chunks of a chunked dataset: where each is stored, how many bytes it takes after its
//! filters, and which filters were skipped for it.
//!
//! h5py's default files index chunks with a version 1 B-tree whose keys hold each chunk's
//! position. Files written with `libver="latest"` choose the index by the dataset's shape: one
//! chunk alone, a fixed array when no dimension can grow, an extensible array when one can, and
//! a version 2 B-tree when several can. An array lists the chunks in order: the dimension that
//! can grow slowest, then the others in the dataset's order.

use super::cursor::{Cursor, Failure, Source, UNDEFINED, checked};
use super::groups::node;
use super::{Chunk, Index, btree2};

/// What the chunk index needs to know about the dataset.
pub(super) struct Grid<'a> {
    /// The dataset's shape.
    pub(super) shape: &'a [u64],
    /// The largest shape it may grow to, `u64::MAX` along a dimension without limit.
    pub(super) most: &'a [u64],
    /// The chunk's shape.
    pub(super) chunk: &'a [u64],
    /// The size of one element in bytes.
    pub(super) element: u64,
    /// Whether the dataset has filters, which changes how array entries are stored.
    pub(super) filtered: bool,
}

impl Grid<'_> {
    /// The bytes of one chunk before its filters.
    fn chunk_bytes(&self) -> u64 {
        self.chunk.iter().product::<u64>() * self.element
    }

    /// The number of chunks along each dimension of `shape`.
    fn counts(&self, shape: &[u64]) -> Vec<u64> {
        shape
            .iter()
            .zip(self.chunk)
            .map(|(&length, &chunk)| length.div_ceil(chunk.max(1)))
            .collect()
    }

    /// The position of the chunk at the `linear`th place of an array index, in elements, with
    /// the chunk counts `counts` and the dimension `first` taken slowest.
    pub(super) fn position(&self, mut linear: u64, counts: &[u64], first: usize) -> Vec<u64> {
        let mut order: Vec<usize> = (0..counts.len()).filter(|&axis| axis != first).collect();
        if first < counts.len() {
            order.insert(0, first);
        }
        let mut scaled = vec![0; counts.len()];
        for &axis in order.iter().skip(1).rev() {
            let count = counts[axis].max(1);
            scaled[axis] = linear % count;
            linear /= count;
        }
        if let Some(&axis) = order.first() {
            scaled[axis] = linear;
        }
        scaled
            .iter()
            .zip(self.chunk)
            .map(|(&at, &chunk)| at * chunk)
            .collect()
    }
}

/// Every chunk the index `index` lists for a dataset of the grid `grid`; chunks never written
/// are left out, and read as the fill value.
///
/// # Errors
///
/// A damaged index, or an index stored as the reader does not read yet, is reported.
pub(super) fn chunks(
    source: &mut Source,
    index: &Index,
    grid: &Grid,
) -> Result<Vec<Chunk>, Failure> {
    let bytes = grid.chunk_bytes();
    match *index {
        Index::Tree(address) => {
            let mut found = Vec::new();
            if address != UNDEFINED {
                tree(source, address, grid, &mut found, 0)?;
            }
            Ok(found)
        }
        Index::Single { address, filtered } => Ok(if address == UNDEFINED {
            Vec::new()
        } else {
            let (size, mask) = filtered.unwrap_or((bytes, 0));
            vec![Chunk {
                offset: vec![0; grid.shape.len()],
                address,
                size,
                mask,
            }]
        }),
        Index::Implicit(address) => {
            let counts = grid.counts(grid.most);
            let total: u64 = counts.iter().product();
            Ok((0..total)
                .map(|linear| Chunk {
                    offset: grid.position(linear, &counts, 0),
                    address: address + linear * bytes,
                    size: bytes,
                    mask: 0,
                })
                .collect())
        }
        Index::Fixed(address) => fixed(source, address, grid),
        Index::Extensible(address) => super::extensible::chunks(source, address, grid),
        Index::Tree2(address) => btree2::records(source, address)?
            .iter()
            .map(|record| {
                let mut cursor = source.cursor(record);
                let address = cursor.address()?;
                let (size, mask) = if grid.filtered {
                    let width =
                        record.len() - usize::from(source.offsets) - 4 - 8 * grid.shape.len();
                    (cursor.sized(width)?, cursor.u32()?)
                } else {
                    (bytes, 0)
                };
                let offset = grid
                    .chunk
                    .iter()
                    .map(|&chunk| cursor.u64().map(|scaled| scaled * chunk))
                    .collect::<Result<_, _>>()?;
                Ok(Chunk {
                    offset,
                    address,
                    size,
                    mask,
                })
            })
            .collect(),
    }
}

/// Adds the chunks under the version 1 B-tree node at `address` to `found`.
fn tree(
    source: &mut Source,
    address: u64,
    grid: &Grid,
    found: &mut Vec<Chunk>,
    depth: usize,
) -> Result<(), Failure> {
    let rank = grid.shape.len();
    let key = 8 + 8 * (rank as u64 + 1);
    let (level, children) = node(source, address, key, depth)?;
    for child in children {
        if level > 0 {
            tree(source, child.address, grid, found, depth + 1)?;
            continue;
        }
        let mut cursor = Cursor::plain(&child.key);
        let size = u64::from(cursor.u32()?);
        let mask = cursor.u32()?;
        let offset = (0..rank).map(|_| cursor.u64()).collect::<Result<_, _>>()?;
        found.push(Chunk {
            offset,
            address: child.address,
            size,
            mask,
        });
    }
    Ok(())
}

/// The chunks of a fixed array index whose header is at `address`.
fn fixed(source: &mut Source, address: u64, grid: &Grid) -> Result<Vec<Chunk>, Failure> {
    let offsets = u64::from(source.offsets);
    let size = 4 + 1 + 1 + 1 + 1 + u64::from(source.lengths) + offsets + 4;
    let header = source.bytes(address, size)?;
    checked(&header, "fixed array header")?;
    let mut cursor = source.cursor(&header);
    cursor.signature(*b"FAHD", "fixed array header")?;
    cursor.skip(2)?;
    let entry = u64::from(cursor.u8()?);
    let page_bits = cursor.u8()?;
    let count = cursor.length()?;
    let block = cursor.address()?;
    if page_bits < 64 && count > 1 << page_bits {
        return Err(
            "the HDF5 file has a paged fixed array of chunks, which is not read yet".to_owned(),
        );
    }
    let prefix = 4 + 1 + 1 + offsets;
    let bytes = source.bytes(block, prefix + count * entry + 4)?;
    checked(&bytes, "fixed array data block")?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"FADB", "fixed array data block")?;
    cursor.skip(2 + usize::from(source.offsets))?;
    let counts = grid.counts(grid.most);
    let mut found = Vec::new();
    for linear in 0..count {
        let (address, size, mask) = element(&mut cursor, grid, entry)?;
        if address != UNDEFINED {
            found.push(Chunk {
                offset: grid.position(linear, &counts, 0),
                address,
                size,
                mask,
            });
        }
    }
    Ok(found)
}

/// A chunk's address, its size after its filters and its filter mask.
pub(super) type Entry = (u64, u64, u32);

/// One entry of a fixed or extensible array, `entry` bytes: a chunk's address, and, when the
/// dataset has filters, its size after them and its filter mask.
pub(super) fn element(cursor: &mut Cursor<'_>, grid: &Grid, entry: u64) -> Result<Entry, Failure> {
    let address = cursor.address()?;
    if !grid.filtered {
        return Ok((address, grid.chunk_bytes(), 0));
    }
    let width = usize::try_from(entry)
        .ok()
        .and_then(|entry| entry.checked_sub(usize::from(cursor.offsets()) + 4))
        .filter(|&width| width <= 8)
        .ok_or_else(|| {
            "the HDF5 file has a chunk index entry of a size no file needs".to_owned()
        })?;
    Ok((address, cursor.sized(width)?, cursor.u32()?))
}
