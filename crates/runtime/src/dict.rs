//! The hash table behind `dict` (decision 0035): CPython's compact layout, with a fixed hash seed
//! so the same program stores and iterates its keys the same way on every run.
//!
//! A table is two arrays. The **entries** hold the keys and values in insertion order, each
//! entry laid out by the compiler as the record `(u64 hash, K, V)`, so an entry is a whole number
//! of 64-bit words and the `(K, V)` pair starts 8 bytes in. The **index** is an open-addressing
//! hash table of positions into the entries, probed linearly from the key's hash. Iterating is a
//! walk over the dense entries array, and the index, 8 bytes a slot, stays small.
//!
//! Removing a key leaves a hole: its entry's hash becomes [`HOLE`], which iteration skips, and its
//! index slot becomes [`DELETED`], which probing steps over. Holes are reclaimed when the entries
//! array is full and the table is rebuilt, so removing never moves an entry and positions stay
//! valid until the next insertion.
//!
//! The runtime does not know the key type. The compiler describes it with a [`DictLayout`]: the
//! size of an entry and of the key, and the key's [`KeyPart`]s, each an integer of 1, 2, 4 or 8
//! bytes or a `str`. Keys hash and compare part by part, so padding inside a tuple key is never
//! read, and a `str` part compares its characters, not its address. Floats are never keys, so
//! comparing integers bit for bit is exact equality.
//!
//! Moving a key or value is a byte copy, as it is everywhere in compiled code. The runtime never
//! drops a key or value: compiled code drops what an entry owns before it removes the entry or
//! frees the table.

use crate::panic::Failure;

/// The hash word of an entry that was removed. Real hashes have their top bit clear, so no key
/// hashes to it.
pub(crate) const HOLE: u64 = u64::MAX;

/// An index slot that never held a position: probing stops here.
const EMPTY: i64 = -1;

/// An index slot whose entry was removed: probing steps over it, and an insertion may reuse it.
const DELETED: i64 = -2;

/// The smallest index, in slots. A power of two, so a hash picks a slot with a mask.
const MINIMUM_SLOTS: usize = 8;

/// The fixed seed every hash starts from (the fractional part of the golden ratio), so hashes,
/// and therefore probe sequences, are the same on every run.
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// The [`KeyPart::kind`] of a `str` part: 16 bytes, a pointer and a length word whose top bit is
/// the "not owned" flag of decision 0022.
pub const STR_PART: u64 = 0;

/// How the compiler describes one `dict` type to the runtime: a constant it emits once per type
/// and passes to every call.
///
/// # Example
///
/// The layout of a `dict[i64, f64]`: an entry is the hash, the key and the value, 24 bytes, and
/// the key is one 8-byte integer at its start.
///
/// ```
/// use runtime::{DictLayout, KeyPart};
///
/// static PARTS: [KeyPart; 1] = [KeyPart { offset: 0, kind: 8 }];
/// let layout = DictLayout { entry_size: 24, key_size: 8, part_count: 1, parts: PARTS.as_ptr() };
/// assert_eq!(layout.key_size, 8);
/// ```
#[repr(C)]
#[derive(Debug)]
pub struct DictLayout {
    /// The size of an entry in bytes, a multiple of 8: the record `(u64, K, V)`.
    pub entry_size: u64,
    /// The size of the key in bytes. The key starts 8 bytes into an entry.
    pub key_size: u64,
    /// How many parts `parts` points to, at least 1.
    pub part_count: u64,
    /// The key's parts, in order.
    pub parts: *const KeyPart,
}

/// One part of a key that hashes and compares: an integer, `bool` or enum tag, or a `str`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPart {
    /// Where the part starts, in bytes from the start of the key.
    pub offset: u64,
    /// The part's size in bytes for an integer, 1, 2, 4 or 8; [`STR_PART`] for a `str`.
    pub kind: u64,
}

