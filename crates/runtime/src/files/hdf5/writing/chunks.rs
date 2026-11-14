//! Chunked datasets as h5py writes them by default: a version 1 B-tree indexes the chunks, and
//! each chunk passes through the dataset's filters, `shuffle` and `fletcher32` here, before it
//! is stored (`H5Dbtree.c`, `H5Dchunk.c` and `H5Z` in the HDF5 library 2.0).
//!
//! Where each chunk goes depends on when the library stores it. A write puts chunks in the
//! dataset's *chunk cache*, in the order of their positions, and a chunk is stored, its space
//! taken and its key added to the B-tree, when it leaves the cache: when a later chunk hashes
//! to its slot, when the cache passes its 8 MiB, or when the dataset closes, which stores what
//! is left in the order it came in. A chunk larger than the cache is stored at once.
//! [`order`] replays that to find the order the chunks are stored in.

use super::btree::{Changed, Class, Insert};
use super::space::Space;

/// The B-tree's `K` for chunks: a node holds up to 64 children.
pub(super) const CHUNK_K: usize = 32;
/// The number of slots in the chunk cache (`H5F_ACS_DATA_CACHE_NUM_SLOTS_DEF` in 2.0).
const SLOTS: u64 = 8191;
/// The chunk cache's size in bytes (`H5F_ACS_DATA_CACHE_BYTE_SIZE_DEF` in 2.0).
const CACHE_BYTES: u64 = 8 * 1024 * 1024;
/// The share of the cache's entries the first way of making room looks at before the second
/// starts (`H5F_ACS_PREEMPT_READ_CHUNKS_DEF`).
const PREEMPT: f64 = 0.75;

/// A chunk's key: its stored size, the mask of filters skipped, and its position counted in
/// chunks along each axis, with a 0 for the element.
pub(super) type Key = (u32, u32, Vec<u64>);

/// A chunk to add to the B-tree: its position, stored size and address.
pub(super) struct Stored {
    /// Its position in chunks, with a 0 for the element.
    pub(super) position: Vec<u64>,
    /// Its stored size.
    pub(super) size: u32,
    /// Its address.
    pub(super) address: u64,
}

/// The chunk B-tree's class (`H5B_BTREE` in `H5Dbtree.c`).
pub(super) struct Chunks;

/// Whether `a` comes before (-1), with (0) or after (1) `b`, axis by axis.
fn order_of(a: &[u64], b: &[u64]) -> i32 {
    a.iter()
        .zip(b)
        .find(|(x, y)| x != y)
        .map_or(0, |(x, y)| if x < y { -1 } else { 1 })
}

impl Class for Chunks {
    type Key = Key;
    type Value = Stored;

    fn follow_edges(&self) -> bool {
        false
    }

    fn compare(&self, left: &Key, value: &Stored, right: &Key) -> i32 {
        let at = &value.position;
        if at.len() == 2 {
            // One axis and the element: the library compares the axis, then the element.
            if at[0] > right.2[0] || (at[0] == right.2[0] && at[1] >= right.2[1]) {
                1
            } else if at[0] < left.2[0] {
                -1
            } else {
                0
            }
        } else if order_of(at, &right.2) >= 0 {
            1
        } else if order_of(at, &left.2) < 0 {
            -1
        } else {
            0
        }
    }

    fn new_child(
        &mut self,
        _: &mut Space,
        insert: Insert,
        left: &mut Key,
        value: &Stored,
        right: &mut Key,
    ) -> u64 {
        *left = (value.size, 0, value.position.clone());
        if insert != Insert::Left {
            *right = (0, 0, value.position.iter().map(|at| at + 1).collect());
        }
        value.address
    }

    fn insert(
        &mut self,
        _: &mut Space,
        _: u64,
        left: &mut Key,
        _: &mut Changed,
        middle: &mut Key,
        value: &Stored,
        _: &mut Key,
    ) -> (Insert, u64) {
        // Every chunk is stored once, so it never matches the chunk on its left: it goes to
        // that chunk's right.
        let _ = left;
        *middle = (value.size, 0, value.position.clone());
        (Insert::Right, value.address)
    }
}

