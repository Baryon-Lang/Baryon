//! The functions compiled code calls for FITS files (decision 0044): the standard library's
//! `fits.getheader`, `fits.getdata` and `fits.writeto` call them, and parse and build the
//! header cards in Baryon.
//!
//! Each raises as the array file functions do (the `array_files` module): the last parameter is
//! the address of the caller's `Raises[None, OSError]`, and the result is `true` when the call
//! failed and wrote its error there. An HDU is numbered as `AstroPy`'s `ext` numbers it, the
//! primary HDU 0. An element type is one of the `ELEMENT_*` numbers the array printer uses, a
//! number type other than a complex one.
//!
//! An `Array[T, rank]` is its header, as compiled code lays it out: a pointer to the elements,
//! then `rank` lengths, then `rank` strides in elements, each 64 bits. FITS names the axes
//! fastest first, `NAXIS1` the length of a row, so an array's shape is the axes reversed, as
//! `AstroPy`'s is.

use super::array_files::{runs, write_header};
use super::files::{RaisedError, finish};
use super::print_array::{
    ELEMENT_F32, ELEMENT_F64, ELEMENT_I8, ELEMENT_I16, ELEMENT_I32, ELEMENT_I64, ELEMENT_U8,
    ELEMENT_U16, ELEMENT_U32, ELEMENT_U64,
};
use super::string_lists::{OwnedList, TextValue, write_list};
use super::strings::text;
use crate::files::fits::{self, Pixel};
use crate::files::{ErrorKind, OsFailure};
use crate::memory;
use crate::panic::{Failure, panic};
use std::io::{self, Write};

/// `fits.getheader(path, ext)`: writes to `out` a new `list[str]` of the cards of HDU `ext` in
/// the FITS file whose path is the `length` bytes at `path`, each 80 characters, `END` and the
/// blank cards after it left out. Returns `true`, with the error at `error`, when the operating
/// system refuses, or with the kind `INVALID_DATA` when the file is not FITS or has no HDU
/// `ext`.
///
/// Cost: the headers of the HDUs up to `ext`; the data before it is skipped, not read.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, `out` to a writable [`OwnedList`] and
/// `error` to a writable [`RaisedError`].
///
/// # Example
///
/// See [`baryon_rt_fits_write`], whose file this reads.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_fits_cards(
    path: *const u8,
    length: usize,
    ext: i64,
    out: *mut OwnedList,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let result = fits::read_hdu(path, ext);
    let write = |hdu: fits::Hdu| {
        let cards: Vec<&str> = hdu.cards.iter().map(String::as_str).collect();
        // SAFETY: the caller guarantees `out` is writable.
        unsafe { write_list(&cards, out) };
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// `fits.getdata(path, ext)`: reads the image of HDU `ext` in the FITS file whose path is the
/// `length` bytes at `path` into a new row-major array of `rank` dimensions and elements of the
/// type numbered `element`, and writes its header to `out`. An integer type reads the type the
/// file stores, or its other signedness shifted by `BZERO` as `AstroPy` writes it; a float type
/// reads any image, with `BSCALE` and `BZERO` applied in its precision. Returns `true`, with the
/// error at `error`, when the operating system refuses, or with the kind `INVALID_DATA` when
/// the file is not FITS, the HDU holds no image or one of another rank, the image does not
/// read as the type, or the file ends inside it.
///
/// Cost: the headers up to the HDU, one allocation, and one pass over the pixels, converted
/// from big-endian as they are read.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type FITS holds,
/// which the checker never passes, or when the image does not fit in memory.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8, `out` to `1 + 2 * rank` writable 64-bit
/// words and `error` to a writable [`RaisedError`].
///
/// # Example
///
/// See [`baryon_rt_fits_write`], whose file this reads.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_fits_read(
    path: *const u8,
    length: usize,
    ext: i64,
    element: u32,
    rank: usize,
    out: *mut i64,
    error: *mut RaisedError,
) -> bool {
    let pixel = pixel(element);
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let result = fits::read_hdu(path, ext).and_then(|hdu| read(path, ext, &hdu, pixel, rank));
    // SAFETY: the caller guarantees `1 + 2 * rank` writable words at `out`, and `read` gives a
    // shape `rank` long.
    let write = |(data, shape): (*mut u8, Vec<usize>)| unsafe { write_header(out, data, &shape) };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, write, error) }
}

