//! HDF5 files (decision 0052): the groups, datasets and attributes of a file, and the values
//! of one dataset, read as the HDF Group's *HDF5 File Format Specification* (version 3.0)
//! defines them, in safe code the exported functions in `abi::hdf5` call. What the objects
//! mean to a program is parsed in Baryon, in the standard library, from the description lines
//! [`describe`] writes; this module reads the file's binary structures and converts values.
//!
//! An HDF5 file is a superblock, which points to the root group, and objects, each described
//! by an object header: a list of messages. A group's messages list its links, the names of
//! its members; a dataset's give its shape (dataspace), its element type (datatype), where its
//! values are (layout) and the filters they passed through. Values are stored in one block,
//! inside the header for small datasets, or in chunks found through an index and each filtered
//! on its own. Both the layout h5py writes by default and the one `libver="latest"` writes are
//! read: version 0 to 3 superblocks, version 1 and 2 object headers, symbol table groups and
//! link messages, dense storage in fractal heaps, and every chunk index.
//!
//! Cost: opening reads every object header once, a few hundred bytes each. A read of a
//! contiguous or compact dataset is one pass over its values; a chunked dataset reads each
//! chunk once, passes it back through its filters, and copies its values into place.

#[cfg(test)]
use super::ErrorKind;
use super::OsFailure;
use cursor::{Failure, Source, UNDEFINED};
use header::read_header;
use messages::{DATASPACE, DATATYPE, FILL, FILL_OLD, FILTERS, LAYOUT};
use std::collections::HashSet;

mod attributes;
mod btree2;
mod chunks;
mod cursor;
mod dense;
mod extensible;
mod groups;
mod header;
mod messages;
pub(crate) mod selection;
mod superblock;
mod values;

pub(crate) use values::{Inflate, deflated_chunks, read_dataset};

/// The element type of a dataset or an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Datatype {
    /// An integer of `size` bytes.
    Integer {
        /// Its size in bytes.
        size: u32,
        /// Whether it is signed.
        signed: bool,
        /// Whether it is stored big-endian.
        big: bool,
    },
    /// An IEEE float of 4 or 8 bytes.
    Float {
        /// Its size in bytes.
        size: u32,
        /// Whether it is stored big-endian.
        big: bool,
    },
    /// Text of a fixed `size` bytes.
    Text {
        /// Its size in bytes.
        size: u32,
    },
    /// Text of any length, stored in the global heap: what h5py writes for a `str`.
    VariableText,
    /// A type the reader does not convert: compound, enum, reference, array and the rest.
    Other {
        /// The format's number for the type's class.
        class: u8,
        /// Its size in bytes.
        size: u32,
    },
}

impl Datatype {
    /// The type as `NumPy` writes it in a `dtype`'s string: `<f4`, `>i2`, `|u1`, `|S5`, or `|O`
    /// for variable-length text and for a type not converted.
    pub(crate) fn name(self) -> String {
        let order = |big: bool, size: u32| match (size, big) {
            (1, _) => '|',
            (_, true) => '>',
            (_, false) => '<',
        };
        match self {
            Self::Integer { size, signed, big } => {
                let letter = if signed { 'i' } else { 'u' };
                format!("{}{letter}{size}", order(big, size))
            }
            Self::Float { size, big } => format!("{}f{size}", order(big, size)),
            Self::Text { size } => format!("|S{size}"),
            Self::VariableText | Self::Other { .. } => "|O".to_owned(),
        }
    }

    /// The size of one stored value in bytes; a variable-length value's reference into the
    /// global heap is 16 bytes in a file with 8-byte addresses.
    fn size(self, source: &Source) -> u64 {
        match self {
            Self::Integer { size, .. }
            | Self::Float { size, .. }
            | Self::Text { size }
            | Self::Other { size, .. } => u64::from(size),
            Self::VariableText => 8 + u64::from(source.offsets),
        }
    }
}

/// A filter of a dataset's pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Filter {
    /// Its number: 1 deflate, 2 shuffle, 3 fletcher32, 4 szip, 5 nbit, 6 scale-offset, and
    /// 256 and up for registered plugins such as LZF (32000) and Blosc (32001).
    pub(crate) id: u16,
    /// The name the file gives it, if any.
    pub(crate) name: String,
    /// Whether a chunk may skip it.
    pub(crate) optional: bool,
    /// Its parameters: deflate's level, shuffle's element size and so on.
    pub(crate) parameters: Vec<u32>,
}

