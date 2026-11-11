//! FITS files (decision 0044): the header cards of an HDU and the pixels of an image, read and
//! written as the FITS standard 4.0 defines them, in safe code the exported functions in
//! `abi::fits` call. What a header's cards mean to a program is parsed in Baryon, in the
//! standard library's `fits` module; this module reads only what it needs to find and convert
//! the pixels.
//!
//! A FITS file is a sequence of HDUs (header and data units). A header is 80-byte ASCII cards,
//! `KEYWORD = value / comment`, ending with the card `END`, padded with blank cards to a
//! multiple of 2880 bytes. An image's data follows: `NAXIS1 * NAXIS2 * ...` big-endian numbers
//! of the type `BITPIX` names (8 `u8`, 16 `i16`, 32 `i32`, 64 `i64`, -32 `f32`, -64 `f64`),
//! `NAXIS1` varying fastest, padded with zeros to a multiple of 2880 bytes. An extension's data
//! is `|BITPIX| / 8 * GCOUNT * (PCOUNT + NAXIS1 * NAXIS2 * ...)` bytes, which is how a reader
//! skips an HDU it does not want.
//!
//! Cost: a read is the headers of the HDUs before the one wanted, then one pass over its
//! pixels, converted from big-endian as they are copied into the array.

use super::{BUFFER, ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};

/// The size of a FITS block: headers and data are padded to a multiple of it.
pub(crate) const BLOCK: usize = 2880;

/// The size of a header card.
pub(crate) const CARD: usize = 80;

/// What one HDU's header says about its data, and its cards.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hdu {
    /// Every card before `END`, each 80 characters, in the file's order.
    pub(crate) cards: Vec<String>,
    /// Where the data starts, in bytes from the start of the file.
    pub(crate) offset: u64,
    /// The type of the stored numbers: 8, 16, 32, 64, -32 or -64.
    pub(crate) bitpix: i64,
    /// The length of each axis, `NAXIS1` first.
    pub(crate) axes: Vec<usize>,
    /// `BSCALE`, 1 when the header has none.
    pub(crate) bscale: f64,
    /// `BZERO`, 0 when the header has none.
    pub(crate) bzero: f64,
    /// The size of the data in bytes, before its padding.
    pub(crate) size: u64,
}

impl Hdu {
    /// Whether the stored numbers are scaled: `BSCALE` other than 1 or `BZERO` other than 0,
    /// exactly, as the standard's defaults are written.
    #[allow(
        clippy::float_cmp,
        reason = "the defaults are exact values a header writes"
    )]
    fn scaled(&self) -> bool {
        self.bscale != 1.0 || self.bzero != 0.0
    }
}

/// The type of the elements a program reads an image into or writes one from, numbered as the
/// runtime's `ELEMENT_*` constants number them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pixel {
    /// `u8`.
    U8,
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `u16`.
    U16,
    /// `i32`.
    I32,
    /// `u32`.
    U32,
    /// `i64`.
    I64,
    /// `u64`.
    U64,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
}

