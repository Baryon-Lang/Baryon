//! Tests of the exported FITS functions, called as compiled code calls them. The bytes written
//! are compared with what `AstroPy` 8.0.2's `writeto` writes for the same arrays.

use super::*;
use crate::baryon_rt_deallocate;
use crate::files::fits::BLOCK;
use std::mem::MaybeUninit;

/// A path for one test's file, named after it and this process.
fn file(test: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "baryon-abi-fits-{}-{test}.fits",
        std::process::id()
    ));
    path.to_str().unwrap().to_owned()
}

/// Writes the array whose header words are `words`, of `rank` dimensions and elements of the
/// type numbered `element`, with the header `cards` when there is one, to `path`: the error
/// raised, if any.
fn write(
    path: &str,
    words: &[i64],
    rank: usize,
    element: u32,
    cards: Option<&[&str]>,
    overwrite: bool,
) -> Result<(), RaisedError> {
    let header = cards.is_some();
    let cards = cards.unwrap_or_default();
    let cards: Vec<TextValue> = cards
        .iter()
        .map(|card| TextValue {
            bytes: card.as_ptr(),
            word: card.len() as u64,
        })
        .collect();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path and the cards are the strings' bytes, `words` describes an array the
    // caller owns, and `error` is a local.
    let failed = unsafe {
        baryon_rt_fits_write(
            path.as_ptr(),
            path.len(),
            words.as_ptr(),
            rank,
            element,
            cards.as_ptr(),
            cards.len(),
            header,
            overwrite,
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failed call wrote the error.
        Err(unsafe { error.assume_init() })
    } else {
        Ok(())
    }
}

/// Reads HDU `ext` of `path` as `rank` dimensions of the type numbered `element`, `size` bytes
/// each: the shape and the elements' bytes, which are then freed, or the error raised.
fn read(
    path: &str,
    ext: i64,
    element: u32,
    size: usize,
    rank: usize,
) -> Result<(Vec<i64>, Vec<u8>), RaisedError> {
    let mut words = vec![0_i64; 1 + 2 * rank];
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes, `words` holds `1 + 2 * rank` words and `error`
    // is a local.
    let failed = unsafe {
        baryon_rt_fits_read(
            path.as_ptr(),
            path.len(),
            ext,
            element,
            rank,
            words.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failed call wrote the error.
        return Err(unsafe { error.assume_init() });
    }
    let count: i64 = words[1..=rank].iter().product();
    let bytes = usize::try_from(count).unwrap() * size;
    let data = std::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(words[0]).unwrap());
    // SAFETY: the runtime allocated `bytes` bytes; they are copied, then freed once.
    let elements = unsafe { std::slice::from_raw_parts(data, bytes) }.to_vec();
    // SAFETY: as above.
    unsafe { baryon_rt_deallocate(data, bytes, size) };
    Ok((words[1..=rank].to_vec(), elements))
}

/// Frees the `str`s an `OSError` the runtime raised owns, and gives its kind.
fn kind(error: &RaisedError) -> u8 {
    for text in [&error.error.strerror, &error.error.filename] {
        // SAFETY: each is an owned `str` the runtime allocated, freed once.
        unsafe { baryon_rt_deallocate(text.bytes, usize::try_from(text.length).unwrap(), 1) };
    }
    error.error.kind
}

/// The header words of the row-major array `values`, of the shape `shape`.
fn array<T>(values: &mut [T], shape: &[i64]) -> Vec<i64> {
    let mut words = vec![i64::try_from(values.as_mut_ptr().expose_provenance()).unwrap()];
    words.extend(shape);
    let mut stride = 1;
    let mut strides: Vec<i64> = shape
        .iter()
        .rev()
        .map(|&length| {
            let this = stride;
            stride *= length;
            this
        })
        .collect();
    strides.reverse();
    words.extend(strides);
    words
}

#[test]
fn unsigned_pixels_are_written_shifted_as_astropy_writes_them() {
    let path = file("unsigned");
    let mut values = [0_u16, 1, 65535, 2, 3, 40000];
    let words = array(&mut values, &[2, 3]);
    write(&path, &words, 2, ELEMENT_U16, None, true).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 2 * BLOCK);
    let header = String::from_utf8(bytes[..BLOCK].to_vec()).unwrap();
    let cards: Vec<&str> = (0..10)
        .map(|at| header[at * 80..(at + 1) * 80].trim_end())
        .collect();
    assert_eq!(
        cards,
        [
            "SIMPLE  =                    T / conforms to FITS standard",
            "BITPIX  =                   16 / array data type",
            "NAXIS   =                    2 / number of array dimensions",
            "NAXIS1  =                    3",
            "NAXIS2  =                    2",
            "EXTEND  =                    T",
            "BSCALE  =                    1",
            "BZERO   =                32768",
            "END",
            "",
        ]
    );
    // `AstroPy`'s bytes: each value less 32768, as a big-endian `i16`.
    assert_eq!(
        bytes[BLOCK..BLOCK + 12],
        [0x80, 0, 0x80, 1, 0x7f, 0xff, 0x80, 2, 0x80, 3, 0x1c, 0x40]
    );
    assert!(bytes[BLOCK + 12..].iter().all(|&byte| byte == 0));
    let (shape, elements) = read(&path, 0, ELEMENT_U16, 2, 2).unwrap();
    assert_eq!(shape, [2, 3]);
    let expected: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    assert_eq!(elements, expected);
    // Read as floats, with BZERO applied.
    let (_, elements) = read(&path, 0, ELEMENT_F64, 8, 2).unwrap();
    let floats: Vec<f64> = elements
        .chunks_exact(8)
        .map(|bytes| f64::from_ne_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(floats, [0.0, 1.0, 65535.0, 2.0, 3.0, 40000.0]);
    // With a header, `AstroPy` writes no EXTEND, and the program's cards before the scaling.
    let given = ["EXTEND  =                    T", "OBJECT  = 'M31     '"];
    write(&path, &words, 2, ELEMENT_U16, Some(&given), true).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let header = String::from_utf8(bytes[5 * 80..9 * 80].to_vec()).unwrap();
    let cards: Vec<&str> = (0..4)
        .map(|at| header[at * 80..(at + 1) * 80].trim_end())
        .collect();
    assert_eq!(
        cards,
        [
            "OBJECT  = 'M31     '",
            "BSCALE  =                    1",
            "BZERO   =                32768",
            "END",
        ]
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn signed_bytes_and_floats_read_back_unchanged() {
    let path = file("round-trip");
    let mut bytes = [-128_i8, -1, 0, 127];
    write(&path, &array(&mut bytes, &[4]), 1, ELEMENT_I8, None, true).unwrap();
    let (shape, elements) = read(&path, 0, ELEMENT_I8, 1, 1).unwrap();
    assert_eq!(shape, [4]);
    assert_eq!(elements, [0x80, 0xff, 0, 0x7f]);
    let mut floats = [1.5_f32, -2.25, 3.0];
    write(&path, &array(&mut floats, &[3]), 1, ELEMENT_F32, None, true).unwrap();
    let (_, elements) = read(&path, 0, ELEMENT_F32, 4, 1).unwrap();
    let expected: Vec<u8> = floats
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    assert_eq!(elements, expected);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn an_existing_file_another_rank_or_type_raises() {
    let path = file("raises");
    let mut values = [1_i32, 2, 3, 4];
    let words = array(&mut values, &[2, 2]);
    write(&path, &words, 2, ELEMENT_I32, None, true).unwrap();
    let error = write(&path, &words, 2, ELEMENT_I32, None, false).unwrap_err();
    assert_eq!(kind(&error), ErrorKind::FileExists as u8);
    for (element, size, rank) in [
        (ELEMENT_I32, 4, 1),
        (ELEMENT_I16, 2, 2),
        (ELEMENT_U32, 4, 2),
    ] {
        let error = read(&path, 0, element, size, rank).unwrap_err();
        assert_eq!(
            kind(&error),
            ErrorKind::InvalidData as u8,
            "{element} {rank}"
        );
    }
    let error = read(&path, 1, ELEMENT_I32, 4, 2).unwrap_err();
    assert_eq!(kind(&error), ErrorKind::InvalidData as u8);
    let card = "COMMENT ".to_owned() + &"x".repeat(80);
    let error = write(&path, &words, 2, ELEMENT_I32, Some(&[&card]), true).unwrap_err();
    assert_eq!(kind(&error), ErrorKind::InvalidData as u8);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn the_cards_are_the_headers_before_end() {
    let path = file("cards");
    let mut values = [1.0_f64];
    let words = array(&mut values, &[1]);
    write(
        &path,
        &words,
        1,
        ELEMENT_F64,
        Some(&["EXPTIME =                 30.0"]),
        true,
    )
    .unwrap();
    let mut list = MaybeUninit::<OwnedList>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes; `list` and `error` are locals.
    let failed = unsafe {
        baryon_rt_fits_cards(
            path.as_ptr(),
            path.len(),
            0,
            list.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    // SAFETY: a call that did not fail wrote the list.
    let list = unsafe { list.assume_init() };
    assert_eq!(list.length, 5);
    let card = list.elements.wrapping_add(4);
    // SAFETY: the list holds five owned `str`s; the last is read.
    let card = unsafe { &*card };
    // SAFETY: an owned `str` is `length` bytes at `bytes`.
    let text = unsafe { std::slice::from_raw_parts(card.bytes, 80) };
    assert!(text.starts_with(b"EXPTIME =                 30.0"));
    std::fs::remove_file(path).unwrap();
}
