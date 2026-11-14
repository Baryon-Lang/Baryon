//! A file being written as its objects: groups and datasets, each with an object header and
//! attributes, built in the order h5py's calls build them.

use super::attributes::{GlobalHeap, Value, set};
use super::encode::{Element, dataspace, datatype, pad8};
use super::groups::{Entry, SymbolTable};
use super::header::Header;
use super::space::{Space, UNDEFINED, Use};

/// The type number of a dataspace message.
const DATASPACE: u16 = 0x01;
/// The type number of a datatype message.
const DATATYPE: u16 = 0x03;
/// The type number of a fill value message.
const FILL: u16 = 0x05;
/// The type number of a layout message.
const LAYOUT: u16 = 0x08;
/// The type number of a symbol table message.
const SYMBOL_TABLE: u16 = 0x11;
/// The header room a group starts with: its symbol table message (`H5G_CRT_OBJ_HEADER`'s hint).
const GROUP_HINT: u64 = 4 + 2 * 8;
/// The header room a dataset starts with (`H5D_MINHDR_SIZE`).
const DATASET_HINT: u64 = 256;
/// The size of a version 0 superblock with its root group's entry.
const SUPERBLOCK: u64 = 96;
/// The fill value message h5py's datasets get: version 2, space allocated late, fill values
/// written if set, the default fill value defined.
const FILL_LATE: [u8; 8] = [2, 2, 2, 1, 0, 0, 0, 0];

/// What an object is.
#[derive(Clone, Debug)]
pub(super) enum Kind {
    /// A group: its symbol table, and its members' names with their indexes among the file's
    /// objects.
    Group(SymbolTable, Vec<(Vec<u8>, usize)>),
    /// A dataset whose values are one block: its address and size.
    Contiguous(u64, u64),
}

/// A group or a dataset.
#[derive(Clone, Debug)]
pub(super) struct Object {
    /// Its object header.
    pub(super) header: Header,
    /// Its attributes' names and their messages' identities.
    attributes: Vec<(String, usize)>,
    /// What it is.
    pub(super) kind: Kind,
}

/// A file being written: its space, objects and global heap. Object 0 is the root group.
#[derive(Debug)]
pub(super) struct Writer {
    /// The file's space.
    pub(super) space: Space,
    /// Every object, in the order made.
    pub(super) objects: Vec<Object>,
    /// The global heap.
    global: GlobalHeap,
}

impl Writer {
    /// A new file: the superblock, then the root group (`H5Fcreate`).
    pub(super) fn new() -> Self {
        let mut space = Space::new();
        space.allocate(Use::Metadata, SUPERBLOCK);
        let mut writer = Self {
            space,
            objects: Vec::new(),
            global: GlobalHeap::default(),
        };
        let root = writer.new_group();
        writer.objects.push(root);
        writer
    }

    /// A new group's object: its header, then its symbol table (`H5G__create`).
    fn new_group(&mut self) -> Object {
        let mut header = Header::new(&mut self.space, GROUP_HINT);
        let table = SymbolTable::new(&mut self.space);
        let mut body = table.btree.root.to_le_bytes().to_vec();
        body.extend_from_slice(&table.heap.address.to_le_bytes());
        header.add(&mut self.space, SYMBOL_TABLE, body, 0);
        Object {
            header,
            attributes: Vec::new(),
            kind: Kind::Group(table, Vec::new()),
        }
    }

    /// The index of the object at `path`, names separated by `/`.
    pub(super) fn find(&self, path: &str) -> Result<usize, String> {
        let mut object = 0;
        for name in path.split('/').filter(|name| !name.is_empty()) {
            let Kind::Group(_, members) = &self.objects[object].kind else {
                return Err(format!("'{path}' is not in a group"));
            };
            object = members
                .iter()
                .find(|(member, _)| member == name.as_bytes())
                .map(|&(_, index)| index)
                .ok_or_else(|| {
                    format!("Unable to synchronously open object (object '{name}' doesn't exist)")
                })?;
        }
        Ok(object)
    }

