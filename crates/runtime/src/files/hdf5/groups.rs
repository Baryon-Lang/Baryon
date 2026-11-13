//! The links of a group: the names it gives its members and the addresses of their object
//! headers.
//!
//! An older group, the kind h5py writes by default, has a symbol table message naming a
//! version 1 B-tree, whose leaves are symbol table nodes (`SNOD`) of entries, and a local heap
//! (`HEAP`) that holds the entries' names. A newer group keeps its links as link messages in
//! its own header, or, past eight links, in a fractal heap indexed by a version 2 B-tree, which
//! [`super::dense`] reads.

use super::cursor::{Failure, Source, UNDEFINED, name};
use super::dense;
use super::header::Message;
use super::messages::{LINK, LINK_INFO, SYMBOL_TABLE, dense as dense_info, link};

/// A link: the member's name and the address of its object header; `None` for a soft or
/// external link, which a reader does not follow.
pub(super) type Link = (String, Option<u64>);

/// The links of the group whose header holds `messages`, in name order for an older group
/// and in the file's order for a newer one.
///
/// # Errors
///
/// A damaged B-tree, heap or link is reported.
pub(super) fn links(source: &mut Source, messages: &[Message]) -> Result<Vec<Link>, Failure> {
    if let Some(table) = messages.iter().find(|message| message.kind == SYMBOL_TABLE) {
        let mut cursor = source.cursor(&table.data);
        let tree = cursor.address()?;
        let heap = cursor.address()?;
        let names = local_heap(source, heap)?;
        let mut found = Vec::new();
        symbols(source, tree, &names, &mut found, 0)?;
        return Ok(found);
    }
    let mut found = messages
        .iter()
        .filter(|message| message.kind == LINK)
        .map(|message| link(source, &message.data))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(info) = messages.iter().find(|message| message.kind == LINK_INFO) {
        let (heap, index) = dense_info(source, &info.data, true)?;
        if heap != UNDEFINED {
            for record in dense::records(source, heap, index, true)? {
                found.push(link(source, &record)?);
            }
        }
    }
    Ok(found)
}

/// The data segment of the local heap at `address`.
fn local_heap(source: &mut Source, address: u64) -> Result<Vec<u8>, Failure> {
    let size = 8 + 2 * u64::from(source.lengths) + u64::from(source.offsets);
    let bytes = source.bytes(address, size)?;
    let mut cursor = source.cursor(&bytes);
    cursor.signature(*b"HEAP", "local heap")?;
    cursor.skip(4)?;
    let length = cursor.length()?;
    cursor.length()?;
    let data = cursor.address()?;
    source.bytes(data, length)
}

/// Adds the entries under the group B-tree node at `address` to `found`, `depth` nodes
/// below the root, naming each from `names`.
fn symbols(
    source: &mut Source,
    address: u64,
    names: &[u8],
    found: &mut Vec<Link>,
    depth: usize,
) -> Result<(), Failure> {
    let (level, children) = node(source, address, 0, depth)?;
    for child in children {
        if level > 0 {
            symbols(source, child.address, names, found, depth + 1)?;
        } else {
            entries(source, child.address, names, found)?;
        }
    }
    Ok(())
}

/// Adds the entries of the symbol table node at `address` to `found`.
fn entries(
    source: &mut Source,
    address: u64,
    names: &[u8],
    found: &mut Vec<Link>,
) -> Result<(), Failure> {
    let head = source.bytes(address, 8)?;
    let mut cursor = source.cursor(&head);
    cursor.signature(*b"SNOD", "symbol table node")?;
    cursor.skip(2)?;
    let count = u64::from(cursor.u16()?);
    let entry = 2 * u64::from(source.offsets) + 24;
    let bytes = source.bytes(address + 8, count * entry)?;
    let mut cursor = source.cursor(&bytes);
    for _ in 0..count {
        let offset = cursor.address()?;
        let header = cursor.address()?;
        cursor.skip(24)?;
        let start = usize::try_from(offset)
            .ok()
            .filter(|&start| start < names.len())
            .ok_or_else(|| "the HDF5 file names a link outside its local heap".to_owned())?;
        found.push((name(&names[start..])?, Some(header)));
    }
    Ok(())
}

/// A child of a version 1 B-tree node: its address, and the key before it, whose bytes are a
/// chunk's size, filter mask and offsets in a chunk B-tree.
pub(super) struct Child {
    /// The address of the child node, or of the chunk.
    pub(super) address: u64,
    /// The key before it.
    pub(super) key: Vec<u8>,
}

/// The level and children of the version 1 B-tree node at `address` of the type `kind`, 0 for
/// a group's and 1 for a chunk index's, `depth` nodes below the root. A chunk key is
/// `key_size` bytes; a group key is a length.
///
/// # Errors
///
/// A node of another type, a tree deeper than any file needs, or a node cut short is
/// damaged.
pub(super) fn node(
    source: &mut Source,
    address: u64,
    key_size: u64,
    depth: usize,
) -> Result<(u8, Vec<Child>), Failure> {
    if depth > 64 {
        return Err("the HDF5 file has a B-tree deeper than 64 levels".to_owned());
    }
    let head_size = 8 + 2 * u64::from(source.offsets);
    let head = source.bytes(address, head_size)?;
    let mut cursor = source.cursor(&head);
    cursor.signature(*b"TREE", "B-tree node")?;
    let kind = cursor.u8()?;
    let level = cursor.u8()?;
    let count = u64::from(cursor.u16()?);
    let key = if kind == 0 {
        u64::from(source.lengths)
    } else {
        key_size
    };
    let size = (count + 1) * key + count * u64::from(source.offsets);
    let bytes = source.bytes(address + head_size, size)?;
    let mut cursor = source.cursor(&bytes);
    let mut children = Vec::new();
    for _ in 0..count {
        let key = cursor
            .take(usize::try_from(key).unwrap_or(usize::MAX))?
            .to_vec();
        let address = cursor.address()?;
        children.push(Child { address, key });
    }
    Ok((level, children))
}
