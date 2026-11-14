//! A chunked dataset as h5py writes one by default: its values cut into chunks of one shape,
//! each chunk filtered and stored where the library's allocator puts it, and a version 1
//! B-tree of chunks that finds them (`H5Dchunk.c`, `H5Dbtree.c`).

use super::btree::Btree;
use super::chunks::{CHUNK_K, Chunks, Key, Stored, order};
use super::encode::pad8;
use super::filters::Filters;
use super::header::to_usize;
use super::space::{Space, Use};

/// A chunked dataset's storage.
#[derive(Clone, Debug)]
pub(super) struct Chunked {
    /// The dataset's shape.
    pub(super) shape: Vec<u64>,
    /// The chunks' shape.
    pub(super) chunk: Vec<u64>,
    /// The size of an element in bytes.
    pub(super) element: u8,
    /// The filters each chunk passes through.
    pub(super) filters: Filters,
    /// For a deflated dataset, the size of each chunk the caller deflated, in C order of
    /// position. The library stores a deflated chunk even when it is larger than the chunk.
    pub(super) deflated: Vec<u64>,
    /// The B-tree of chunks, once values are written.
    pub(super) btree: Option<Btree<Key>>,
    /// The chunks stored: position in chunks, address and stored size, in the order stored.
    pub(super) stored: Vec<(Vec<u64>, u64, u64)>,
}

impl Chunked {
    /// Storage for a dataset of `shape` in chunks of `chunk`, of elements of `element` bytes,
    /// with nothing stored yet.
    pub(super) fn new(shape: &[u64], chunk: &[u64], element: u8, filters: Filters) -> Self {
        Self {
            shape: shape.to_vec(),
            chunk: chunk.to_vec(),
            element,
            filters,
            deflated: Vec::new(),
            btree: None,
            stored: Vec::new(),
        }
    }

    /// The bytes of one chunk before any filter.
    fn chunk_bytes(&self) -> u64 {
        self.chunk.iter().product::<u64>() * u64::from(self.element)
    }

    /// The number of chunks along each axis.
    fn counts(&self) -> Vec<u64> {
        self.shape
            .iter()
            .zip(&self.chunk)
            .map(|(length, chunk)| length.div_ceil(*chunk))
            .collect()
    }

    /// Sets the sizes of the deflated chunks, `deflated`, as [`Chunked::deflated`] holds them.
    ///
    /// # Errors
    ///
    /// Fails unless there is one size per chunk.
    pub(super) fn set_deflated(&mut self, deflated: Vec<u64>) -> Result<(), String> {
        let count: u64 = self.counts().iter().product();
        if deflated.len() as u64 != count {
            return Err(format!(
                "the dataset has {count} chunks, not {}",
                deflated.len()
            ));
        }
        self.deflated = deflated;
        Ok(())
    }

    /// The bytes of values the caller hands over: the dataset's values, or for a deflated
    /// dataset its deflated chunks one after another.
    pub(super) fn values_size(&self) -> u64 {
        if self.filters.deflate.is_some() {
            self.deflated.iter().sum()
        } else {
            self.shape.iter().product::<u64>() * u64::from(self.element)
        }
    }

    /// The index in C order of the chunk at `position`.
    fn linear(&self, position: &[u64]) -> usize {
        let counts = self.counts();
        to_usize(
            position
                .iter()
                .zip(&counts)
                .fold(0, |linear, (at, count)| linear * count + at),
        )
    }

    /// The size of a chunk's key in the B-tree: its size and filter mask, then an offset per
    /// axis and one for the element.
    fn key_size(&self) -> u64 {
        8 + 8 * (self.chunk.len() as u64 + 1)
    }

    /// Writes the values: makes the B-tree, then stores every chunk in the order the chunk
    /// cache lets them go, each one's space taken and its key added as it goes
    /// (`H5D__chunk_write` and the close that follows).
    pub(super) fn store(&mut self, space: &mut Space) {
        if self.shape.contains(&0) {
            return;
        }
        let two_k = 2 * CHUNK_K;
        let node_size = 24 + two_k as u64 * 8 + (two_k as u64 + 1) * self.key_size();
        let empty: Key = (0, 0, vec![0; self.chunk.len() + 1]);
        let mut btree = Btree::new(space, node_size, two_k, empty);
        let bytes = self.chunk_bytes();
        let checksum = if self.filters.fletcher32 { 4 } else { 0 };
        for position in order(&self.shape, &self.chunk, u64::from(self.element)) {
            let inner = if self.filters.deflate.is_some() {
                self.deflated[self.linear(&position)]
            } else {
                bytes
            };
            let size = inner + checksum;
            let address = space.allocate(Use::Raw, size);
            let mut key_position = position.clone();
            key_position.push(0);
            let value = Stored {
                position: key_position,
                size: u32::try_from(size).unwrap_or(u32::MAX),
                address,
            };
            btree.insert(&mut Chunks, space, &value);
            self.stored.push((position, address, size));
        }
        self.btree = Some(btree);
    }

