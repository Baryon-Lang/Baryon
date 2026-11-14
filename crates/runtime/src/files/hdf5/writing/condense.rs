//! Removing a message from an object header and condensing the header afterwards, as the HDF5
//! library 2.0 does when h5py replaces an attribute (`H5O__condense_header` and the functions
//! it calls in `H5Oalloc.c`).
//!
//! Condensing repeats three steps until none changes anything: move each message up into the
//! null message just before it, or into a null message of an earlier chunk that holds it, and
//! pull the messages of the last chunk back into its continuation message when they fit; merge
//! null messages that touch; and drop chunks that hold only a null message, with the
//! continuation message that points to them. Each dropped chunk's space goes back to the file.

use super::header::{Header, Kind, MESSAGE_HEADER, Message};
use super::space::{Space, Use};

impl Header {
    /// Removes the message `id` and condenses the header (`H5O__attr_remove`).
    pub(super) fn remove(&mut self, space: &mut Space, id: usize) {
        if let Some(message) = self.messages.iter_mut().find(|message| message.id == id) {
            release(message);
        }
        self.condense(space);
    }

    /// Condenses the header (`H5O__condense_header`).
    fn condense(&mut self, space: &mut Space) {
        self.messages
            .retain(|message| message.kind != Kind::Deleted);
        loop {
            let moved = self.move_forward(space);
            let merged = self.merge_nulls(space);
            let dropped = self.drop_empty_chunks(space);
            if !(moved || merged || dropped) {
                break;
            }
        }
    }

    /// Moves messages towards the start of the header (`H5O__move_msgs_forward`). Returns
    /// whether any moved.
    fn move_forward(&mut self, space: &mut Space) -> bool {
        let mut any = false;
        loop {
            let mut packed = false;
            let mut index = 0;
            while index < self.messages.len() {
                let message = &self.messages[index];
                if message.kind == Kind::Null {
                    let end = message.offset + message.size;
                    if end != self.chunks[message.chunk].1
                        && let Some(next) = self.messages.iter().position(|next| {
                            next.chunk == message.chunk && next.offset - MESSAGE_HEADER == end
                        })
                        && self.messages[next].kind != Kind::Null
                    {
                        // The next message moves up over the null message.
                        let offset = self.messages[index].offset;
                        let next_size = self.messages[next].size;
                        self.messages[next].offset = offset;
                        self.messages[index].offset = offset + next_size + MESSAGE_HEADER;
                        packed = true;
                    }
                } else {
                    let (kind, chunk, size) = (message.kind, message.chunk, message.size);
                    if matches!(kind, Kind::Continuation(_)) && self.pull_last_chunk(space, index) {
                        packed = true;
                        break;
                    }
                    if let Some(null) = self.messages.iter().position(|null| {
                        null.kind == Kind::Null && chunk > null.chunk && size <= null.size
                    }) {
                        self.move_into(index, null);
                        packed = true;
                    }
                    // Any move so far, here or by a null message before, starts the scan again.
                    if packed {
                        break;
                    }
                }
                index += 1;
            }
            if !packed {
                return any;
            }
            any = true;
        }
    }

    /// Moves the message at `index` into the null message at `null`, in an earlier chunk,
    /// leaving a null message where it was.
    fn move_into(&mut self, index: usize, null: usize) {
        let (old_chunk, old_offset, size) = {
            let message = &self.messages[index];
            (message.chunk, message.offset, message.size)
        };
        let (null_chunk, null_offset, null_size) = {
            let null = &self.messages[null];
            (null.chunk, null.offset, null.size)
        };
        let message = &mut self.messages[index];
        message.chunk = null_chunk;
        message.offset = null_offset;
        if size == null_size {
            let null = &mut self.messages[null];
            null.chunk = old_chunk;
            null.offset = old_offset;
        } else {
            let null = &mut self.messages[null];
            null.offset += size + MESSAGE_HEADER;
            null.size -= size + MESSAGE_HEADER;
            self.push(Kind::Null, size, old_chunk, old_offset);
        }
    }

