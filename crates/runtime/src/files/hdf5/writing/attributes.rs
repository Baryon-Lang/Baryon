//! Attributes as h5py writes them, and the global heap that holds their text.
//!
//! h5py's `attrs[name] = value` deletes an attribute of that name if there is one, then
//! creates the attribute, an attribute message added to the object's header, and writes its
//! value (`AttributeManager.create` in h5py 3.16). A number or an array of numbers is stored
//! in the message. A `str` is variable-length UTF-8 text: the message holds its length and
//! where it is in the *global heap*, a list of collections of at least 4096 bytes, each
//! holding objects one after another (`H5HG` in the HDF5 library 2.0). Deleting an attribute
//! leaves its text in the global heap, as the library does.

use super::encode::{Element, dataspace, datatype, pad8};
use super::header::{Header, align8, to_usize};
use super::space::{Space, Use};

/// The smallest a global heap collection is.
const COLLECTION_MINIMUM: u64 = 4096;
/// The size of a collection's header: `GCOL`, version, 3 reserved bytes and its size.
const COLLECTION_HEADER: u64 = 16;
/// The size of an object's header in a collection: index, reference count, 4 reserved
/// bytes and its size.
const OBJECT_HEADER: u64 = 16;
/// The largest a collection grows to in place (`H5HG_MAXSIZE`).
const COLLECTION_MAXIMUM: u64 = 65536;
/// The most collections the library remembers as having free space (`H5F_NCWFS`).
const REMEMBERED: usize = 16;
/// The type number of an attribute message.
const ATTRIBUTE: u16 = 0x0C;
/// The datatype h5py gives a `str`: variable-length text, null-terminated, UTF-8, whose base
/// type is an unsigned byte.
const VARIABLE_TEXT: [u8; 20] = [
    0x19, 0x01, 0x01, 0x00, 16, 0, 0, 0, 0x10, 0, 0, 0, 1, 0, 0, 0, 0, 0, 8, 0,
];

/// One global heap collection.
#[derive(Clone, Debug)]
struct Collection {
    /// Its address.
    address: u64,
    /// Its size in bytes.
    size: u64,
    /// Its objects after the free-space object: index, offset in the collection, bytes.
    objects: Vec<(u16, u64, Vec<u8>)>,
    /// The next index to hand out.
    next: u16,
    /// Where its free space starts, `None` once used up.
    free_at: Option<u64>,
    /// The size of its free space.
    free: u64,
}

impl Collection {
    /// Adds `bytes` as the next object and returns its index (`H5HG__alloc`).
    fn add(&mut self, bytes: &[u8]) -> u16 {
        let index = self.next;
        self.next += 1;
        let need = OBJECT_HEADER + align8(bytes.len() as u64);
        let at = self.free_at.unwrap_or(COLLECTION_HEADER);
        self.objects.push((index, at, bytes.to_vec()));
        if need == self.free {
            self.free = 0;
            self.free_at = None;
        } else {
            self.free -= need;
            self.free_at = Some(at + need);
        }
        index
    }

    /// The collection's bytes.
    fn encode(&self) -> (u64, Vec<u8>) {
        let mut bytes = vec![0; to_usize(self.size)];
        bytes[..8].copy_from_slice(b"GCOL\x01\x00\x00\x00");
        bytes[8..16].copy_from_slice(&self.size.to_le_bytes());
        let mut object = |index: u16, at: u64, size: u64| {
            let at = to_usize(at);
            bytes[at..at + 2].copy_from_slice(&index.to_le_bytes());
            bytes[at + 8..at + 16].copy_from_slice(&size.to_le_bytes());
            at + 16
        };
        let mut placed = Vec::new();
        for (index, at, data) in &self.objects {
            placed.push((object(*index, *at, data.len() as u64), data));
        }
        if let Some(at) = self.free_at
            && self.free >= OBJECT_HEADER
        {
            object(0, at, self.free);
        }
        for (at, data) in placed {
            bytes[at..at + data.len()].copy_from_slice(data);
        }
        (self.address, bytes)
    }
}

/// The file's global heap.
#[derive(Clone, Debug, Default)]
pub(super) struct GlobalHeap {
    /// Every collection, in the order they were made.
    collections: Vec<Collection>,
    /// The collections remembered as having free space, most recently useful first, as
    /// indexes into `collections` (the file's `cwfs` list).
    remembered: Vec<usize>,
}

