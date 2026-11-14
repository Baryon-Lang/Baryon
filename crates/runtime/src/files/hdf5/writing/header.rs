//! Version 1 object headers, built the way the HDF5 library 2.0 builds them (`H5Oalloc.c`),
//! so every message lands where h5py's would.
//!
//! A version 1 header is a 16-byte prefix and a list of messages, each an 8-byte message
//! header and a body padded to a multiple of 8 bytes. They live in *chunks*: the first right
//! after the prefix, the others anywhere in the file, each pointed to by a continuation message.
//! Unused space is a *null* message. A new message takes the smallest null message that holds
//! it, splitting off the rest; when none does, a chunk grows in place if its space allows, and
//! otherwise a new chunk starts, which the continuation message pointing to it, and messages
//! moved to make room for that, take space for. Removing a message makes it null, and the header
//! is then *condensed* (the `condense` module).
//!
//! Only the messages' places and sizes matter until the file is written: the library writes a
//! header's bytes only when it leaves the metadata cache, at the latest when the file closes,
//! from the messages it holds then, with zeros between them.

use super::space::{Space, Use};

/// The size of a message's header: type, size, flags and 3 reserved bytes.
pub(super) const MESSAGE_HEADER: u64 = 8;
/// The size of the prefix before the first chunk's messages.
const PREFIX: u64 = 16;
/// The smallest a chunk's messages may be (`H5O_MIN_SIZE`).
const MINIMUM: u64 = 22;
/// The size of a continuation message's body: the chunk's address and length.
pub(super) const CONTINUATION_SIZE: u64 = 16;
/// The type number of a continuation message.
const CONTINUATION: u16 = 0x10;
/// The type number of an attribute message.
const ATTRIBUTE: u16 = 0x0C;

/// `n` rounded up to a multiple of 8.
pub(super) fn align8(n: u64) -> u64 {
    n.next_multiple_of(8)
}

/// What a message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// Unused space.
    Null,
    /// A null message merged into another, kept in the list until the header is condensed.
    Deleted,
    /// A continuation message: the index of the chunk it points to.
    Continuation(usize),
    /// Any other message, by its type number.
    Message(u16),
}

/// One message.
#[derive(Clone, Debug)]
pub(super) struct Message {
    /// Its identity, which stays the same while messages move.
    pub(super) id: usize,
    /// What it is.
    pub(super) kind: Kind,
    /// The size of its body in bytes, a multiple of 8.
    pub(super) size: u64,
    /// The chunk it is in.
    pub(super) chunk: usize,
    /// Where its body starts, counted from the chunk's start (the prefix's, in the first).
    pub(super) offset: u64,
    /// Its body's bytes, at most `size` of them.
    pub(super) body: Vec<u8>,
    /// Its flags: 1 when constant.
    pub(super) flags: u8,
}

/// An object header being built.
#[derive(Clone, Debug)]
pub(super) struct Header {
    /// Its address.
    pub(super) address: u64,
    /// Each chunk's address and size, the first's size counting the prefix.
    pub(super) chunks: Vec<(u64, u64)>,
    /// The messages, in the library's order, which is the order the file lists them.
    pub(super) messages: Vec<Message>,
    /// The next message identity to hand out.
    next_id: usize,
}

impl Header {
    /// A new header with room for `hint` bytes of messages, its space taken from `space`.
    pub(super) fn new(space: &mut Space, hint: u64) -> Self {
        let hint = align8(hint.max(MINIMUM));
        let size = PREFIX + hint;
        let address = space.allocate(Use::Metadata, size);
        let mut header = Self {
            address,
            chunks: vec![(address, size)],
            messages: Vec::new(),
            next_id: 0,
        };
        header.push(
            Kind::Null,
            hint - MESSAGE_HEADER,
            0,
            PREFIX + MESSAGE_HEADER,
        );
        header
    }

    /// Adds a message to the end of the list and returns its index.
    pub(super) fn push(&mut self, kind: Kind, size: u64, chunk: usize, offset: u64) -> usize {
        self.messages.push(Message {
            id: self.next_id,
            kind,
            size,
            chunk,
            offset,
            body: Vec::new(),
            flags: 0,
        });
        self.next_id += 1;
        self.messages.len() - 1
    }

    /// The index of the message `id`.
    fn index(&self, id: usize) -> Option<usize> {
        self.messages.iter().position(|message| message.id == id)
    }

    /// Replaces the body of the message `id`, which keeps its size.
    pub(super) fn set_body(&mut self, id: usize, body: Vec<u8>) {
        if let Some(index) = self.index(id) {
            self.messages[index].body = body;
        }
    }