    /// Moves the messages of the last chunk into the continuation message at `index` that
    /// points to it when they fit, and drops the chunk (`H5O__move_cont`). Returns whether it
    /// did.
    fn pull_last_chunk(&mut self, space: &mut Space, index: usize) -> bool {
        let Kind::Continuation(last) = self.messages[index].kind else {
            return false;
        };
        if last != self.chunks.len() - 1 {
            return false;
        }
        let needed: u64 = self
            .messages
            .iter()
            .filter(|message| message.chunk == last && message.kind != Kind::Null)
            .map(|message| message.size + MESSAGE_HEADER)
            .sum();
        let continuation = &self.messages[index];
        if needed == 0 || needed > continuation.size + MESSAGE_HEADER {
            return false;
        }
        let mut at = continuation.offset - MESSAGE_HEADER;
        let end = continuation.offset + continuation.size;
        let chunk = continuation.chunk;
        release(&mut self.messages[index]);
        for message in &mut self.messages {
            if message.chunk == last && message.kind != Kind::Null {
                message.offset = at + MESSAGE_HEADER;
                message.chunk = chunk;
                at += message.size + MESSAGE_HEADER;
            }
        }
        let (address, length) = self.chunks[last];
        space.free(Use::Metadata, address, length);
        let gap = end - at;
        if gap >= MESSAGE_HEADER {
            let null = &mut self.messages[index];
            null.size = gap - MESSAGE_HEADER;
            null.offset = at + MESSAGE_HEADER;
        } else {
            self.messages.remove(index);
        }
        self.messages
            .retain(|message| !(message.chunk == last && message.kind == Kind::Null));
        self.chunks.pop();
        true
    }

    /// Merges null messages that touch (`H5O__merge_null`). Returns whether any merged.
    fn merge_nulls(&mut self, space: &mut Space) -> bool {
        let mut any = false;
        'again: loop {
            for first in 0..self.messages.len() {
                if self.messages[first].kind != Kind::Null {
                    continue;
                }
                for second in 0..self.messages.len() {
                    let (a, b) = (&self.messages[first], &self.messages[second]);
                    if first == second || b.kind != Kind::Null || a.chunk != b.chunk {
                        continue;
                    }
                    let joined = if a.offset + a.size == b.offset - MESSAGE_HEADER {
                        Some(a.offset)
                    } else if a.offset - MESSAGE_HEADER == b.offset + b.size {
                        Some(b.offset)
                    } else {
                        None
                    };
                    if let Some(offset) = joined {
                        let extra = MESSAGE_HEADER + b.size;
                        let a = &mut self.messages[first];
                        a.offset = offset;
                        a.size += extra;
                        self.messages.remove(second);
                        self.drop_empty_chunks(space);
                        any = true;
                        continue 'again;
                    }
                }
            }
            return any;
        }
    }

    /// Drops each chunk after the first that holds only a null message, with the continuation
    /// message pointing to it, and frees its space (`H5O__remove_empty_chunks`). Returns
    /// whether any went.
    fn drop_empty_chunks(&mut self, space: &mut Space) -> bool {
        let mut any = false;
        while let Some(null) = self.messages.iter().position(|message| {
            message.kind == Kind::Null
                && message.chunk > 0
                && MESSAGE_HEADER + message.size == self.chunks[message.chunk].1
        }) {
            let chunk = self.messages[null].chunk;
            if let Some(continuation) = self
                .messages
                .iter_mut()
                .find(|message| message.kind == Kind::Continuation(chunk))
            {
                release(continuation);
            }
            let (address, length) = self.chunks.remove(chunk);
            space.free(Use::Metadata, address, length);
            self.messages.remove(null);
            for message in &mut self.messages {
                if message.chunk > chunk {
                    message.chunk -= 1;
                }
                if let Kind::Continuation(target) = &mut message.kind
                    && *target > chunk
                {
                    *target -= 1;
                }
            }
            any = true;
        }
        any
    }
}

/// Makes `message` a null message (`H5O__release_mesg`).
fn release(message: &mut Message) {
    message.kind = Kind::Null;
    message.body.clear();
    message.flags = 0;
}
