//! Groups as h5py writes them by default: *symbol table* groups. A group's object header holds
//! one symbol table message, pointing to a B-tree and a local heap. The local heap holds the
//! members' names; the B-tree's leaves are *symbol table nodes*, each listing up to 8 members
//! in name order, with the address of each member's object header (`H5G` and `H5HL` in the
//! HDF5 library 2.0).

use super::btree::{Btree, Changed, Class, Insert};
use super::header::{align8, to_usize};
use super::space::{Space, Use};
use std::collections::BTreeMap;

/// The B-tree's `K` for groups: a node holds up to 32 children.
const GROUP_K: usize = 16;
/// The symbol table node's `K`: a node lists up to 8 members.
const LEAF_K: usize = 4;
/// The size of a group B-tree node: header, 33 keys and 32 children of 8 bytes.
const NODE_SIZE: u64 = 24 + (2 * GROUP_K as u64 + 1) * 8 + 2 * GROUP_K as u64 * 8;
/// The size of a symbol table node: header and 8 entries of 40 bytes.
const LEAF_SIZE: u64 = 8 + 2 * LEAF_K as u64 * 40;
/// The size of a free block's record in a local heap: the next block's offset and the size.
const FREE_RECORD: u64 = 16;
/// The size of a local heap's prefix.
const HEAP_PREFIX: u64 = 32;
/// The data a new group's local heap starts with: room for 4 short names and a free record
/// (`H5G_SIZE_HINT` handling in `H5G__stab_create`).
const HEAP_HINT: u64 = 8 + 4 * 16 + FREE_RECORD;

/// A local heap: a group's members' names.
#[derive(Clone, Debug)]
pub(super) struct LocalHeap {
    /// The prefix's address.
    pub(super) address: u64,
    /// The data's address.
    data_address: u64,
    /// The data, its length the data block's size.
    pub(super) data: Vec<u8>,
    /// Whether the data follows the prefix, written with it as one block.
    joined: bool,
    /// The free blocks, `(offset, size)`, in the heap's free list order.
    free: Vec<(u64, u64)>,
}

impl LocalHeap {
    /// A new local heap with `hint` bytes of data, its space taken from `space`
    /// (`H5HL_create`).
    fn new(space: &mut Space, hint: u64) -> Self {
        let hint = align8(hint.max(FREE_RECORD));
        let address = space.allocate(Use::Metadata, HEAP_PREFIX + hint);
        Self {
            address,
            data_address: address + HEAP_PREFIX,
            data: vec![0; to_usize(hint)],
            joined: true,
            free: vec![(0, hint)],
        }
    }

    /// Adds `bytes` and returns their offset (`H5HL_insert`).
    fn insert(&mut self, space: &mut Space, bytes: &[u8]) -> u64 {
        let need = align8(bytes.len() as u64);
        let size = self.data.len() as u64;
        let mut offset = None;
        let mut last: Option<usize> = None;
        for index in 0..self.free.len() {
            let (at, length) = self.free[index];
            if length > need && length - need >= FREE_RECORD {
                offset = Some(at);
                self.free[index] = (at + need, length - need);
                break;
            } else if length == need {
                offset = Some(at);
                self.free.remove(index);
                break;
            } else if last.is_none_or(|last| self.free[last].0 < at) {
                last = Some(index);
            }
        }
        let offset = offset.unwrap_or_else(|| {
            let ends_heap = last.is_some_and(|last| self.free[last].0 + self.free[last].1 == size);
            let mut more = need.max(size);
            if !ends_heap && more < need + FREE_RECORD {
                more = need;
            }
            let new_size = size + more;
            if space.try_extend(Use::Metadata, self.data_address, size, more) {
                // Grown in place.
            } else {
                // The data moves: its old space is freed first, then new space is taken.
                space.free(Use::Metadata, self.data_address, size);
                let moved = space.allocate(Use::Metadata, new_size);
                if moved != self.data_address {
                    self.joined = false;
                }
                self.data_address = moved;
            }
            self.data.resize(to_usize(new_size), 0);
            match last {
                Some(last) if ends_heap => {
                    let (at, length) = self.free[last];
                    let remaining = length + more - need;
                    if remaining < FREE_RECORD {
                        self.free.remove(last);
                    } else {
                        self.free[last] = (at + need, remaining);
                    }
                    at
                }
                _ => {
                    if more - need >= FREE_RECORD {
                        self.free.insert(0, (size + need, more - need));
                    }
                    size
                }
            }
        });
        let at = to_usize(offset);
        self.data[at..at + bytes.len()].copy_from_slice(bytes);
        offset
    }

    /// The name at `offset`.
    pub(super) fn name(&self, offset: u64) -> &[u8] {
        let rest = &self.data[to_usize(offset)..];
        let end = rest
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(rest.len());
        &rest[..end]
    }

    /// The heap's bytes: the prefix and the data, joined or apart.
    fn encode(&self) -> Vec<(u64, Vec<u8>)> {
        let mut data = self.data.clone();
        for (index, &(at, length)) in self.free.iter().enumerate() {
            let next = self.free.get(index + 1).map_or(1, |next| next.0);
            let at = to_usize(at);
            data[at..at + 8].copy_from_slice(&next.to_le_bytes());
            data[at + 8..at + 16].copy_from_slice(&length.to_le_bytes());
        }
        let first_free = self.free.first().map_or(1, |first| first.0);
        let mut prefix = b"HEAP".to_vec();
        prefix.extend_from_slice(&[0; 4]);
        prefix.extend_from_slice(&(data.len() as u64).to_le_bytes());
        prefix.extend_from_slice(&first_free.to_le_bytes());
        prefix.extend_from_slice(&self.data_address.to_le_bytes());
        if self.joined {
            prefix.extend_from_slice(&data);
            vec![(self.address, prefix)]
        } else {
            vec![(self.address, prefix), (self.data_address, data)]
        }
    }
}