    /// Adds a message of type `kind` with `body` and `flags` and returns its identity
    /// (`H5O__alloc`).
    pub(super) fn add(&mut self, space: &mut Space, kind: u16, body: Vec<u8>, flags: u8) -> usize {
        let size = align8(body.len() as u64);
        let index = match self.best_null(size) {
            Some(index) => index,
            None => (0..self.chunks.len())
                .find_map(|chunk| self.extend_chunk(space, chunk, size))
                .unwrap_or_else(|| self.new_chunk(space, size)),
        };
        self.take_null(index, Kind::Message(kind), size);
        let message = &mut self.messages[index];
        message.body = body;
        message.flags = flags;
        message.id
    }

    /// The null message that best holds `size` bytes: the smallest, then the earliest chunk
    /// (`H5O__alloc_null` search in `H5O__alloc`).
    fn best_null(&self, size: u64) -> Option<usize> {
        let mut found: Option<usize> = None;
        for (index, message) in self.messages.iter().enumerate() {
            if message.kind != Kind::Null || message.size < size {
                continue;
            }
            let Some(best) = found.map(|best| &self.messages[best]) else {
                found = Some(index);
                continue;
            };
            if message.size == size {
                if message.chunk < best.chunk {
                    found = Some(index);
                    if message.chunk == 0 {
                        break;
                    }
                }
            } else if message.size < best.size
                || (message.size == best.size && message.chunk < best.chunk)
            {
                found = Some(index);
            }
        }
        found
    }

    /// Makes the null message at `index` a `kind` of `size` bytes, splitting off the rest as a
    /// new null message (`H5O__alloc_null`).
    fn take_null(&mut self, index: usize, kind: Kind, size: u64) {
        let message = &self.messages[index];
        if message.size > size {
            let rest = message.size - size;
            let (chunk, offset) = (message.chunk, message.offset + size + MESSAGE_HEADER);
            self.push(Kind::Null, rest - MESSAGE_HEADER, chunk, offset);
        }
        let message = &mut self.messages[index];
        message.size = size;
        message.kind = kind;
        message.body.clear();
        message.flags = 0;
    }

    /// Grows `chunk` in place to hold a `size`-byte message, and returns the null message that
    /// holds it, or `None` when the space after the chunk is taken (`H5O__alloc_extend_chunk`).
    fn extend_chunk(&mut self, space: &mut Space, chunk: usize, size: u64) -> Option<usize> {
        let (address, length) = self.chunks[chunk];
        let last = self.messages.iter().position(|message| {
            message.chunk == chunk
                && message.kind == Kind::Null
                && message.offset + message.size == length
        });
        let delta = align8(match last {
            Some(index) => size - self.messages[index].size,
            None => size + MESSAGE_HEADER,
        });
        if !space.try_extend(Use::Metadata, address, length, delta) {
            return None;
        }
        self.chunks[chunk].1 += delta;
        Some(match last {
            Some(index) => {
                self.messages[index].size += delta;
                index
            }
            None => self.push(
                Kind::Null,
                delta - MESSAGE_HEADER,
                chunk,
                length + MESSAGE_HEADER,
            ),
        })
    }

    /// The message to move into a new chunk to make room for its continuation message, with
    /// the null message right after it, and the size the new chunk then needs
    /// (`H5O__alloc_find_best_nonnull`).
    fn best_to_move(&self, size: u64) -> (u64, Option<(usize, Option<usize>)>) {
        let mut found: Option<(usize, Option<usize>, u64)> = None;
        let mut moved_together = 0;
        for (index, message) in self.messages.iter().enumerate() {
            if matches!(message.kind, Kind::Continuation(_) | Kind::Deleted) {
                continue;
            }
            let end = message.offset + message.size;
            let after = if end == self.chunks[message.chunk].1 {
                None
            } else {
                self.messages.iter().position(|null| {
                    null.kind == Kind::Null
                        && null.chunk == message.chunk
                        && null.offset - MESSAGE_HEADER == end
                })
            };
            let total =
                message.size + after.map_or(0, |null| MESSAGE_HEADER + self.messages[null].size);
            if total >= CONTINUATION_SIZE {
                let better = match found {
                    None => true,
                    Some((best, _, best_total)) => {
                        let best = &self.messages[best];
                        (best.kind == Kind::Message(ATTRIBUTE)
                            && message.kind != Kind::Message(ATTRIBUTE))
                            || total < best_total
                            || (total == best_total && message.chunk < best.chunk)
                    }
                };
                if better {
                    found = Some((index, after, total));
                }
            } else if found.is_none() && message.chunk == self.chunks.len() - 1 {
                moved_together += message.size + MESSAGE_HEADER;
            }
        }
        match found {
            None => (size + moved_together, None),
            Some((index, after, _)) => (
                size + MESSAGE_HEADER + self.messages[index].size,
                Some((index, after)),
            ),
        }
    }

