//! Writing a FITS file of one image, as `AstroPy`'s `writeto` writes it: the cards it starts
//! with, a program's own cards, and the padding to whole blocks.

use super::{BUFFER, CARD, ErrorKind, OsFailure, Pixel, padded};
use std::fs;
use std::io::{self, BufWriter, Read, Write};

/// The cards `AstroPy`'s `writeto` writes first for an image of `pixel`s with the axes `axes`,
/// `NAXIS1` first: `SIMPLE`, `BITPIX`, `NAXIS`, each `NAXISn`, `EXTEND`, and `BSCALE` and
/// `BZERO` for a type stored shifted, with `AstroPy`'s comments.
pub(super) fn mandatory(pixel: Pixel, axes: &[usize]) -> Vec<String> {
    let (bitpix, bzero) = pixel.stored();
    let mut cards = vec![
        card("SIMPLE", "T", "conforms to FITS standard"),
        card("BITPIX", &bitpix.to_string(), "array data type"),
        card(
            "NAXIS",
            &axes.len().to_string(),
            "number of array dimensions",
        ),
    ];
    for (axis, length) in axes.iter().enumerate() {
        cards.push(card(&format!("NAXIS{}", axis + 1), &length.to_string(), ""));
    }
    cards.push(card("EXTEND", "T", ""));
    if bzero != 0.0 {
        cards.push(card("BSCALE", "1", ""));
        #[allow(
            clippy::cast_possible_truncation,
            reason = "each shift is a whole number"
        )]
        cards.push(card("BZERO", &(bzero as i128).to_string(), ""));
    }
    cards
}

/// One card with a value in the fixed format: the keyword in columns 1 to 8, `= ` in 9 and 10,
/// the value right-justified to column 30, and ` / comment` after it when there is one, padded
/// with spaces to 80 characters.
fn card(keyword: &str, value: &str, comment: &str) -> String {
    let mut text = format!("{keyword:<8}= {value:>20}");
    if !comment.is_empty() {
        text.push_str(" / ");
        text.push_str(comment);
    }
    format!("{text:<80}")
}

/// Whether `card`'s keyword is one [`mandatory`] writes, which a program's own header cannot
/// replace: `AstroPy` drops them from a header it writes with data too.
pub(super) fn is_mandatory(card: &str) -> bool {
    let keyword = card.get(..8).unwrap_or(card).trim_end();
    matches!(
        keyword,
        "SIMPLE" | "BITPIX" | "NAXIS" | "EXTEND" | "BSCALE" | "BZERO" | "END"
    ) || keyword
        .strip_prefix("NAXIS")
        .is_some_and(|axis| !axis.is_empty() && axis.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Writes a FITS file of one image to `path` as `AstroPy`'s `writeto` does: `SIMPLE`, `BITPIX`,
/// `NAXIS` and each `NAXISn`; then `EXTEND` when the program gave no header, or the program's
/// `cards` without their mandatory ones when it did; then `BSCALE` and `BZERO` for a type stored
/// shifted; `END` and blank cards to a whole block; then the image that `emit` writes in
/// big-endian order, returning its size in bytes; then zeros to a whole block. Makes the file,
/// or replaces the one there when `overwrite` is true.
///
/// # Errors
///
/// Fails as the operating system does, with `FILE_EXISTS` when the file exists and
/// `overwrite` is false, and with `INVALID_DATA` when a card is longer than 80 characters or
/// not ASCII.
pub(crate) fn write(
    path: &str,
    pixel: Pixel,
    axes: &[usize],
    cards: Option<&[String]>,
    overwrite: bool,
    emit: impl FnOnce(&mut dyn Write) -> io::Result<u64>,
) -> Result<(), OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let given = cards.unwrap_or_default();
    if let Some(card) = given
        .iter()
        .find(|card| card.len() > CARD || !card.is_ascii())
    {
        return Err(OsFailure::new(
            ErrorKind::InvalidData,
            format!("a FITS card must be 80 ASCII characters at most: {card}"),
            path,
        ));
    }
    let file = if overwrite {
        fs::File::create(path)
    } else {
        fs::File::create_new(path)
    }
    .map_err(failed)?;
    let mut writer = BufWriter::with_capacity(BUFFER, file);
    let mut header = String::new();
    // `mandatory` gives the cards up to each `NAXISn`, then `EXTEND`, then the scaling cards.
    let mandatory = mandatory(pixel, axes);
    let (leading, rest) = mandatory.split_at(3 + axes.len());
    let (extend, scaling) = rest.split_at(1);
    let middle = match cards {
        None => extend,
        Some(_) => &[],
    };
    let program = given.iter().filter(|card| !is_mandatory(card));
    for card in leading
        .iter()
        .map(String::as_str)
        .chain(middle.iter().map(String::as_str))
    {
        push_card(&mut header, card);
    }
    for card in program.chain(scaling) {
        push_card(&mut header, card);
    }
    push_card(&mut header, "END");
    let header_size = padded(header.len() as u64);
    let header_size = usize::try_from(header_size).unwrap_or(usize::MAX);
    header.push_str(&" ".repeat(header_size - header.len()));
    writer.write_all(header.as_bytes()).map_err(failed)?;
    let written = emit(&mut writer).map_err(failed)?;
    let padding = padded(written) - written;
    io::copy(&mut io::repeat(0).take(padding), &mut writer).map_err(failed)?;
    writer.flush().map_err(failed)
}

/// Appends `card`, at most 80 characters, to `header`, padded with spaces to 80.
pub(super) fn push_card(header: &mut String, card: &str) {
    header.push_str(card);
    header.extend(std::iter::repeat_n(' ', CARD.saturating_sub(card.len())));
}