/// Where a dataset's values are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// In the object header itself.
    Compact(Vec<u8>),
    /// In one block of the file; the address is undefined until a value is written.
    Contiguous {
        /// The block's address.
        address: u64,
        /// Its size in bytes.
        size: u64,
    },
    /// In chunks of a fixed shape, found through an index.
    Chunked {
        /// The chunk's shape.
        chunk: Vec<u64>,
        /// The index.
        index: Index,
    },
}

/// A chunk index: the structure that finds a chunked dataset's chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Index {
    /// A version 1 B-tree at the address, the index of h5py's default files.
    Tree(u64),
    /// One chunk, the whole dataset, and its size and filter mask when filtered.
    Single {
        /// The chunk's address.
        address: u64,
        /// Its size after filters and its filter mask, for a filtered dataset.
        filtered: Option<(u64, u32)>,
    },
    /// Every chunk stored in order from the address, unfiltered.
    Implicit(u64),
    /// A fixed array whose header is at the address.
    Fixed(u64),
    /// An extensible array whose header is at the address.
    Extensible(u64),
    /// A version 2 B-tree whose header is at the address.
    Tree2(u64),
}

/// One chunk of a dataset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chunk {
    /// The position of its first element in the dataset.
    pub(crate) offset: Vec<u64>,
    /// Its address.
    pub(crate) address: u64,
    /// Its size in bytes as stored, after its filters.
    pub(crate) size: u64,
    /// Bit `i` set when the `i`th filter was skipped for this chunk.
    pub(crate) mask: u32,
}

/// A dataset: what its header says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Dataset {
    /// Its shape, slowest axis first; empty for a scalar.
    pub(crate) shape: Vec<u64>,
    /// The largest shape it may grow to, `u64::MAX` along a dimension without limit.
    pub(crate) most: Vec<u64>,
    /// Its element type.
    pub(crate) datatype: Datatype,
    /// Where its values are.
    pub(crate) layout: Layout,
    /// The filters its chunks passed through, in the order they were applied.
    pub(crate) filters: Vec<Filter>,
    /// The bytes of the value of an element never written; zero when `None`.
    pub(crate) fill: Option<Vec<u8>>,
}

/// An attribute's values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Values {
    /// Text, fixed or variable in length, each value without the zero bytes that may end it.
    Text(Vec<String>),
    /// Integers, signed or not.
    Integers(Vec<i128>),
    /// Floats.
    Floats(Vec<f64>),
    /// Values of a type the reader does not convert.
    Unread,
}

/// An attribute of a group or a dataset.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Attribute {
    /// Its name.
    pub(crate) name: String,
    /// Its element type.
    pub(crate) datatype: Datatype,
    /// Its shape; empty for a scalar.
    pub(crate) shape: Vec<u64>,
    /// Its values.
    pub(crate) values: Values,
}

/// A group or a dataset.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Object {
    /// Its path from the root, such as `/run1/temp`; `/` for the root group.
    pub(crate) path: String,
    /// Its dataset, `None` for a group.
    pub(crate) dataset: Option<Dataset>,
    /// Its attributes.
    pub(crate) attributes: Vec<Attribute>,
}

/// An HDF5 file's objects, the root first and each group's members after it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hdf5 {
    /// The objects.
    pub(crate) objects: Vec<Object>,
    /// The size of an address in bytes.
    offsets: u8,
    /// The size of a length in bytes.
    lengths: u8,
    /// The address every other counts from.
    base: u64,
}

impl Hdf5 {
    /// The object at `path`, with or without its leading `/`.
    pub(crate) fn find(&self, path: &str) -> Option<&Object> {
        let path = format!("/{}", path.trim_start_matches('/'));
        self.objects.iter().find(|object| object.path == path)
    }

    /// The file at `path`, opened with the sizes this one was read with.
    fn source(&self, path: &str) -> Result<Source, OsFailure> {
        let mut source = Source::open(path)?;
        source.offsets = self.offsets;
        source.lengths = self.lengths;
        source.base = self.base;
        Ok(source)
    }
}

