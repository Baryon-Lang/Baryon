//! Extensible arrays, the chunk index of a dataset that can grow along one dimension.
//!
//! The header (`EAHD`) gives the sizes. The index block (`EAIB`) holds the first few entries
//! itself, then the addresses of the first data blocks (`EADB`) and of super blocks (`EASB`),
//! each of which lists more data blocks. Super block `s` lists `2^(s/2)` data blocks of
//! `2^((s+1)/2)` times the smallest block's entries each, so the array doubles as it grows; the
//! first super blocks' data blocks are listed in the index block itself.

use super::Chunk;
use super::chunks::{Entry, Grid, element};
use super::cursor::{Failure, Source, UNDEFINED, checked};

/// What an extensible array's header says.
struct Header {
    /// The bytes of one entry.
    entry: u64,
    /// The bytes of a block's offset in the array.
    offset_size: usize,
    /// The entries the index block holds itself.
    inline: u64,
    /// The entries of the smallest data block.
    smallest: u64,
    /// The data block addresses the index block holds.
    direct: u64,
    /// The super block addresses the index block holds.
    supers: u64,
    /// The super blocks whose data blocks the index block lists.
    listed: u64,
    /// Data blocks with more entries than this are paged.
    page: u64,
    /// The index block's address.
    index: u64,
}

/// The chunks of the extensible array whose header is at `address`, for a dataset of the grid
/// `grid`.
///
/// # Errors
///
/// A damaged array, or one whose data blocks are paged, which the reader does not read yet, is
/// reported.
pub(super) fn chunks(
    source: &mut Source,
    address: u64,
    grid: &Grid,
) -> Result<Vec<Chunk>, Failure> {
    let header = read(source, address)?;
    let offsets = u64::from(source.offsets);
    let size =
        6 + offsets + header.inline * header.entry + (header.direct + header.supers) * offsets + 4;
    let bytes = source.bytes(header.index, size)?;
    checked(&bytes, "extensible array index block")?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"EAIB", "extensible array index block")?;
    cursor.skip(2 + usize::from(source.offsets))?;
    let first = grid
        .most
        .iter()
        .position(|&most| most == u64::MAX)
        .unwrap_or(0);
    let counts: Vec<u64> = grid
        .most
        .iter()
        .zip(grid.chunk)
        .map(|(&most, &chunk)| {
            if most == u64::MAX {
                0
            } else {
                most.div_ceil(chunk.max(1))
            }
        })
        .collect();
    let mut found = Vec::new();
    let mut linear = 0_u64;
    let keep = |linear: u64, (address, size, mask): Entry, found: &mut Vec<Chunk>| {
        if address != UNDEFINED {
            found.push(Chunk {
                offset: grid.position(linear, &counts, first),
                address,
                size,
                mask,
            });
        }
    };
    for _ in 0..header.inline {
        keep(
            linear,
            element(&mut cursor, grid, header.entry)?,
            &mut found,
        );
        linear += 1;
    }
    let direct = (0..header.direct)
        .map(|_| cursor.address())
        .collect::<Result<Vec<_>, _>>()?;
    let supers = (0..header.supers)
        .map(|_| cursor.address())
        .collect::<Result<Vec<_>, _>>()?;
    let mut direct = direct.into_iter();
    for block in 0..header.listed + header.supers {
        let entries = header.smallest << block.div_ceil(2).min(63);
        let blocks = 1_u64 << (block / 2).min(63);
        let addresses: Vec<u64> = if block < header.listed {
            let count = usize::try_from(blocks).unwrap_or(usize::MAX);
            direct.by_ref().take(count).collect()
        } else {
            let at = supers[usize::try_from(block - header.listed).unwrap_or(usize::MAX)];
            if at == UNDEFINED {
                linear = linear.saturating_add(blocks.saturating_mul(entries));
                continue;
            }
            super_block(source, &header, at, (blocks, entries))?
        };
        for data in addresses {
            if data != UNDEFINED {
                for (at, entry) in data_block(source, &header, data, entries, grid)? {
                    keep(linear + at, entry, &mut found);
                }
            }
            linear = linear.saturating_add(entries);
        }
    }
    Ok(found)
}

/// The header at `address`.
fn read(source: &mut Source, address: u64) -> Result<Header, Failure> {
    let size = 12 + 6 * u64::from(source.lengths) + u64::from(source.offsets) + 4;
    let bytes = source.bytes(address, size)?;
    checked(&bytes, "extensible array header")?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"EAHD", "extensible array header")?;
    cursor.skip(2)?;
    let entry = u64::from(cursor.u8()?);
    let bits = u64::from(cursor.u8()?);
    let inline = u64::from(cursor.u8()?);
    let smallest = u64::from(cursor.u8()?);
    let pointers = u64::from(cursor.u8()?);
    let page_bits = u64::from(cursor.u8()?);
    for _ in 0..6 {
        cursor.length()?;
    }
    let index = cursor.address()?;
    if !smallest.is_power_of_two() || !pointers.is_power_of_two() || bits > 64 || page_bits > 63 {
        return Err("the HDF5 file has an extensible array of a shape no file needs".to_owned());
    }
    let blocks = 1 + bits.saturating_sub(u64::from(smallest.ilog2()));
    let listed = 2 * u64::from(pointers.ilog2());
    Ok(Header {
        entry,
        offset_size: usize::try_from(bits.div_ceil(8)).unwrap_or(usize::MAX),
        inline,
        smallest,
        direct: 2 * (pointers - 1),
        supers: blocks.saturating_sub(listed),
        listed,
        page: 1 << page_bits,
        index,
    })
}

/// The addresses of the `blocks` data blocks of `entries` entries each that the super block at
/// `address` lists.
///
/// # Errors
///
/// Blocks of more entries than a page holds are paged, which the reader does not read yet:
/// HDF5's default page of 1,024 entries is first passed after about 100,000 chunks along the
/// growing dimension.
fn super_block(
    source: &mut Source,
    header: &Header,
    address: u64,
    (blocks, entries): (u64, u64),
) -> Result<Vec<u64>, Failure> {
    if entries > header.page {
        return Err(
            "the HDF5 file has more chunks along a growing dimension than one page of \
                    its extensible array holds, which is not read yet"
                .to_owned(),
        );
    }
    let offsets = u64::from(source.offsets);
    let size = 6 + offsets + header.offset_size as u64 + blocks * offsets + 4;
    let bytes = source.bytes(address, size)?;
    checked(&bytes, "extensible array super block")?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"EASB", "extensible array super block")?;
    cursor.skip(2 + usize::from(source.offsets) + header.offset_size)?;
    (0..blocks).map(|_| cursor.address()).collect()
}

/// The entries of the data block at `address`, `entries` of them, each with its place in the
/// block.
fn data_block(
    source: &mut Source,
    header: &Header,
    address: u64,
    entries: u64,
    grid: &Grid,
) -> Result<Vec<(u64, Entry)>, Failure> {
    let prefix = 6 + u64::from(source.offsets) + header.offset_size as u64;
    let bytes = source.bytes(address, prefix + entries * header.entry + 4)?;
    checked(&bytes, "extensible array data block")?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"EADB", "extensible array data block")?;
    cursor.skip(usize::try_from(prefix).unwrap_or(usize::MAX) - 4)?;
    (0..entries)
        .map(|at| Ok((at, element(&mut cursor, grid, header.entry)?)))
        .collect()
}