    /// The group that `path` names a new member of, and the member's name. Groups on the way
    /// that are not there yet are made, as h5py makes them (`H5Pset_create_intermediate_group`).
    fn parent<'a>(&mut self, path: &'a str) -> Result<(usize, &'a str), String> {
        let trimmed = path.trim_end_matches('/');
        let (parents, name) = trimmed.rsplit_once('/').unwrap_or(("", trimmed));
        let mut group = 0;
        for step in parents.split('/').filter(|step| !step.is_empty()) {
            if let Some(member) = self.member(group, step)? {
                group = member;
                continue;
            }
            let made = self.new_group();
            self.objects.push(made);
            let index = self.objects.len() - 1;
            self.link(group, step, index);
            group = index;
        }
        if name.is_empty() || self.member(group, name)?.is_some() {
            return Err("Unable to synchronously create object (name already exists)".to_owned());
        }
        Ok((group, name))
    }

    /// The member `name` of the object at `group`, which must be a group.
    fn member(&self, group: usize, name: &str) -> Result<Option<usize>, String> {
        let Kind::Group(_, members) = &self.objects[group].kind else {
            return Err(format!(
                "Unable to synchronously create object ('{name}' is not in a group)"
            ));
        };
        Ok(members
            .iter()
            .find(|(member, _)| member == name.as_bytes())
            .map(|&(_, index)| index))
    }

    /// Links the object at `object` into the group `group` as `name` (`H5G__stab_insert`).
    fn link(&mut self, group: usize, name: &str, object: usize) {
        let Kind::Group(table, members) = &mut self.objects[group].kind else {
            return;
        };
        table.insert(&mut self.space, name.as_bytes(), object);
        members.push((name.as_bytes().to_vec(), object));
    }

    /// `create_group(path)`.
    pub(super) fn create_group(&mut self, path: &str) -> Result<(), String> {
        let (parent, name) = self.parent(path)?;
        let group = self.new_group();
        self.objects.push(group);
        self.link(parent, name, self.objects.len() - 1);
        Ok(())
    }

    /// `create_dataset(path, data=values)` for values of `element` in `shape`, stored in one
    /// block. Returns the block's address and size, where the values go.
    pub(super) fn create_dataset(
        &mut self,
        path: &str,
        element: Element,
        shape: &[u64],
    ) -> Result<(u64, u64), String> {
        let (parent, name) = self.parent(path)?;
        let size = shape
            .iter()
            .try_fold(u64::from(element.size()), |size, &length| {
                size.checked_mul(length)
            })
            .ok_or("the dataset is too large")?;
        let space = &mut self.space;
        let mut header = Header::new(space, DATASET_HINT);
        header.add(space, DATASPACE, dataspace(shape), 0);
        header.add(space, DATATYPE, pad8(&datatype(element)), 1);
        header.add(space, FILL, FILL_LATE.to_vec(), 1);
        let layout = header.add(space, LAYOUT, contiguous(UNDEFINED, size), 0);
        self.objects.push(Object {
            header,
            attributes: Vec::new(),
            kind: Kind::Contiguous(UNDEFINED, size),
        });
        let index = self.objects.len() - 1;
        self.link(parent, name, index);
        // The values' space is taken when they are written, after the dataset is linked.
        let address = if size > 0 {
            self.space.allocate(Use::Raw, size)
        } else {
            UNDEFINED
        };
        let object = &mut self.objects[index];
        object.header.set_body(layout, contiguous(address, size));
        object.kind = Kind::Contiguous(address, size);
        Ok((address, size))
    }

    /// `attrs[name] = value` on the object at `path`.
    pub(super) fn set_attribute(
        &mut self,
        path: &str,
        name: &str,
        value: &Value,
    ) -> Result<(), String> {
        let object = self.find(path)?;
        let object = &mut self.objects[object];
        let old = object
            .attributes
            .iter()
            .position(|(other, _)| other == name)
            .map(|index| object.attributes.remove(index).1);
        let id = set(
            &mut self.space,
            &mut self.global,
            &mut object.header,
            old,
            (name, value),
        );
        object.attributes.push((name.to_owned(), id));
        Ok(())
    }

    /// Every structure's bytes, by address, and the end of the file, after closing its space.
    pub(super) fn encode(mut self) -> (u64, Vec<(u64, Vec<u8>)>) {
        self.space.close();
        let mut blocks = vec![(0, self.superblock())];
        for object in &self.objects {
            blocks.extend(object.header.encode());
            if let Kind::Group(table, _) = &object.kind {
                blocks.extend(table.encode(|entry| self.entry(entry)));
            }
        }
        blocks.extend(self.global.encode());
        (self.space.end, blocks)
    }

    /// The version 0 superblock, with the root group's entry.
    fn superblock(&self) -> Vec<u8> {
        let mut bytes = b"\x89HDF\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 8, 8, 0]);
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        for address in [0, UNDEFINED, self.space.end, UNDEFINED] {
            bytes.extend_from_slice(&address.to_le_bytes());
        }
        bytes.extend_from_slice(&self.entry(&Entry {
            name: Vec::new(),
            offset: 0,
            object: 0,
        }));
        bytes
    }

    /// The 40-byte symbol table entry of `entry`: a group's caches its B-tree and heap.
    fn entry(&self, entry: &Entry) -> Vec<u8> {
        let object = &self.objects[entry.object];
        let mut bytes = entry.offset.to_le_bytes().to_vec();
        bytes.extend_from_slice(&object.header.address.to_le_bytes());
        if let Kind::Group(table, _) = &object.kind {
            bytes.extend_from_slice(&1_u32.to_le_bytes());
            bytes.extend_from_slice(&[0; 4]);
            bytes.extend_from_slice(&table.btree.root.to_le_bytes());
            bytes.extend_from_slice(&table.heap.address.to_le_bytes());
        } else {
            bytes.resize(40, 0);
        }
        bytes
    }
}

/// A version 3 contiguous layout message: the values' address and size.
fn contiguous(address: u64, size: u64) -> Vec<u8> {
    let mut body = vec![3, 1];
    body.extend_from_slice(&address.to_le_bytes());
    body.extend_from_slice(&size.to_le_bytes());
    pad8(&body)
}
