//! What a chunked dataset's chunks pass through before they are stored, and the shape of its
//! chunks when the program gives none: h5py's `shuffle=`, `compression="gzip"` and
//! `fletcher32=` (`H5Zshuffle.c`, `H5Zdeflate.c`, `H5Zfletcher32.c` and `H5checksum.c` in the
//! HDF5 library 2.0) and h5py's `guess_chunk` (`h5py/_hl/filters.py`).
//!
//! The runtime does not compress: the caller deflates each shuffled chunk with the standard
//! library's `zlib` and hands over the result. So, as decision 0052's amendment says, a gzip
//! file's layout is h5py's but its compressed bytes are this zlib's, not the C zlib's.

use super::encode::pad8;

/// The filters a chunked dataset's chunks pass through, in the order h5py adds them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Filters {
    /// `shuffle=True`: each chunk's bytes are regrouped so that the first byte of every
    /// element comes first, then every second byte, and so on.
    pub(super) shuffle: bool,
    /// `compression="gzip"` with its level, 0 to 9: each chunk is deflated, after the
    /// shuffle, by the caller.
    pub(super) deflate: Option<u8>,
    /// `fletcher32=True`: each chunk ends with its Fletcher-32 checksum.
    pub(super) fletcher32: bool,
}

impl Filters {
    /// The filters `text` names, separated by commas: `shuffle`, `gzip=` and a level, and
    /// `fletcher32`.
    ///
    /// # Errors
    ///
    /// Fails on a name that is not one of those.
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        let mut filters = Self::default();
        for name in text.split(',').filter(|name| !name.is_empty()) {
            match name {
                "shuffle" => filters.shuffle = true,
                "fletcher32" => filters.fletcher32 = true,
                _ => {
                    filters.deflate = name
                        .strip_prefix("gzip=")
                        .and_then(|level| level.parse().ok())
                        .filter(|level| *level <= 9);
                    if filters.deflate.is_none() {
                        return Err(format!("not a filter: {name:?}"));
                    }
                }
            }
        }
        Ok(filters)
    }

    /// Whether there are none.
    pub(super) fn is_empty(self) -> bool {
        !self.shuffle && self.deflate.is_none() && !self.fletcher32
    }

    /// The chunk `chunk`, of elements of `element` bytes, shuffled if asked: what is
    /// deflated, or stored when it is not.
    pub(super) fn shuffled(self, chunk: Vec<u8>, element: usize) -> Vec<u8> {
        if self.shuffle {
            shuffle(&chunk, element)
        } else {
            chunk
        }
    }

    /// The chunk `chunk`, already through the filters before it, with its checksum added
    /// if asked: what is stored.
    pub(super) fn finish(self, mut chunk: Vec<u8>) -> Vec<u8> {
        if self.fletcher32 {
            let sum = fletcher32(&chunk);
            chunk.extend_from_slice(&sum.to_le_bytes());
        }
        chunk
    }

    /// The body of the filter pipeline message (version 1) for chunks of elements of
    /// `element` bytes.
    pub(super) fn message(self, element: u8) -> Vec<u8> {
        let mut list: Vec<(u16, &[u8], u16, Vec<u32>)> = Vec::new();
        if self.shuffle {
            // Optional (flag 1), with the element size as its one parameter.
            list.push((2, b"shuffle", 1, vec![u32::from(element)]));
        }
        if let Some(level) = self.deflate {
            // Optional too, with the level as its parameter.
            list.push((1, b"deflate", 1, vec![u32::from(level)]));
        }
        if self.fletcher32 {
            list.push((3, b"fletcher32", 0, Vec::new()));
        }
        let mut body = vec![1, u8::try_from(list.len()).unwrap_or(u8::MAX)];
        body.extend_from_slice(&[0; 6]);
        for (id, name, flags, parameters) in list {
            let mut name = name.to_vec();
            name.push(0);
            let name = pad8(&name);
            for field in [id, u16::try_from(name.len()).unwrap_or(u16::MAX), flags] {
                body.extend_from_slice(&field.to_le_bytes());
            }
            body.extend_from_slice(&u16::try_from(parameters.len()).unwrap_or(0).to_le_bytes());
            body.extend_from_slice(&name);
            for parameter in &parameters {
                body.extend_from_slice(&parameter.to_le_bytes());
            }
            if parameters.len() % 2 == 1 {
                body.extend_from_slice(&[0; 4]);
            }
        }
        body
    }
}

