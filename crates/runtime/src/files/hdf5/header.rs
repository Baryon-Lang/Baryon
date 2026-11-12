//! Object headers: the list of messages that says what a group or a dataset is.
//!
//! Version 1 headers, which h5py writes by default, begin with a 16-byte prefix and hold
//! messages each aligned to 8 bytes. Version 2 headers, which `libver="latest"` writes, begin
//! with the signature `OHDR`, pack their messages with no alignment, and end with a checksum.
//! Either kind may continue in more blocks, each named by a continuation message; a version 2
//! continuation block begins with `OCHK` and ends with a checksum too.

use super::cursor::{Cursor, Failure, Source, checked};

/// A continuation message: where more of the header's messages are.
const CONTINUATION: u16 = 0x10;

/// One message of an object header.
#[derive(Clone, Debug)]
pub(super) struct Message {
    /// Its type, as the format numbers it: 1 for a dataspace, 3 for a datatype and so on.
    pub(super) kind: u16,
    /// Its flags; bit 1 marks a message shared with other objects and stored elsewhere.
    pub(super) flags: u8,
    /// Its bytes.
    pub(super) data: Vec<u8>,
}

/// The messages of the object header at `address`, continuations followed.
///
/// # Errors
///
/// A header of a version the format does not define, one whose checksum does not match, or
/// one cut short is damaged.
pub(super) fn read_header(source: &mut Source, address: u64) -> Result<Vec<Message>, Failure> {
    let prefix = source.bytes(address, 16)?;
    if prefix[..4] == *b"OHDR" {
        return late(source, address, &prefix);
    }
    if prefix[0] != 1 {
        return Err(format!(
            "the HDF5 file has an object header of version {}, which the format does not \
             define",
            prefix[0]
        ));
    }
    let mut cursor = source.cursor(&prefix);
    cursor.skip(2)?;
    let count = cursor.u16()?;
    cursor.skip(4)?;
    let size = cursor.u32()?;
    let mut blocks = vec![(address + 16, u64::from(size))];
    let mut messages = Vec::new();
    let mut seen = 0;
    while !blocks.is_empty() {
        let (at, length) = blocks.remove(0);
        let bytes = source.bytes(at, length)?;
        let mut cursor = source.cursor(&bytes);
        while cursor.rest().len() >= 8 && seen < count {
            seen += 1;
            let kind = cursor.u16()?;
            let size = cursor.u16()?;
            let flags = cursor.u8()?;
            cursor.skip(3)?;
            let data = cursor.take(usize::from(size))?.to_vec();
            cursor.align(8)?;
            push(
                source,
                &mut messages,
                &mut blocks,
                Message { kind, flags, data },
            )?;
        }
    }
    Ok(messages)
}

/// A version 2 header at `address`, whose first 16 bytes are `prefix`.
fn late(source: &mut Source, address: u64, prefix: &[u8]) -> Result<Vec<Message>, Failure> {
    let flags = prefix[5];
    // The signature, version and flags, then the times and the attribute phase change values
    // when the flags say they are there.
    let start: usize =
        6 + if flags & 0x20 != 0 { 16 } else { 0 } + if flags & 0x10 != 0 { 4 } else { 0 };
    let width = 1_usize << (flags & 3);
    let size = {
        let bytes = source.bytes(address + start as u64, width as u64)?;
        source.cursor(&bytes).sized(width)?
    };
    let whole = source.bytes(address, (start + width) as u64 + size + 4)?;
    checked(&whole, "object header")?;
    let body = start + width;
    let mut blocks = Vec::new();
    let mut messages = Vec::new();
    let order = flags & 0x04 != 0;
    let mut bytes = whole[body..whole.len() - 4].to_vec();
    loop {
        let mut cursor = source.cursor(&bytes);
        let mut found = Vec::new();
        while cursor.rest().len() >= 4 + if order { 2 } else { 0 } {
            found.push(message(&mut cursor, order)?);
        }
        for message in found {
            push(source, &mut messages, &mut blocks, message)?;
        }
        if blocks.is_empty() {
            break;
        }
        let (at, length) = blocks.remove(0);
        let block = source.bytes(at, length)?;
        if block.len() < 8 || block[..4] != *b"OCHK" {
            return Err(
                "the HDF5 file has no object header continuation where one should be".to_owned(),
            );
        }
        checked(&block, "object header continuation")?;
        bytes = block[4..block.len() - 4].to_vec();
    }
    Ok(messages)
}

/// The next message of a version 2 header, whose messages carry a creation order when
/// `order` is set.
fn message(cursor: &mut Cursor<'_>, order: bool) -> Result<Message, Failure> {
    let kind = u16::from(cursor.u8()?);
    let size = cursor.u16()?;
    let flags = cursor.u8()?;
    if order {
        cursor.skip(2)?;
    }
    let data = cursor.take(usize::from(size))?.to_vec();
    Ok(Message { kind, flags, data })
}

/// Adds `message` to `messages`, or, for a continuation, its block to `blocks`; null messages,
/// which fill space, are dropped.
fn push(
    source: &Source,
    messages: &mut Vec<Message>,
    blocks: &mut Vec<(u64, u64)>,
    message: Message,
) -> Result<(), Failure> {
    match message.kind {
        0 => {}
        CONTINUATION => {
            let mut cursor = source.cursor(&message.data);
            let at = cursor.address()?;
            let length = cursor.length()?;
            blocks.push((at, length));
        }
        _ => messages.push(message),
    }
    Ok(())
}