impl Pixel {
    /// The size of one element in bytes.
    pub(crate) fn size(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    /// How the FITS standard stores this type: `BITPIX` and `BZERO`. A signed `i8` and the
    /// unsigned types wider than a byte are stored as the other signedness with `BZERO`
    /// shifting them, as `AstroPy` stores them.
    pub(crate) fn stored(self) -> (i64, f64) {
        match self {
            Self::U8 => (8, 0.0),
            Self::I8 => (8, -128.0),
            Self::I16 => (16, 0.0),
            Self::U16 => (16, 32768.0),
            Self::I32 => (32, 0.0),
            Self::U32 => (32, 2_147_483_648.0),
            Self::I64 => (64, 0.0),
            Self::U64 => (64, 9_223_372_036_854_775_808.0),
            Self::F32 => (-32, 0.0),
            Self::F64 => (-64, 0.0),
        }
    }

    /// The name of the type, for messages.
    fn name(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

/// Reads the header of HDU `ext` of the FITS file at `path`, counting the primary HDU as 0.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the file is not FITS, has
/// fewer HDUs, or has a header this reader cannot follow.
pub(crate) fn read_hdu(path: &str, ext: i64) -> Result<Hdu, OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let Ok(wanted) = usize::try_from(ext) else {
        return Err(invalid(format!("there is no HDU {ext}; the first is 0")));
    };
    let mut file = io::BufReader::with_capacity(BUFFER, fs::File::open(path).map_err(failed)?);
    let mut offset = 0_u64;
    for index in 0..=wanted {
        let hdu = header(&mut file, offset, index).map_err(|failure| match failure {
            Failure::Io(error) => failed(error),
            Failure::Invalid(message) => invalid(message),
        })?;
        if index == wanted {
            return Ok(hdu);
        }
        offset = hdu.offset + padded(hdu.size);
        file.seek(SeekFrom::Start(offset)).map_err(failed)?;
    }
    Err(invalid(format!("there is no HDU {ext}")))
}

/// Why reading a header failed: the operating system, or the bytes.
enum Failure {
    /// A read failed.
    Io(io::Error),
    /// The bytes are not a FITS header this reader follows; the message says why.
    Invalid(String),
}

/// Reads the header of HDU number `index` that starts `offset` bytes into `file`, where `file`
/// is positioned.
fn header(file: &mut impl Read, offset: u64, index: usize) -> Result<Hdu, Failure> {
    let mut cards = Vec::new();
    let mut block = vec![0_u8; BLOCK];
    let mut blocks = 0_u64;
    'blocks: loop {
        let mut filled = 0;
        while filled < BLOCK {
            match file.read(&mut block[filled..]) {
                Ok(0) => break,
                Ok(count) => filled += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(Failure::Io(error)),
            }
        }
        if filled < BLOCK {
            return Err(Failure::Invalid(
                if index > 0 && blocks == 0 && filled == 0 {
                    format!("there is no HDU {index}; the file has {index}")
                } else {
                    "the file ends inside a FITS header".to_owned()
                },
            ));
        }
        blocks += 1;
        for card in block.chunks_exact(CARD) {
            let Ok(card) = std::str::from_utf8(card) else {
                return Err(Failure::Invalid(
                    "a FITS header card is not ASCII text".to_owned(),
                ));
            };
            if cards.is_empty() && blocks == 1 {
                let first = if index == 0 { "SIMPLE  =" } else { "XTENSION=" };
                if !card.starts_with(first) {
                    return Err(Failure::Invalid(if index == 0 {
                        "the file is not a FITS file: it does not start with SIMPLE".to_owned()
                    } else {
                        format!("HDU {index} does not start with XTENSION")
                    }));
                }
            }
            if card.trim_end() == "END" {
                break 'blocks;
            }
            cards.push(card.to_owned());
        }
    }
    let hdu_offset = offset + blocks * BLOCK as u64;
    describe(cards, hdu_offset).map_err(Failure::Invalid)
}

/// What `cards` say about the data that starts at `offset`. The error is the message of the
/// `INVALID_DATA` failure.
fn describe(cards: Vec<String>, offset: u64) -> Result<Hdu, String> {
    let integer = |keyword: &str| -> Result<Option<i64>, String> {
        value(&cards, keyword)
            .map(|text| {
                text.parse::<i64>()
                    .map_err(|_| format!("the FITS keyword {keyword} is not an integer: {text}"))
            })
            .transpose()
    };
    let real = |keyword: &str, default: f64| -> Result<f64, String> {
        value(&cards, keyword).map_or(Ok(default), |text| {
            text.replace(['D', 'd'], "E")
                .parse::<f64>()
                .map_err(|_| format!("the FITS keyword {keyword} is not a number: {text}"))
        })
    };
    let bitpix = integer("BITPIX")?.ok_or("the FITS header has no BITPIX")?;
    if ![8, 16, 32, 64, -32, -64].contains(&bitpix) {
        return Err(format!(
            "the FITS header has BITPIX = {bitpix}, which the standard does not allow"
        ));
    }
    let naxis = integer("NAXIS")?.ok_or("the FITS header has no NAXIS")?;
    let naxis = usize::try_from(naxis)
        .ok()
        .filter(|&naxis| naxis <= 999)
        .ok_or_else(|| format!("the FITS header has NAXIS = {naxis}"))?;
    let mut axes = Vec::with_capacity(naxis);
    for axis in 1..=naxis {
        let keyword = format!("NAXIS{axis}");
        let length =
            integer(&keyword)?.ok_or_else(|| format!("the FITS header has no {keyword}"))?;
        axes.push(
            usize::try_from(length)
                .map_err(|_| format!("the FITS header has {keyword} = {length}"))?,
        );
    }
    let pcount = integer("PCOUNT")?.unwrap_or(0);
    let gcount = integer("GCOUNT")?.unwrap_or(1);
    let elements = if axes.is_empty() {
        0
    } else {
        axes.iter().map(|&length| length as u64).product::<u64>()
    };
    let size = (bitpix.unsigned_abs() / 8)
        .saturating_mul(gcount.unsigned_abs())
        .saturating_mul(pcount.unsigned_abs().saturating_add(elements));
    Ok(Hdu {
        bscale: real("BSCALE", 1.0)?,
        bzero: real("BZERO", 0.0)?,
        cards,
        offset,
        bitpix,
        axes,
        size,
    })
}

/// The value text of the card whose keyword is `keyword`, without its comment: `None` when no
/// card has it or the card has no value.
fn value<'a>(cards: &'a [String], keyword: &str) -> Option<&'a str> {
    let card = cards
        .iter()
        .find(|card| card.get(..8).is_some_and(|name| name.trim_end() == keyword))?;
    let text = card.get(8..)?.strip_prefix("= ")?;
    Some(text.split('/').next().unwrap_or("").trim())
}