    /// The body of the version 3 chunked layout message: the B-tree's address, then the
    /// chunk's lengths and the element's size as 32-bit numbers.
    pub(super) fn layout(&self) -> Vec<u8> {
        let rank = u8::try_from(self.chunk.len() + 1).unwrap_or(u8::MAX);
        let mut body = vec![3, 2, rank];
        let address = self
            .btree
            .as_ref()
            .map_or(super::space::UNDEFINED, |tree| tree.root);
        body.extend_from_slice(&address.to_le_bytes());
        for length in self.chunk.iter().chain([&u64::from(self.element)]) {
            body.extend_from_slice(&u32::try_from(*length).unwrap_or(u32::MAX).to_le_bytes());
        }
        pad8(&body)
    }

    /// The B-tree's nodes as `(address, bytes)`. A key's offsets are in elements along each
    /// axis, then in bytes along the element, which is 0 except in the key past the last chunk.
    pub(super) fn encode(&self) -> Vec<(u64, Vec<u8>)> {
        let Some(btree) = &self.btree else {
            return Vec::new();
        };
        let dims: Vec<u64> = self
            .chunk
            .iter()
            .copied()
            .chain([u64::from(self.element)])
            .collect();
        btree.encode(1, |(size, mask, position)| {
            let mut bytes = size.to_le_bytes().to_vec();
            bytes.extend_from_slice(&mask.to_le_bytes());
            for (at, length) in position.iter().zip(&dims) {
                bytes.extend_from_slice(&(at * length).to_le_bytes());
            }
            bytes
        })
    }

    /// Each stored chunk's address and bytes, made from `values`: the dataset's values in C
    /// order, cut into chunks padded with zeros past the dataset's edge and filtered, or for
    /// a deflated dataset the deflated chunks in C order of position, which only take their
    /// checksum.
    pub(super) fn chunks(&self, values: &[u8]) -> Vec<(u64, Vec<u8>)> {
        let element = usize::from(self.element);
        let mut starts = Vec::with_capacity(self.deflated.len());
        let mut start = 0;
        for &size in &self.deflated {
            starts.push(start);
            start += to_usize(size);
        }
        self.stored
            .iter()
            .map(|(position, address, _)| {
                let chunk = if self.filters.deflate.is_some() {
                    let index = self.linear(position);
                    let start = starts[index];
                    values[start..start + to_usize(self.deflated[index])].to_vec()
                } else {
                    let chunk = cut(values, &self.shape, &self.chunk, position, element);
                    self.filters.shuffled(chunk, element)
                };
                (*address, self.filters.finish(chunk))
            })
            .collect()
    }
}

/// The chunk at `position` of `values`, a C-order array of `shape` with elements of
/// `element` bytes, cut in chunks of `chunk`; parts past the array's edge are zeros.
fn cut(values: &[u8], shape: &[u64], chunk: &[u64], position: &[u64], element: usize) -> Vec<u8> {
    let rank = shape.len();
    let start: Vec<u64> = position
        .iter()
        .zip(chunk)
        .map(|(at, length)| at * length)
        .collect();
    let inside: Vec<u64> = (0..rank)
        .map(|axis| chunk[axis].min(shape[axis] - start[axis]))
        .collect();
    let mut bytes = vec![0; to_usize(chunk.iter().product::<u64>()) * element];
    let run = to_usize(inside[rank - 1]) * element;
    // Walk the rows along the last axis inside the array, an index per axis before it.
    let mut index = vec![0_u64; rank - 1];
    loop {
        let (mut source, mut target) = (0, 0);
        for axis in 0..rank {
            let at = index.get(axis).copied().unwrap_or(0);
            source = source * shape[axis] + start[axis] + at;
            target = target * chunk[axis] + at;
        }
        let (source, target) = (to_usize(source) * element, to_usize(target) * element);
        bytes[target..target + run].copy_from_slice(&values[source..source + run]);
        let Some(axis) = (0..rank - 1)
            .rev()
            .find(|&axis| index[axis] + 1 < inside[axis])
        else {
            return bytes;
        };
        index[axis] += 1;
        index[axis + 1..].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_pads_the_edge() {
        // A 3 × 3 array of bytes 0 to 8 in 2 × 2 chunks: the chunk at (1, 0) holds row 2's
        // first two values, then zeros.
        let values: Vec<u8> = (0..9).collect();
        assert_eq!(cut(&values, &[3, 3], &[2, 2], &[1, 0], 1), [6, 7, 0, 0]);
        assert_eq!(cut(&values, &[3, 3], &[2, 2], &[0, 1], 1), [2, 0, 5, 0]);
    }
}