    /// Starts a new chunk with room for a `size`-byte message and a continuation message
    /// pointing to it, and returns the null message that holds `size` bytes
    /// (`H5O__alloc_chunk`).
    fn new_chunk(&mut self, space: &mut Space, size: u64) -> usize {
        let size = align8(size);
        let mut continuation = self.best_null(CONTINUATION_SIZE);
        let (size, found) = match continuation {
            Some(_) => (size, None),
            None => self.best_to_move(size),
        };
        let length = MINIMUM.max(size + MESSAGE_HEADER);
        let address = space.allocate(Use::Metadata, length);
        let chunk = self.chunks.len();
        self.chunks.push((address, length));
        let mut at = 0;
        if continuation.is_none() {
            match found {
                None => {
                    // Every message of the last chunk moves to the new one.
                    let previous = chunk - 1;
                    for message in &mut self.messages {
                        if message.chunk != previous {
                            continue;
                        }
                        if message.kind == Kind::Null {
                            message.kind = Kind::Deleted;
                        } else {
                            message.chunk = chunk;
                            message.offset = at + MESSAGE_HEADER;
                            at += MESSAGE_HEADER + message.size;
                        }
                    }
                    let start = if previous == 0 { PREFIX } else { 0 };
                    let free = self.chunks[previous].1 - start - MESSAGE_HEADER;
                    continuation =
                        Some(self.push(Kind::Null, free, previous, start + MESSAGE_HEADER));
                }
                Some((index, after)) => {
                    let (moved_size, moved_chunk, moved_offset) = {
                        let moved = &self.messages[index];
                        (moved.size, moved.chunk, moved.offset)
                    };
                    let null = self.push(Kind::Null, moved_size, moved_chunk, moved_offset);
                    let moved = &mut self.messages[index];
                    moved.chunk = chunk;
                    moved.offset = at + MESSAGE_HEADER;
                    at += MESSAGE_HEADER + moved_size;
                    if let Some(after) = after {
                        let extra = MESSAGE_HEADER + self.messages[after].size;
                        self.messages[null].size += extra;
                        self.messages[after].kind = Kind::Deleted;
                    }
                    continuation = Some(null);
                }
            }
        }
        let rest = self.push(
            Kind::Null,
            length - at - MESSAGE_HEADER,
            chunk,
            at + MESSAGE_HEADER,
        );
        if let Some(index) = continuation {
            self.take_null(index, Kind::Continuation(chunk), CONTINUATION_SIZE);
        }
        rest
    }

    /// The header's bytes: each chunk's address and image.
    pub(super) fn encode(&self) -> Vec<(u64, Vec<u8>)> {
        let live = self
            .messages
            .iter()
            .filter(|message| message.kind != Kind::Deleted)
            .count();
        let mut images: Vec<(u64, Vec<u8>)> = self
            .chunks
            .iter()
            .map(|&(address, length)| (address, vec![0; to_usize(length)]))
            .collect();
        let first = &mut images[0].1;
        first[0] = 1;
        first[2..4].copy_from_slice(&u16::try_from(live).unwrap_or(u16::MAX).to_le_bytes());
        first[4..8].copy_from_slice(&1_u32.to_le_bytes());
        let size = u32::try_from(self.chunks[0].1 - PREFIX).unwrap_or(u32::MAX);
        first[8..12].copy_from_slice(&size.to_le_bytes());
        for message in &self.messages {
            let (kind, body) = match message.kind {
                Kind::Deleted => continue,
                Kind::Null => (0, Vec::new()),
                Kind::Continuation(chunk) => {
                    let (address, length) = self.chunks[chunk];
                    let mut body = address.to_le_bytes().to_vec();
                    body.extend_from_slice(&length.to_le_bytes());
                    (CONTINUATION, body)
                }
                Kind::Message(kind) => (kind, message.body.clone()),
            };
            let image = &mut images[message.chunk].1;
            let at = to_usize(message.offset);
            let start = at - to_usize(MESSAGE_HEADER);
            image[start..start + 2].copy_from_slice(&kind.to_le_bytes());
            let size = u16::try_from(message.size).unwrap_or(u16::MAX);
            image[start + 2..start + 4].copy_from_slice(&size.to_le_bytes());
            image[start + 4] = message.flags;
            image[at..at + body.len()].copy_from_slice(&body);
        }
        images
    }
}

/// `n` as a `usize`: every size here is of a structure held in memory.
pub(super) fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}