/// `bytes`, elements of `element` bytes, regrouped byte by byte; bytes past the last whole
/// element stay at the end.
fn shuffle(bytes: &[u8], element: usize) -> Vec<u8> {
    if element <= 1 {
        return bytes.to_vec();
    }
    let count = bytes.len() / element;
    let mut shuffled = Vec::with_capacity(bytes.len());
    for byte in 0..element {
        shuffled.extend((0..count).map(|index| bytes[index * element + byte]));
    }
    shuffled.extend_from_slice(&bytes[count * element..]);
    shuffled
}

/// The library's Fletcher-32 checksum of `bytes` (`H5_checksum_fletcher32`): 16-bit words,
/// big-endian, a lone last byte as the high half of one, sums folded every 360 words.
fn fletcher32(bytes: &[u8]) -> u32 {
    let fold = |sum: u32| (sum & 0xffff) + (sum >> 16);
    let (mut first, mut second) = (0_u32, 0_u32);
    let words = bytes.chunks_exact(2);
    let last = words.remainder().first().copied();
    let words: Vec<u32> = words
        .map(|pair| u32::from(u16::from_be_bytes([pair[0], pair[1]])))
        .collect();
    for block in words.chunks(360) {
        for word in block {
            first += word;
            second += first;
        }
        first = fold(first);
        second = fold(second);
    }
    if let Some(byte) = last {
        first += u32::from(byte) << 8;
        second += first;
        first = fold(first);
        second = fold(second);
    }
    (fold(second) << 16) | fold(first)
}

/// h5py's guess at a chunk shape for a dataset of `shape` with elements of `element` bytes:
/// halve the axes in turn, first to last, until a chunk is near a target that grows with the
/// dataset, between 8 KiB and 1 MiB.
pub(super) fn guess(shape: &[u64], element: u8) -> Vec<u64> {
    /// h5py's `CHUNK_BASE`, `CHUNK_MIN` and `CHUNK_MAX`.
    const BASE: f64 = 16.0 * 1024.0;
    /// The smallest target.
    const MIN: f64 = 8.0 * 1024.0;
    /// The largest target, and the size every chunk stays under.
    const MAX: f64 = 1024.0 * 1024.0;
    #[expect(clippy::cast_precision_loss, reason = "h5py computes in doubles too")]
    let mut chunks: Vec<f64> = shape
        .iter()
        .map(|&length| if length == 0 { 1024.0 } else { length as f64 })
        .collect();
    let element = f64::from(element);
    let product = |chunks: &[f64]| chunks.iter().product::<f64>();
    let size = product(&chunks) * element;
    let target = (BASE * 2_f64.powf((size / (1024.0 * 1024.0)).log10())).clamp(MIN, MAX);
    let mut axis = 0;
    loop {
        let bytes = product(&chunks) * element;
        if (bytes < target || (bytes - target).abs() / target < 0.5) && bytes < MAX {
            break;
        }
        // Every length is a whole number of at least 1, so a product of 1 means all are 1.
        if chunks.iter().all(|&length| length <= 1.0) {
            break;
        }
        let at = axis % chunks.len();
        chunks[at] = (chunks[at] / 2.0).ceil();
        axis += 1;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole, positive lengths no larger than the shape's"
    )]
    chunks.iter().map(|&length| length as u64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_groups_bytes() {
        assert_eq!(shuffle(&[1, 2, 3, 4, 5, 6, 7], 2), [1, 3, 5, 2, 4, 6, 7]);
    }

    #[test]
    fn fletcher32_matches_the_library() {
        // The sum the prototype that matched h5py's files gives; an odd length takes the
        // lone last byte path.
        assert_eq!(fletcher32(b"abcde"), 0x4ff0_29c7);
    }

    #[test]
    fn guess_matches_h5py() {
        // What h5py 3.16's `guess_chunk` gives for the same shapes and sizes.
        assert_eq!(guess(&[1000], 8), [1000]);
        assert_eq!(guess(&[100, 100], 8), [25, 50]);
        assert_eq!(guess(&[300, 200, 10], 4), [38, 50, 3]);
    }
}