impl GlobalHeap {
    /// Adds `bytes` and returns the address of its collection and its index there
    /// (`H5HG_insert`).
    pub(super) fn insert(&mut self, space: &mut Space, bytes: &[u8]) -> (u64, u16) {
        let need = OBJECT_HEADER + align8(bytes.len() as u64);
        let mut found = self
            .remembered
            .iter()
            .position(|&index| self.collections[index].free >= need);
        if found.is_none() {
            // A collection that can grow in place to fit (`H5F_cwfs_find_free_heap`).
            for (place, &index) in self.remembered.iter().enumerate() {
                let collection = &mut self.collections[index];
                let more = collection.size.max(need - collection.free);
                if collection.size + more <= COLLECTION_MAXIMUM
                    && space.try_extend(Use::Raw, collection.address, collection.size, more)
                {
                    let old = collection.size;
                    collection.size += more;
                    collection.free += more;
                    collection.free_at.get_or_insert(old);
                    found = Some(place);
                    break;
                }
            }
        }
        let index = if let Some(place) = found {
            if place > 0 {
                self.remembered.swap(place - 1, place);
            }
            self.remembered[place.saturating_sub(1)]
        } else {
            self.create(space, need + COLLECTION_HEADER)
        };
        let collection = &mut self.collections[index];
        (collection.address, collection.add(bytes))
    }

    /// Makes a collection of at least `size` bytes and returns its index (`H5HG__create`).
    fn create(&mut self, space: &mut Space, size: u64) -> usize {
        let size = align8(size.max(COLLECTION_MINIMUM));
        let address = space.allocate(Use::Raw, size);
        let index = self.collections.len();
        self.collections.push(Collection {
            address,
            size,
            objects: Vec::new(),
            next: 1,
            free_at: Some(COLLECTION_HEADER),
            free: size - COLLECTION_HEADER,
        });
        if self.remembered.len() < REMEMBERED {
            self.remembered.insert(0, index);
        } else if let Some(place) = self
            .remembered
            .iter()
            .rposition(|&other| self.collections[other].free < size - COLLECTION_HEADER)
        {
            // The last one with less free space is forgotten and the new one goes first
            // (`H5F_cwfs_add`).
            self.remembered.remove(place);
            self.remembered.insert(0, index);
        }
        index
    }

    /// The bytes of every collection.
    pub(super) fn encode(&self) -> Vec<(u64, Vec<u8>)> {
        self.collections.iter().map(Collection::encode).collect()
    }
}

/// An attribute's value.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    /// Text, stored as variable-length UTF-8.
    Text(String),
    /// Numbers of one element type in a shape, `[]` for a single number: their bytes,
    /// little-endian, in C order.
    Numbers(Element, Vec<u64>, Vec<u8>),
}

/// Sets the attribute `name` of the object whose header is `header` to `value`: deletes an
/// attribute already named so, whose message is `old`, then adds the new message and writes
/// the value. Returns the new message's identity.
pub(super) fn set(
    space: &mut Space,
    global: &mut GlobalHeap,
    header: &mut Header,
    old: Option<usize>,
    (name, value): (&str, &Value),
) -> usize {
    if let Some(old) = old {
        header.remove(space, old);
    }
    let (kind, shape, size) = match value {
        Value::Text(_) => (VARIABLE_TEXT.to_vec(), Vec::new(), 16),
        Value::Numbers(element, shape, bytes) => (datatype(*element), shape.clone(), bytes.len()),
    };
    let shape = dataspace(&shape);
    let id = header.add(
        space,
        ATTRIBUTE,
        message(name, &kind, &shape, &vec![0; size]),
        0,
    );
    let data = match value {
        Value::Text(text) => {
            let (address, index) = global.insert(space, text.as_bytes());
            let mut data = u32::try_from(text.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes()
                .to_vec();
            data.extend_from_slice(&address.to_le_bytes());
            data.extend_from_slice(&u32::from(index).to_le_bytes());
            data
        }
        Value::Numbers(_, _, bytes) => bytes.clone(),
    };
    header.set_body(id, message(name, &kind, &shape, &data));
    id
}

/// A version 1 attribute message: version, a reserved byte, the sizes of the name (with its
/// terminating zero), datatype and dataspace, then each padded to 8 bytes, then the data.
fn message(name: &str, kind: &[u8], shape: &[u8], data: &[u8]) -> Vec<u8> {
    let mut name = name.as_bytes().to_vec();
    name.push(0);
    let length = |bytes: &[u8]| u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes();
    let mut body = vec![1, 0];
    body.extend_from_slice(&length(&name));
    body.extend_from_slice(&length(kind));
    body.extend_from_slice(&length(shape));
    body.extend_from_slice(&pad8(&name));
    body.extend_from_slice(&pad8(kind));
    body.extend_from_slice(&pad8(shape));
    body.extend_from_slice(data);
    body
}
