//! Version 2 B-trees, which index a newer group's links and attributes by name and a newer
//! dataset's chunks by position.
//!
//! The header (`BTHD`) gives the node size, the record size, the tree's depth and the root.
//! A leaf (`BTLF`) holds records; an internal node (`BTIN`) holds records and, after them, one
//! pointer per child, each an address, the child's record count and, two or more levels up, the
//! count of records below it. Those counts are stored in as few bytes as their largest possible
//! value needs, which the reader works out from the node size as the HDF5 library does.

use super::cursor::{Failure, Source, checked};

/// The bytes before a node's records and its checksum: signature, version and type.
const PREFIX: u64 = 4 + 1 + 1 + 4;

/// Every record of the version 2 B-tree whose header is at `address`, in the tree's order.
///
/// # Errors
///
/// A damaged header or node, or a tree deeper than any file needs, is reported.
pub(super) fn records(source: &mut Source, address: u64) -> Result<Vec<Vec<u8>>, Failure> {
    let size =
        4 + 1 + 1 + 4 + 2 + 2 + 2 + u64::from(source.offsets) + 2 + u64::from(source.lengths) + 4;
    let bytes = source.bytes(address, size)?;
    checked(&bytes, "B-tree header")?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"BTHD", "B-tree header")?;
    cursor.skip(2)?;
    let node_size = u64::from(cursor.u32()?);
    let record = u64::from(cursor.u16()?);
    let depth = cursor.u16()?;
    cursor.skip(2)?;
    let root = cursor.address()?;
    let count = cursor.u16()?;
    if depth > 32 || record == 0 {
        return Err("the HDF5 file has a B-tree of a shape no file needs".to_owned());
    }
    let shape = Shape::new(source, node_size, record, depth)?;
    let mut found = Vec::new();
    shape.node(source, root, usize::from(count), depth, &mut found)?;
    Ok(found)
}

/// What a tree's nodes look like at each level.
struct Shape {
    /// The size of every node in bytes.
    node_size: u64,
    /// The size of one record in bytes.
    record: u64,
    /// The bytes of a child's record count, the same at every level.
    count_size: usize,
    /// For each level, the bytes of the count of all records below a node there.
    total_sizes: Vec<usize>,
}

impl Shape {
    /// Works out the field sizes of a tree `depth` levels deep, as the HDF5 library does.
    fn new(source: &Source, node_size: u64, record: u64, depth: u16) -> Result<Self, Failure> {
        let leaf = node_size
            .checked_sub(PREFIX)
            .map(|space| space / record)
            .filter(|&leaf| leaf > 0)
            .ok_or_else(|| "the HDF5 file has a B-tree whose nodes hold no record".to_owned())?;
        let count_size = encoded_size(leaf);
        let mut totals = vec![leaf];
        let mut total_sizes = vec![0];
        for level in 1..=usize::from(depth) {
            let pointer = u64::from(source.offsets)
                + count_size as u64
                + if level > 1 {
                    total_sizes[level - 1] as u64
                } else {
                    0
                };
            let most = node_size.saturating_sub(PREFIX + pointer) / (record + pointer);
            let total = (most + 1)
                .saturating_mul(totals[level - 1])
                .saturating_add(most);
            totals.push(total);
            total_sizes.push(encoded_size(total));
        }
        Ok(Self {
            node_size,
            record,
            count_size,
            total_sizes,
        })
    }

    /// Adds the records of the node at `address`, which holds `count` records `level` levels
    /// above the leaves, and of every node below it, to `found`.
    fn node(
        &self,
        source: &mut Source,
        address: u64,
        count: usize,
        level: u16,
        found: &mut Vec<Vec<u8>>,
    ) -> Result<(), Failure> {
        let bytes = source.bytes(address, self.node_size)?;
        let (signature, what) = if level == 0 {
            (b"BTLF", "B-tree leaf")
        } else {
            (b"BTIN", "B-tree node")
        };
        let mut cursor = source.cursor(&bytes);
        cursor.signature(*signature, what)?;
        cursor.skip(2)?;
        let record = usize::try_from(self.record).unwrap_or(usize::MAX);
        let records = (0..count)
            .map(|_| cursor.take(record).map(<[u8]>::to_vec))
            .collect::<Result<Vec<_>, _>>()?;
        if level == 0 {
            let end = cursor.position() + 4;
            checked(&bytes[..end.min(bytes.len())], what)?;
            found.extend(records);
            return Ok(());
        }
        let mut children = Vec::new();
        for _ in 0..=count {
            let child = cursor.address()?;
            let below = cursor.sized(self.count_size)?;
            if level > 1 {
                cursor.sized(self.total_sizes[usize::from(level) - 1])?;
            }
            children.push((child, usize::try_from(below).unwrap_or(usize::MAX)));
        }
        let end = cursor.position() + 4;
        checked(&bytes[..end.min(bytes.len())], what)?;
        let mut records = records.into_iter();
        for (child, below) in children {
            self.node(source, child, below, level - 1, found)?;
            found.extend(records.next());
        }
        Ok(())
    }
}

/// The bytes needed for any number up to `most`, as the HDF5 library counts them.
fn encoded_size(most: u64) -> usize {
    (most.max(1).ilog2() / 8 + 1) as usize
}