/// Reads every group, dataset and attribute of the HDF5 file at `path`. A group's members and
/// an object's attributes are listed in name order, as h5py lists them. A member reached by
/// two hard links is listed once, under the first path found; soft and external links are not
/// followed.
///
/// # Errors
///
/// A file that cannot be opened raises its operating system's error; one that is not HDF5, is
/// damaged, or is stored in a form the reader does not read yet raises `INVALID_DATA`, with a
/// message that names the structure.
pub(crate) fn read_file(path: &str) -> Result<Hdf5, OsFailure> {
    let mut source = Source::open(path)?;
    let root =
        superblock::read_superblock(&mut source).map_err(|failure| source.invalid(failure))?;
    let mut objects = Vec::new();
    let mut seen = HashSet::new();
    walk(&mut source, root, "/", &mut objects, &mut seen)
        .map_err(|failure| source.invalid(failure))?;
    Ok(Hdf5 {
        objects,
        offsets: source.offsets,
        lengths: source.lengths,
        base: source.base,
    })
}

/// Adds the object whose header is at `address`, named `path`, and every object below it to
/// `objects`, skipping headers already in `seen`.
fn walk(
    source: &mut Source,
    address: u64,
    path: &str,
    objects: &mut Vec<Object>,
    seen: &mut HashSet<u64>,
) -> Result<(), Failure> {
    if address == UNDEFINED || !seen.insert(address) {
        return Ok(());
    }
    let within = |failure: Failure| format!("{failure}, in the HDF5 object {path}");
    let messages = read_header(source, address).map_err(within)?;
    let mut attributes = attributes::attributes(source, &messages).map_err(within)?;
    attributes.sort_by(|left, right| left.name.cmp(&right.name));
    let dataset = if messages.iter().any(|message| message.kind == LAYOUT) {
        Some(dataset(source, &messages, path).map_err(within)?)
    } else {
        None
    };
    let group = dataset.is_none();
    objects.push(Object {
        path: path.to_owned(),
        dataset,
        attributes,
    });
    if group {
        let mut links = groups::links(source, &messages).map_err(within)?;
        links.sort_by(|left, right| left.0.cmp(&right.0));
        for (name, target) in links {
            if let Some(target) = target {
                let child = if path == "/" {
                    format!("/{name}")
                } else {
                    format!("{path}/{name}")
                };
                walk(source, target, &child, objects, seen)?;
            }
        }
    }
    Ok(())
}

/// The dataset `path` whose header holds `messages`.
fn dataset(source: &Source, messages: &[header::Message], path: &str) -> Result<Dataset, Failure> {
    let find = |kind: u16| {
        messages
            .iter()
            .find(|message| message.kind == kind)
            .ok_or_else(|| format!("the HDF5 dataset {path} has no message of type {kind}"))
    };
    let shared = |message: &header::Message| {
        if message.flags & 2 == 0 {
            Ok(())
        } else {
            Err(format!(
                "the HDF5 dataset {path} has a shared or committed type, which is not read yet"
            ))
        }
    };
    let space = find(DATASPACE)?;
    let kind = find(DATATYPE)?;
    shared(space)?;
    shared(kind)?;
    let (shape, most) = messages::dataspace(source, &space.data)?.unwrap_or_default();
    let filters = match messages.iter().find(|message| message.kind == FILTERS) {
        Some(message) => messages::filters(&message.data)?,
        None => Vec::new(),
    };
    let fill = match messages
        .iter()
        .find(|message| message.kind == FILL || message.kind == FILL_OLD)
    {
        Some(message) => messages::fill(message.kind, &message.data)?,
        None => None,
    };
    Ok(Dataset {
        shape,
        most,
        datatype: messages::datatype(&kind.data)?,
        layout: messages::layout(source, &find(LAYOUT)?.data)?,
        filters,
        fill,
    })
}

/// The lines that describe `file` to the standard library, one per object and one per
/// attribute, fields separated by tabs:
///
/// - `G`, the group's path;
/// - `D`, the dataset's path, its `NumPy` type string and its shape, lengths separated by commas
///   and empty for a scalar;
/// - `A`, the owner's path, the attribute's kind (`str`, `int`, `float` or `other`), its shape
///   as above, its name, then its values, each in its own field: text as stored, numbers as
///   Python prints them.
pub(crate) fn describe(file: &Hdf5) -> Vec<String> {
    let mut lines = Vec::new();
    for object in &file.objects {
        let line = match &object.dataset {
            Some(dataset) => format!(
                "D\t{}\t{}\t{}",
                object.path,
                dataset.datatype.name(),
                joined(&dataset.shape)
            ),
            None => format!("G\t{}", object.path),
        };
        lines.push(line);
        for attribute in &object.attributes {
            lines.push(attributes::line(&object.path, attribute));
        }
    }
    lines
}

/// `lengths` separated by commas.
fn joined(lengths: &[u64]) -> String {
    lengths
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests;