/// A `dict`'s table. Compiled code holds a pointer to one, null while the `dict` is empty, and
/// reads one field itself: `entries`, the first word, to reach an entry by its position. The rest
/// is the runtime's alone.
#[repr(C)]
#[derive(Debug)]
pub struct Table {
    /// The first word of the entries array: `storage`'s buffer, read by compiled code.
    entries: *mut u64,
    /// The number of keys, `len(d)`.
    live: i64,
    /// The entries, `capacity` of them, the first `used` written.
    storage: Vec<u64>,
    /// How many entries have been written since the last rebuild, holes included.
    used: usize,
    /// How many entries `storage` has room for: two thirds of the slots, rounded down.
    capacity: usize,
    /// The index: for each slot, a position into the entries, [`EMPTY`] or [`DELETED`].
    index: Vec<i64>,
}

/// A key the runtime reads: its bytes and the parts that say how to hash and compare them.
#[derive(Clone, Copy)]
pub(crate) struct Key<'a> {
    /// The key's bytes, as compiled code lays the key out.
    bytes: &'a [u8],
    /// Its parts.
    parts: &'a [KeyPart],
}

impl<'a> Key<'a> {
    /// The key in `bytes`, described by `parts`.
    ///
    /// # Safety
    ///
    /// Every [`STR_PART`] of `bytes` holds a pointer to as many readable bytes as its length word
    /// says, with the top bit masked off, that stay readable while the key is used.
    #[allow(unsafe_code, reason = "the caller vouches for the key's `str` parts")]
    pub(crate) unsafe fn new(bytes: &'a [u8], parts: &'a [KeyPart]) -> Self {
        Self { bytes, parts }
    }

    /// The hash of the key: each part mixed into a state that starts at [`SEED`], then
    /// finalized so every bit of the input moves the low bits a slot is picked with. The top bit
    /// is always clear, so the hash is never [`HOLE`].
    fn hash(self) -> u64 {
        let mut state = SEED;
        for part in self.parts {
            if part.kind == STR_PART {
                let text = self.text(*part);
                for chunk in text.chunks(8) {
                    let mut word = [0; 8];
                    word[..chunk.len()].copy_from_slice(chunk);
                    state = mix(state, u64::from_le_bytes(word));
                }
                state = mix(state, text.len() as u64);
            } else {
                state = mix(state, self.integer(*part));
            }
        }
        finish(state) >> 1
    }

    /// Whether this key equals `other`, part by part: integers bit for bit, `str`s by their
    /// characters.
    fn equals(self, other: Key<'_>) -> bool {
        self.parts.iter().all(|&part| {
            if part.kind == STR_PART {
                self.text(part) == other.text(part)
            } else {
                self.integer(part) == other.integer(part)
            }
        })
    }

    /// The integer part `part`, zero-extended to 64 bits. Zero-extending a signed integer keeps
    /// equal values equal, which is all hashing and comparing need.
    fn integer(self, part: KeyPart) -> u64 {
        let start = offset(part.offset);
        let mut word = [0; 8];
        let width = offset(part.kind).min(8);
        if let Some(bytes) = self.bytes.get(start..start + width) {
            word[..width].copy_from_slice(bytes);
        }
        u64::from_ne_bytes(word)
    }

    /// The characters of the `str` part `part`, as UTF-8 bytes.
    fn text(self, part: KeyPart) -> &'a [u8] {
        let start = offset(part.offset);
        let Some(header) = self.bytes.get(start..start + 16) else {
            return &[];
        };
        let mut word = [0; 8];
        word.copy_from_slice(&header[8..]);
        // The top bit flags a literal's bytes as not owned (decision 0022).
        let length = offset(u64::from_ne_bytes(word) & (u64::MAX >> 1));
        if length == 0 {
            return &[];
        }
        // SAFETY: `header` holds 16 readable bytes, and the first 8 are the `str`'s pointer;
        // reading it unaligned keeps its provenance whatever the key's alignment.
        #[allow(unsafe_code, reason = "reads the pointer of a `str` key part")]
        let pointer = unsafe { header.as_ptr().cast::<*const u8>().read_unaligned() };
        // SAFETY: `Key::new`'s caller guarantees a `str` part points to `length` readable bytes
        // for as long as the key is used.
        #[allow(unsafe_code, reason = "reads the characters of a `str` key part")]
        unsafe {
            std::slice::from_raw_parts(pointer, length)
        }
    }
}