/// The positions of the chunks of a dataset of `shape` in chunks of `chunk` elements of
/// `element` bytes, in the order the library stores them on a write of the whole dataset
/// followed by closing it (`H5D__chunk_write`, `H5D__chunk_lock`, `H5D__chunk_cache_prune` and
/// `H5D__chunk_flush`).
pub(super) fn order(shape: &[u64], chunk: &[u64], element: u64) -> Vec<Vec<u64>> {
    let counts: Vec<u64> = shape
        .iter()
        .zip(chunk)
        .map(|(n, c)| n.div_ceil(*c))
        .collect();
    let bytes = chunk.iter().product::<u64>() * element;
    let total: u64 = counts.iter().product();
    let positions = (0..total).map(|mut linear| {
        let mut position = vec![0; counts.len()];
        for (axis, count) in counts.iter().enumerate().rev() {
            position[axis] = linear % count;
            linear /= count;
        }
        position
    });
    if bytes > CACHE_BYTES {
        return positions.collect();
    }
    let bits: Vec<u32> = counts
        .iter()
        .map(|count| count.max(&1).next_power_of_two().trailing_zeros())
        .collect();
    let mut cache = Cache {
        entries: Vec::new(),
        used: 0,
        bytes,
        stored: Vec::new(),
    };
    for position in positions {
        let whole = position
            .iter()
            .zip(chunk)
            .zip(shape)
            .all(|((at, length), total)| (at + 1) * length <= *total);
        let slot = position
            .iter()
            .zip(&bits)
            .skip(1)
            .fold(position[0], |value, (at, bits)| (value << bits) ^ at)
            % SLOTS;
        cache.add(position, slot, whole);
    }
    cache.close()
}

/// One chunk in the cache.
struct Entry {
    /// Its position.
    position: Vec<u64>,
    /// Its slot.
    slot: u64,
    /// Whether the write filled it: only those go first when room is made.
    whole: bool,
}

/// The chunk cache: its entries, oldest first, with what has left it so far.
struct Cache {
    /// The entries, oldest first.
    entries: Vec<Entry>,
    /// The bytes the entries hold.
    used: u64,
    /// The bytes of one chunk.
    bytes: u64,
    /// The positions of the chunks stored so far, in order.
    stored: Vec<Vec<u64>>,
}

impl Cache {
    /// Adds the chunk at `position` to `slot`, storing the chunk there and making room first.
    fn add(&mut self, position: Vec<u64>, slot: u64, whole: bool) {
        if let Some(index) = self.entries.iter().position(|entry| entry.slot == slot) {
            self.evict(index);
        }
        self.prune();
        self.entries.push(Entry {
            position,
            slot,
            whole,
        });
        self.used += self.bytes;
    }

    /// Stores the entry at `index` and removes it.
    fn evict(&mut self, index: usize) {
        let entry = self.entries.remove(index);
        self.stored.push(entry.position);
        self.used -= self.bytes;
    }

    /// Makes room for one more chunk (`H5D__chunk_cache_prune`): two pointers walk the entries
    /// from the oldest, the first taking only chunks written whole, the second, which starts
    /// once the first has passed three quarters of them, taking any.
    fn prune(&mut self) {
        let bytes = self.bytes;
        if self.used + bytes <= CACHE_BYTES {
            return;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "the library truncates the same way"
        )]
        let mut countdown = (self.entries.len() as f64 * PREEMPT) as i64;
        // Pointers are positions in `entries`; removing an entry shifts the later ones.
        let mut first = Some(0);
        let mut second: Option<usize> = None;
        while (first.is_some() || second.is_some()) && self.used + bytes > CACHE_BYTES {
            if countdown == 0 {
                second = Some(0);
            }
            let mut next = [first.map(|at| at + 1), second.map(|at| at + 1)];
            for way in 0..2 {
                if self.used + bytes <= CACHE_BYTES {
                    break;
                }
                let pointer = if way == 0 { first } else { second };
                let Some(at) = pointer.filter(|&at| at < self.entries.len()) else {
                    continue;
                };
                if way == 0 && !self.entries[at].whole {
                    continue;
                }
                self.evict(at);
                for (index, slot) in [&mut first, &mut second].into_iter().enumerate() {
                    if *slot == Some(at) {
                        *slot = None;
                    }
                    if let Some(after) = next[index]
                        && after > at
                    {
                        next[index] = Some(after - 1);
                    }
                }
            }
            first = next[0].filter(|&at| at < self.entries.len());
            second = next[1].filter(|&at| at < self.entries.len());
            countdown -= 1;
        }
    }

    /// Closes the dataset: stores what is left, oldest first, and returns every chunk in the
    /// order stored.
    fn close(mut self) -> Vec<Vec<u64>> {
        self.stored
            .extend(self.entries.drain(..).map(|entry| entry.position));
        self.stored
    }
}