/// `size` rounded up to a whole number of blocks.
fn padded(size: u64) -> u64 {
    size.div_ceil(BLOCK as u64) * BLOCK as u64
}

/// Reads the image of `hdu`, in the file at `path`, into `memory`, as elements of `pixel`,
/// after checking that the image fits them: the type stored or its unsigned form with `BZERO`
/// for an integer type, any image for a float type, with `BSCALE` and `BZERO` applied as
/// `AstroPy` applies them, in the precision of the type asked for. `memory` holds
/// `elements * pixel.size()` bytes, where `elements` is the image's size.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when the image does not fit
/// `pixel` or the file ends before the image does.
pub(crate) fn read_image(
    path: &str,
    hdu: &Hdu,
    pixel: Pixel,
    memory: &mut [u8],
) -> Result<(), OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let scaled = hdu.scaled();
    let float = matches!(pixel, Pixel::F32 | Pixel::F64);
    if !float
        && (pixel.stored() != (hdu.bitpix, hdu.bzero) || hdu.bscale.to_bits() != 1.0_f64.to_bits())
    {
        let scaling = if scaled {
            format!(" with BSCALE = {} and BZERO = {}", hdu.bscale, hdu.bzero)
        } else {
            String::new()
        };
        return Err(invalid(format!(
            "the FITS image has BITPIX = {}{scaling}, which does not read as `{}`; read it as a \
             float array",
            hdu.bitpix,
            pixel.name()
        )));
    }
    let width = usize::try_from(hdu.bitpix.unsigned_abs() / 8).unwrap_or(8);
    let mut file = fs::File::open(path).map_err(failed)?;
    file.seek(SeekFrom::Start(hdu.offset)).map_err(failed)?;
    let mut reader = io::BufReader::with_capacity(BUFFER, file);
    let mut stored = [0_u8; 8];
    for element in memory.chunks_exact_mut(pixel.size()) {
        reader.read_exact(&mut stored[..width]).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                invalid("the file ends inside the FITS image".to_owned())
            } else {
                failed(error)
            }
        })?;
        convert(&stored[..width], hdu, pixel, element);
    }
    Ok(())
}

/// Writes into `element` the value of one stored big-endian number `stored`, of `hdu`'s
/// `BITPIX`, as an element of `pixel`, which `read_image` has checked fits it.
fn convert(stored: &[u8], hdu: &Hdu, pixel: Pixel, element: &mut [u8]) {
    let mut bytes = [0_u8; 8];
    bytes[..stored.len()].copy_from_slice(stored);
    // The stored number as the widest type of its kind; an integer as its bits, so the
    // unsigned forms come out of the same bits with the top bit flipped, as `BZERO` shifts them.
    let (integer, float) = match hdu.bitpix {
        8 => (i64::from(bytes[0]), f64::from(bytes[0])),
        16 => {
            let value = i16::from_be_bytes([bytes[0], bytes[1]]);
            (i64::from(value), f64::from(value))
        }
        32 => {
            let value = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            (i64::from(value), f64::from(value))
        }
        64 => {
            let value = i64::from_be_bytes(bytes);
            #[allow(clippy::cast_precision_loss, reason = "AstroPy converts the same way")]
            (value, value as f64)
        }
        -32 => (
            0,
            f64::from(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        ),
        _ => (0, f64::from_be_bytes(bytes)),
    };
    let scaled = hdu.scaled();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "each integer keeps its bits, as the checked BITPIX and BZERO promise"
    )]
    match pixel {
        Pixel::U8 => element.copy_from_slice(&[integer as u8]),
        Pixel::I8 => element.copy_from_slice(&((integer as u8) ^ 0x80).to_ne_bytes()),
        Pixel::I16 => element.copy_from_slice(&(integer as i16).to_ne_bytes()),
        Pixel::U16 => element.copy_from_slice(&((integer as u16) ^ 0x8000).to_ne_bytes()),
        Pixel::I32 => element.copy_from_slice(&(integer as i32).to_ne_bytes()),
        Pixel::U32 => element.copy_from_slice(&((integer as u32) ^ 0x8000_0000).to_ne_bytes()),
        Pixel::I64 => element.copy_from_slice(&integer.to_ne_bytes()),
        Pixel::U64 => element.copy_from_slice(&((integer as u64) ^ (1 << 63)).to_ne_bytes()),
        Pixel::F32 => {
            // `AstroPy` scales in the precision of the result: `stored * bscale + bzero` as `f32`s.
            let value = float as f32;
            let value = if scaled {
                value * hdu.bscale as f32 + hdu.bzero as f32
            } else {
                value
            };
            element.copy_from_slice(&value.to_ne_bytes());
        }
        Pixel::F64 => {
            let value = if scaled {
                float * hdu.bscale + hdu.bzero
            } else {
                float
            };
            element.copy_from_slice(&value.to_ne_bytes());
        }
    }
}

mod writing;

pub(crate) use writing::write;
#[cfg(test)]
use writing::{mandatory, push_card};

#[cfg(test)]
mod tests;