impl Table {
    /// A new empty table with room for a few entries of `layout`'s size.
    ///
    /// # Errors
    ///
    /// Fails when the memory cannot be allocated.
    pub(crate) fn new(layout: &DictLayout) -> Result<Self, Failure<'static>> {
        let mut table = Self {
            entries: std::ptr::null_mut(),
            live: 0,
            storage: Vec::new(),
            used: 0,
            capacity: 0,
            index: Vec::new(),
        };
        table.rebuild(layout, MINIMUM_SLOTS)?;
        Ok(table)
    }

    /// The number of keys.
    pub(crate) fn len(&self) -> i64 {
        self.live
    }

    /// The position of the entry whose key equals `key`, or -1 when there is none.
    ///
    /// Cost: one hash of the key, then one comparison per entry in its probe sequence that has
    /// the same hash; constant time on average.
    pub(crate) fn find(&self, layout: &DictLayout, key: Key<'_>) -> i64 {
        let hash = key.hash();
        let mask = self.index.len() - 1;
        let mut slot = slot_of(hash, mask);
        loop {
            match self.index[slot] {
                EMPTY => return -1,
                DELETED => {}
                position => {
                    if self.hash_at(layout, position) == hash
                        && self.key_at(layout, position, key.parts).equals(key)
                    {
                        return position;
                    }
                }
            }
            slot = (slot + 1) & mask;
        }
    }

    /// Adds `key`, which must not be in the table yet, as the last entry, copying its bytes in,
    /// and returns the new entry's position. Its value is left for compiled code to write.
    ///
    /// When the entries array is full the table is rebuilt first, dropping the holes and
    /// doubling when needed, so positions from before the call may change.
    ///
    /// # Errors
    ///
    /// Fails when the table must grow and the memory cannot be allocated.
    pub(crate) fn insert(
        &mut self,
        layout: &DictLayout,
        key: Key<'_>,
    ) -> Result<i64, Failure<'static>> {
        if self.used == self.capacity {
            let wanted = usize::try_from(self.live).unwrap_or(0).saturating_mul(3);
            let slots = wanted.next_power_of_two().max(MINIMUM_SLOTS);
            self.rebuild(layout, slots)?;
        }
        let hash = key.hash();
        let position = self.used;
        let words = words_per_entry(layout);
        let entry = &mut self.storage[position * words..(position + 1) * words];
        entry[0] = hash;
        let bytes = bytes_of_mut(&mut entry[1..]);
        bytes[..key.bytes.len()].copy_from_slice(key.bytes);
        self.used += 1;
        self.live += 1;
        let position = position_of(position);
        self.place(hash, position);
        Ok(position)
    }

    /// Removes the entry at `position`, which holds a key: marks the entry a hole and its slot
    /// deleted. Compiled code has already moved the value out and dropped the key.
    pub(crate) fn remove(&mut self, layout: &DictLayout, position: i64) {
        let hash = self.hash_at(layout, position);
        let mask = self.index.len() - 1;
        let mut slot = slot_of(hash, mask);
        while self.index[slot] != position {
            if self.index[slot] == EMPTY {
                return;
            }
            slot = (slot + 1) & mask;
        }
        self.index[slot] = DELETED;
        let words = words_per_entry(layout);
        self.storage[at(position) * words] = HOLE;
        self.live -= 1;
    }

    /// The first position at or after `position` that holds a key, or -1 when none does.
    pub(crate) fn next(&self, layout: &DictLayout, position: i64) -> i64 {
        let Ok(start) = usize::try_from(position) else {
            return -1;
        };
        let words = words_per_entry(layout);
        (start..self.used)
            .find(|&at| self.storage[at * words] != HOLE)
            .map_or(-1, position_of)
    }

    /// Rebuilds the table with an index of `slots` slots, copying the entries that hold keys
    /// across in order and leaving the holes behind.
    ///
    /// Cost: one pass over the entries and one insertion into the new index per key.
    fn rebuild(&mut self, layout: &DictLayout, slots: usize) -> Result<(), Failure<'static>> {
        let words = words_per_entry(layout);
        let capacity = slots * 2 / 3;
        let mut storage = Vec::new();
        let total = capacity.saturating_mul(words);
        storage
            .try_reserve_exact(total)
            .map_err(|_| Failure::OutOfMemory {
                bytes: total.saturating_mul(8),
            })?;
        let mut index = Vec::new();
        index
            .try_reserve_exact(slots)
            .map_err(|_| Failure::OutOfMemory {
                bytes: slots.saturating_mul(8),
            })?;
        for entry in self.storage.chunks(words).take(self.used) {
            if entry[0] != HOLE {
                storage.extend_from_slice(entry);
            }
        }
        let used = storage.len() / words;
        storage.resize(total, 0);
        index.resize(slots, EMPTY);
        self.storage = storage;
        self.index = index;
        self.used = used;
        self.capacity = capacity;
        self.entries = self.storage.as_mut_ptr();
        for position in 0..used {
            let hash = self.storage[position * words];
            self.place(hash, position_of(position));
        }
        Ok(())
    }

    /// Puts `position` in the first empty or deleted slot of `hash`'s probe sequence. There is
    /// always one: at most two thirds of the slots hold a position or are deleted.
    fn place(&mut self, hash: u64, position: i64) {
        let mask = self.index.len() - 1;
        let mut slot = slot_of(hash, mask);
        while self.index[slot] >= 0 {
            slot = (slot + 1) & mask;
        }
        self.index[slot] = position;
    }

    /// The hash word of the entry at `position`.
    fn hash_at(&self, layout: &DictLayout, position: i64) -> u64 {
        self.storage[at(position) * words_per_entry(layout)]
    }

    /// The key of the entry at `position`.
    fn key_at<'a>(&'a self, layout: &DictLayout, position: i64, parts: &'a [KeyPart]) -> Key<'a> {
        let words = words_per_entry(layout);
        let start = at(position) * words + 1;
        let bytes = bytes_of(&self.storage[start..start + words - 1]);
        Key {
            bytes: &bytes[..offset(layout.key_size).min(bytes.len())],
            parts,
        }
    }
}