/// One member in a symbol table node: its name, the name's offset in the heap, and the
/// member's index among the file's objects.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    /// The name.
    pub(super) name: Vec<u8>,
    /// The name's offset in the local heap.
    pub(super) offset: u64,
    /// The member's index among the file's objects.
    pub(super) object: usize,
}

/// A group's symbol table: its B-tree, local heap and symbol table nodes.
#[derive(Clone, Debug)]
pub(super) struct SymbolTable {
    /// The B-tree, keyed by name offsets.
    pub(super) btree: Btree<u64>,
    /// The local heap of names.
    pub(super) heap: LocalHeap,
    /// The symbol table nodes by address, each its entries in name order.
    pub(super) leaves: BTreeMap<u64, Vec<Entry>>,
}

/// The group B-tree's class: names in a local heap and symbol table nodes
/// (`H5G_BT_SNODE`).
struct Symbols<'a> {
    /// The heap of names.
    heap: &'a mut LocalHeap,
    /// The symbol table nodes.
    leaves: &'a mut BTreeMap<u64, Vec<Entry>>,
}

impl Symbols<'_> {
    /// A new, empty symbol table node.
    fn new_leaf(&mut self, space: &mut Space) -> u64 {
        let address = space.allocate(Use::Metadata, LEAF_SIZE);
        self.leaves.insert(address, Vec::new());
        address
    }
}

impl Class for Symbols<'_> {
    type Key = u64;
    type Value = (Vec<u8>, usize);
    fn follow_edges(&self) -> bool {
        true
    }

    fn compare(&self, left: &u64, value: &Self::Value, right: &u64) -> i32 {
        if value.0.as_slice() <= self.heap.name(*left) {
            -1
        } else {
            i32::from(value.0.as_slice() > self.heap.name(*right))
        }
    }

    fn new_child(
        &mut self,
        space: &mut Space,
        insert: Insert,
        left: &mut u64,
        _: &Self::Value,
        right: &mut u64,
    ) -> u64 {
        if insert == Insert::First {
            *left = 0;
            *right = 0;
        }
        self.new_leaf(space)
    }

    fn insert(
        &mut self,
        space: &mut Space,
        address: u64,
        _: &mut u64,
        changed: &mut Changed,
        middle: &mut u64,
        (name, object): &Self::Value,
        right: &mut u64,
    ) -> (Insert, u64) {
        let entries = &self.leaves[&address];
        let index = entries.partition_point(|entry| entry.name.as_slice() < name.as_slice());
        let mut name_bytes = name.clone();
        name_bytes.push(0);
        let offset = self.heap.insert(space, &name_bytes);
        let entry = Entry {
            name: name.clone(),
            offset,
            object: *object,
        };
        if self.leaves[&address].len() < 2 * LEAF_K {
            let entries = self.leaves.entry(address).or_default();
            if index == entries.len() {
                *right = offset;
                changed.right = true;
            }
            entries.insert(index, entry);
            return (Insert::Nothing, 0);
        }
        // The node is full: its upper half moves to a new node.
        let split = self.new_leaf(space);
        let entries = self.leaves.entry(address).or_default();
        let upper = entries.split_off(LEAF_K);
        *middle = entries[LEAF_K - 1].offset;
        if index <= LEAF_K {
            if index == LEAF_K {
                *middle = offset;
            }
            entries.insert(index, entry);
            self.leaves.insert(split, upper);
        } else {
            let index = index - LEAF_K;
            if index == LEAF_K {
                *right = offset;
                changed.right = true;
            }
            let mut upper = upper;
            upper.insert(index, entry);
            self.leaves.insert(split, upper);
        }
        (Insert::Right, split)
    }
}

impl SymbolTable {
    /// A new, empty symbol table: its B-tree, then its heap holding the empty name
    /// (`H5G__stab_create`).
    pub(super) fn new(space: &mut Space) -> Self {
        let btree = Btree::new(space, NODE_SIZE, 2 * GROUP_K, 0);
        let mut heap = LocalHeap::new(space, HEAP_HINT);
        heap.insert(space, b"\0");
        Self {
            btree,
            heap,
            leaves: BTreeMap::new(),
        }
    }

    /// Adds the member `name`, the object at `object` (`H5G__stab_insert`).
    pub(super) fn insert(&mut self, space: &mut Space, name: &[u8], object: usize) {
        let mut class = Symbols {
            heap: &mut self.heap,
            leaves: &mut self.leaves,
        };
        self.btree
            .insert(&mut class, space, &(name.to_vec(), object));
    }

    /// The bytes of the B-tree, the heap and the symbol table nodes; `entry` encodes one
    /// member's 40-byte entry.
    pub(super) fn encode(&self, entry: impl Fn(&Entry) -> Vec<u8>) -> Vec<(u64, Vec<u8>)> {
        let mut blocks = self.heap.encode();
        blocks.extend(self.btree.encode(0, |key| key.to_le_bytes().to_vec()));
        for (&address, entries) in &self.leaves {
            let mut bytes = b"SNOD\x01\x00".to_vec();
            let count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
            bytes.extend_from_slice(&count.to_le_bytes());
            for member in entries {
                bytes.extend_from_slice(&entry(member));
            }
            bytes.resize(to_usize(LEAF_SIZE), 0);
            blocks.push((address, bytes));
        }
        blocks
    }
}