/// Reads the image of `hdu`, HDU `ext` of the file at `path`, into new memory as `pixel`s:
/// the memory and the shape, the axes reversed. The memory is freed again when the read fails.
#[allow(unsafe_code, reason = "reads into memory it allocates")]
fn read(
    path: &str,
    ext: i64,
    hdu: &fits::Hdu,
    pixel: Pixel,
    rank: usize,
) -> Result<(*mut u8, Vec<usize>), OsFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    if hdu.axes.is_empty() {
        return Err(invalid(format!(
            "HDU {ext} of the FITS file holds no image"
        )));
    }
    if hdu.axes.len() != rank {
        return Err(invalid(format!(
            "HDU {ext} of the FITS file holds an image of rank {}, not rank {rank} as the \
             program asks",
            hdu.axes.len()
        )));
    }
    let count = hdu
        .axes
        .iter()
        .try_fold(1_i64, |count, &length| {
            count.checked_mul(i64::try_from(length).ok()?)
        })
        .unwrap_or(i64::MAX);
    let size = pixel.size();
    let (pointer, layout) = match memory::array_layout(count, size, size)
        .and_then(|layout| Ok((memory::allocate_zeroed(layout)?, layout)))
    {
        Ok(memory) => memory,
        Err(failure) => panic(failure, None),
    };
    // SAFETY: the allocation holds `layout.size()` zeroed bytes that nothing else refers to; an
    // empty one is an aligned pointer that is never read through.
    let memory = unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), layout.size()) };
    if let Err(failure) = fits::read_image(path, hdu, pixel, memory) {
        // SAFETY: `pointer` came from `allocate_zeroed` with this layout just above.
        unsafe { memory::deallocate(pointer, layout) };
        return Err(failure);
    }
    let shape = hdu.axes.iter().rev().copied().collect();
    Ok((pointer.as_ptr(), shape))
}

/// `fits.writeto(path, data, header, overwrite)`: writes the array whose header is at `array`,
/// of `rank` dimensions and elements of the type numbered `element`, to the file whose path is
/// the `length` bytes at `path`, as a FITS file of one image as `AstroPy` writes it: the
/// mandatory cards, with the `card_count` cards at `cards` that are not mandatory when `header`
/// is true (the program gave a header) or `EXTEND` when it is false, `END`, then every element
/// in row-major order, big-endian, with the unsigned types and `i8` shifted by `BZERO`.
/// Makes the file, or replaces the one there when `overwrite` is true. Returns `true`, with the
/// error at `error`, when the operating system refuses, with the kind `FILE_EXISTS` when the
/// file exists and `overwrite` is false, or with `INVALID_DATA` when a card is longer than 80
/// characters or not ASCII.
///
/// Cost: one pass over the elements, converted to big-endian as they are written.
///
/// # Panics
///
/// Ends the program with a panic for an element number that is not a number type FITS holds,
/// which the checker never passes.
///
/// # Safety
///
/// `path` points to `length` readable bytes of UTF-8; `array` to a readable array header of
/// `rank` dimensions, whose every element its lengths and strides reach is readable and
/// unchanged during the call; `cards` to `card_count` readable [`TextValue`]s of UTF-8; and
/// `error` to a writable [`RaisedError`].
///
/// # Example
///
/// ```
/// use runtime::{
///     ELEMENT_I16, OwnedList, TextValue, baryon_rt_deallocate, baryon_rt_fits_cards,
///     baryon_rt_fits_read, baryon_rt_fits_write,
/// };
/// use std::mem::MaybeUninit;
///
/// let path = std::env::temp_dir().join(format!("baryon-doc-fits-{}.fits", std::process::id()));
/// let path = path.to_str().unwrap();
/// let mut pixels = [1_i16, -2, 3, -4, 5, -6];
/// // A 2 by 3 image: the pointer, the lengths, then the strides.
/// let image = [pixels.as_mut_ptr() as i64, 2, 3, 3, 1];
/// let card = "OBJECT  = 'M31     '";
/// let cards = [TextValue { bytes: card.as_ptr(), word: card.len() as u64 }];
/// let mut loaded = [0_i64; 5];
/// let mut header = MaybeUninit::<OwnedList>::uninit();
/// let mut error = MaybeUninit::uninit();
/// // SAFETY: the path and the card are the strings' bytes, `image` describes `pixels`, and
/// // `loaded`, `header` and `error` are locals. The read pixels are freed once.
/// unsafe {
///     let failed = baryon_rt_fits_write(
///         path.as_ptr(), path.len(), image.as_ptr(), 2, ELEMENT_I16, cards.as_ptr(), 1, true,
///         true, error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     let failed = baryon_rt_fits_read(
///         path.as_ptr(), path.len(), 0, ELEMENT_I16, 2, loaded.as_mut_ptr(), error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     assert_eq!(loaded[1..], [2, 3, 3, 1]);
///     let data = loaded[0] as *mut u8;
///     assert_eq!(data.cast::<i16>().add(5).read(), -6);
///     baryon_rt_deallocate(data, 12, 2);
///     let failed = baryon_rt_fits_cards(
///         path.as_ptr(), path.len(), 0, header.as_mut_ptr(), error.as_mut_ptr(),
///     );
///     assert!(!failed);
///     // SIMPLE, BITPIX, NAXIS, NAXIS1, NAXIS2 and OBJECT.
///     assert_eq!(header.assume_init().length, 6);
/// }
/// std::fs::remove_file(path).unwrap();
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
#[allow(
    clippy::too_many_arguments,
    reason = "compiled code passes each part of the call as its own argument"
)]
pub unsafe extern "C" fn baryon_rt_fits_write(
    path: *const u8,
    length: usize,
    array: *const i64,
    rank: usize,
    element: u32,
    cards: *const TextValue,
    card_count: usize,
    header: bool,
    overwrite: bool,
    error: *mut RaisedError,
) -> bool {
    let pixel = pixel(element);
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `path`.
    let path = unsafe { text(path, length) };
    let cards: &[TextValue] = if card_count == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `card_count` readable values at `cards`.
        unsafe { std::slice::from_raw_parts(cards, card_count) }
    };
    let cards: Vec<String> = cards
        .iter()
        .map(|card| {
            let length = usize::try_from(card.word & !(1 << 63)).unwrap_or(0);
            // SAFETY: the caller guarantees each card points to `length` readable bytes of
            // UTF-8.
            unsafe { text(card.bytes, length) }.to_owned()
        })
        .collect();
    // SAFETY: the caller guarantees a header at `array`, whose first word is the pointer.
    let data = unsafe { array.cast::<*const u8>().read() };
    // SAFETY: the header's `rank` lengths follow the pointer.
    let lengths = unsafe { std::slice::from_raw_parts(array.wrapping_add(1), rank) };
    // SAFETY: and its `rank` strides follow the lengths.
    let strides = unsafe { std::slice::from_raw_parts(array.wrapping_add(1 + rank), rank) };
    let axes: Vec<usize> = lengths
        .iter()
        .rev()
        .map(|&length| usize::try_from(length).unwrap_or(0))
        .collect();
    let result = fits::write(
        path,
        pixel,
        &axes,
        header.then_some(&cards[..]),
        overwrite,
        |writer| {
            let mut stored = Stored {
                writer,
                pixel,
                written: 0,
            };
            // SAFETY: the caller's promise about the elements, passed on.
            unsafe { runs(&mut stored, data, lengths, strides, pixel.size()) }?;
            Ok(stored.written)
        },
    );
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(result, |()| {}, error) }
}