/// Mixes `word` into the hash `state`: the step of the Fx hash, which multiplies by an odd
/// constant so every input bit reaches the high bits.
fn mix(state: u64, word: u64) -> u64 {
    (state.rotate_left(5) ^ word).wrapping_mul(0x517C_C1B7_2722_0A95)
}

/// The 64-bit finalizer of `MurmurHash3`, which spreads the high bits [`mix`] fills into the low
/// bits the index is probed with.
fn finish(mut state: u64) -> u64 {
    state ^= state >> 33;
    state = state.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    state ^= state >> 33;
    state = state.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    state ^ (state >> 33)
}

/// The first slot `hash` probes in an index of `mask + 1` slots.
fn slot_of(hash: u64, mask: usize) -> usize {
    offset(hash) & mask
}

/// The position `at` as compiled code holds it, an `i64`. A table never has more entries than
/// fit in memory, so the conversion never saturates in practice.
fn position_of(at: usize) -> i64 {
    i64::try_from(at).unwrap_or(i64::MAX)
}

/// The index into the entries of `position`, which is not negative.
fn at(position: i64) -> usize {
    usize::try_from(position).unwrap_or(usize::MAX)
}

/// The number of 64-bit words in one entry of `layout`.
fn words_per_entry(layout: &DictLayout) -> usize {
    offset(layout.entry_size) / 8
}

/// `value` as a `usize`. Every target is 64-bit, so nothing is lost.
fn offset(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// The bytes of `words`, in memory order.
fn bytes_of(words: &[u64]) -> &[u8] {
    // SAFETY: a `u64` slice is readable as bytes: `u8` has alignment 1, every byte of an
    // initialized `u64` is initialized, and the length is the same memory's size in bytes.
    #[allow(unsafe_code, reason = "views an entry's words as bytes")]
    unsafe {
        std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), size_of_val(words))
    }
}

/// The bytes of `words`, in memory order, to write.
fn bytes_of_mut(words: &mut [u64]) -> &mut [u8] {
    // SAFETY: as in `bytes_of`; any byte pattern is a valid `u64`, so writing bytes keeps the
    // words valid, and the borrow is exclusive.
    #[allow(unsafe_code, reason = "views an entry's words as bytes to write a key")]
    unsafe {
        std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), size_of_val(words))
    }
}

#[cfg(test)]
mod tests;
