//! A small deflate decoder (RFC 1951) for the tests that read back the PNG images `byplot`
//! embeds in SVG files (roadmap 3.15.9), so they need no dependency and no Python. It decodes
//! stored, fixed-Huffman and dynamic-Huffman blocks, and gives `None` for anything malformed,
//! which the caller turns into a failed test.

/// Reads bits from a deflate stream, least significant first, as RFC 1951 packs them.
struct Bits<'a> {
    /// The stream.
    bytes: &'a [u8],
    /// The next bit to read, counted from the start of `bytes`.
    at: usize,
}

impl Bits<'_> {
    /// The next `count` bits, the first read the least significant.
    fn take(&mut self, count: u32) -> usize {
        let mut value = 0;
        for k in 0..count {
            let byte = self.bytes[self.at / 8];
            value |= usize::from((byte >> (self.at % 8)) & 1) << k;
            self.at += 1;
        }
        value
    }

    /// Moves to the next whole byte, as a stored block's header does.
    fn align(&mut self) {
        self.at = self.at.div_ceil(8) * 8;
    }
}

/// A canonical Huffman code, as RFC 1951 section 3.2.2 builds it from code lengths: how many
/// codes each length has, and the symbols in code order.
struct Huffman {
    /// `counts[n]` is how many symbols have a code of `n` bits.
    counts: [u16; 16],
    /// The symbols with a code, shortest codes first, then by symbol.
    symbols: Vec<u16>,
}

impl Huffman {
    /// The code for `lengths`, one per symbol, 0 for a symbol with no code.
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0_u16; 16];
        for &length in lengths {
            counts[usize::from(length)] += 1;
        }
        counts[0] = 0;
        let mut symbols = Vec::new();
        for length in 1..16 {
            for (symbol, &each) in lengths.iter().enumerate() {
                if usize::from(each) == length {
                    symbols.push(u16::try_from(symbol).ok()?);
                }
            }
        }
        Some(Self { counts, symbols })
    }

    /// The next symbol in `bits`, read a bit at a time down the canonical code, or `None` for
    /// a code longer than 15 bits.
    fn decode(&self, bits: &mut Bits) -> Option<usize> {
        let (mut code, mut first, mut index) = (0_usize, 0_usize, 0_usize);
        for length in 1..16 {
            code |= bits.take(1);
            let count = usize::from(self.counts[length]);
            if code < first + count {
                return Some(usize::from(self.symbols[index + code - first]));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

/// The base lengths of length symbols 257 to 285, and their extra bits.
const LENGTHS: [(usize, u32); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];

/// The base distances of distance symbols 0 to 29, and their extra bits.
const DISTANCES: [(usize, u32); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

/// The order in which a dynamic block lists its code-length code's lengths.
const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// The bytes the zlib stream `stream` holds, after its 2-byte header, and the position of its
/// Adler-32 checksum, the four bytes after the last block. `None` for a malformed stream.
pub(super) fn zlib(stream: &[u8]) -> Option<(Vec<u8>, usize)> {
    assert_eq!(stream[0] & 0x0f, 8, "deflate");
    let mut bits = Bits {
        bytes: stream,
        at: 16,
    };
    let mut out = Vec::new();
    loop {
        let last = bits.take(1);
        match bits.take(2) {
            0 => {
                bits.align();
                let at = bits.at / 8;
                let length = usize::from(u16::from_le_bytes([stream[at], stream[at + 1]]));
                out.extend_from_slice(&stream[at + 4..at + 4 + length]);
                bits.at = (at + 4 + length) * 8;
            }
            1 => {
                let mut lengths = [8_u8; 288];
                lengths[144..256].fill(9);
                lengths[256..280].fill(7);
                let literals = Huffman::new(&lengths)?;
                let distances = Huffman::new(&[5; 30])?;
                codes(&mut bits, &mut out, &literals, &distances)?;
            }
            2 => {
                let (literals, distances) = dynamic(&mut bits)?;
                codes(&mut bits, &mut out, &literals, &distances)?;
            }
            _ => return None,
        }
        if last == 1 {
            break;
        }
    }
    bits.align();
    Some((out, bits.at / 8))
}

/// The literal and distance codes a dynamic block's header describes.
fn dynamic(bits: &mut Bits) -> Option<(Huffman, Huffman)> {
    let literal_count = bits.take(5) + 257;
    let distance_count = bits.take(5) + 1;
    let code_count = bits.take(4) + 4;
    let mut code_lengths = [0_u8; 19];
    for &place in &ORDER[..code_count] {
        code_lengths[place] = u8::try_from(bits.take(3)).ok()?;
    }
    let lengths_code = Huffman::new(&code_lengths)?;
    let mut lengths = Vec::new();
    while lengths.len() < literal_count + distance_count {
        let symbol = lengths_code.decode(bits)?;
        let (value, repeat) = match symbol {
            0..=15 => (u8::try_from(symbol).ok()?, 1),
            16 => (*lengths.last()?, 3 + bits.take(2)),
            17 => (0, 3 + bits.take(3)),
            _ => (0, 11 + bits.take(7)),
        };
        lengths.extend(std::iter::repeat_n(value, repeat));
    }
    Some((
        Huffman::new(&lengths[..literal_count])?,
        Huffman::new(&lengths[literal_count..])?,
    ))
}

/// Decodes one block's literals and copies into `out`, up to its end-of-block symbol. `None`
/// for a code that is not in the block's codes.
fn codes(
    bits: &mut Bits,
    out: &mut Vec<u8>,
    literals: &Huffman,
    distances: &Huffman,
) -> Option<()> {
    loop {
        let symbol = literals.decode(bits)?;
        match symbol {
            0..=255 => out.push(u8::try_from(symbol).ok()?),
            256 => return Some(()),
            _ => {
                let (base, extra) = *LENGTHS.get(symbol - 257)?;
                let length = base + bits.take(extra);
                let (base, extra) = *DISTANCES.get(distances.decode(bits)?)?;
                let distance = base + bits.take(extra);
                let start = out.len().checked_sub(distance)?;
                for k in 0..length {
                    out.push(out[start + k]);
                }
            }
        }
    }
}