/// The pixel type of the element type numbered `element`.
///
/// # Panics
///
/// Ends the program with a panic for a number that is not a number type FITS holds, which the
/// checker never passes.
fn pixel(element: u32) -> Pixel {
    match element {
        ELEMENT_U8 => Pixel::U8,
        ELEMENT_I8 => Pixel::I8,
        ELEMENT_I16 => Pixel::I16,
        ELEMENT_U16 => Pixel::U16,
        ELEMENT_I32 => Pixel::I32,
        ELEMENT_U32 => Pixel::U32,
        ELEMENT_I64 => Pixel::I64,
        ELEMENT_U64 => Pixel::U64,
        ELEMENT_F32 => Pixel::F32,
        ELEMENT_F64 => Pixel::F64,
        _ => panic(Failure::Message("FITS element type out of range"), None),
    }
}

/// A writer that stores elements as a FITS image holds them: each element of `pixel` it is
/// given, in the machine's byte order, goes to `writer` big-endian, its top bit flipped where
/// `BZERO` shifts the type. It is given whole elements, as [`runs`] writes them.
struct Stored<'a> {
    /// Where the stored bytes go.
    writer: &'a mut dyn Write,
    /// The type of the elements.
    pixel: Pixel,
    /// How many bytes have gone to `writer`.
    written: u64,
}

impl Write for Stored<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let size = self.pixel.size();
        let shifted = self.pixel.stored().1 != 0.0;
        let whole = bytes.len() - bytes.len() % size;
        let mut stored = Vec::with_capacity(whole);
        for element in bytes[..whole].chunks_exact(size) {
            store(element, shifted, &mut stored);
        }
        self.writer.write_all(&stored)?;
        self.written += whole as u64;
        Ok(whole)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// Appends to `stored` the big-endian bytes of the element `element`, in the machine's byte
/// order and 1, 2, 4 or 8 bytes long, with its top bit flipped when `shifted`: an unsigned
/// number less 2 to the power of its width less one, or an `i8` plus 128, as `BZERO` stores
/// them.
fn store(element: &[u8], shifted: bool, stored: &mut Vec<u8>) {
    let mut bytes = [0_u8; 8];
    bytes[..element.len()].copy_from_slice(element);
    match element.len() {
        1 => stored.push(bytes[0] ^ (0x80 * u8::from(shifted))),
        2 => {
            let value = u16::from_ne_bytes([bytes[0], bytes[1]]) ^ (0x8000 * u16::from(shifted));
            stored.extend_from_slice(&value.to_be_bytes());
        }
        4 => {
            let value = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                ^ (0x8000_0000 * u32::from(shifted));
            stored.extend_from_slice(&value.to_be_bytes());
        }
        _ => {
            let value = u64::from_ne_bytes(bytes) ^ (u64::from(shifted) << 63);
            stored.extend_from_slice(&value.to_be_bytes());
        }
    }
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests;
