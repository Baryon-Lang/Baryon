//! The functions compiled code calls for `str`'s methods that give one value (roadmap 1.13.12):
//! indexing and slicing, `strip`, `lstrip`, `rstrip`, `startswith`, `endswith`, `find`,
//! `replace`, `lower` and `upper`. The ones that make or read a `list[str]` are in
//! `string_lists`.
//!
//! A `str` argument arrives as the address of its UTF-8 bytes and their count, with the static
//! flag of decision 0022 already masked off. A new `str` goes back as an [`OwnedText`] the caller
//! owns and frees: a copy, never a view into an argument, so it outlives the text it came from.

use super::strings::bytes;
use super::text::{OwnedText, owned};
use crate::strings::{self, Side, is_python_whitespace};

/// The `length` bytes of UTF-8 at `pointer` as a `str`; empty when `length` is 0.
///
/// # Safety
///
/// `pointer` points to `length` readable bytes of UTF-8 that nothing writes while the `str`
/// lives, or `length` is 0.
#[allow(unsafe_code, reason = "reads text compiled code points to")]
pub(super) unsafe fn text<'a>(pointer: *const u8, length: usize) -> &'a str {
    // SAFETY: the caller guarantees `length` readable bytes at `pointer`.
    let bytes = unsafe { bytes(pointer, length) };
    // SAFETY: the caller guarantees the bytes are UTF-8, which every `str` value holds.
    unsafe { std::str::from_utf8_unchecked(bytes) }
}

/// The characters of the text at `text` at positions `start`, `start + step` and so on, `count`
/// of them, as a new `str`: `text[i]` with a `count` of 1, and the slice `text[i:j:k]`.
/// Positions count characters, as Python's do. Compiled code checks the range first.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_deallocate, baryon_rt_str_take};
///
/// let name = "Ångström";
/// // SAFETY: the pointer and length are the constant's; the result is freed once.
/// unsafe {
///     let taken = baryon_rt_str_take(name.as_ptr(), name.len(), 0, 4, 2);
///     assert_eq!(std::slice::from_raw_parts(taken.bytes, 6), "Ågtö".as_bytes());
///     baryon_rt_deallocate(taken.bytes, taken.length as usize, 1);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_take(
    text: *const u8,
    length: usize,
    start: i64,
    count: i64,
    step: i64,
) -> OwnedText {
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8.
    let text = unsafe { self::text(text, length) };
    owned(&strings::take(text, start, count, step))
}

/// The text at `text` without the whitespace at the ends `side` names, as a new `str`: 0 for
/// both ends (`strip()`), 1 for the start (`lstrip()`) and 2 for the end (`rstrip()`).
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_deallocate, baryon_rt_str_strip};
///
/// let line = "  42\n";
/// // SAFETY: the pointer and length are the constant's; the result is freed once.
/// unsafe {
///     let stripped = baryon_rt_str_strip(line.as_ptr(), line.len(), 0);
///     assert_eq!(std::slice::from_raw_parts(stripped.bytes, 2), b"42");
///     baryon_rt_deallocate(stripped.bytes, stripped.length as usize, 1);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_strip(
    text: *const u8,
    length: usize,
    side: i32,
) -> OwnedText {
    // SAFETY: the caller guarantees the text is readable UTF-8.
    let text = unsafe { self::text(text, length) };
    owned(strings::strip(
        text,
        Side::from_code(side),
        is_python_whitespace,
    ))
}

/// The text at `text` without any of the characters of the text at `characters` at the ends
/// `side` names, as [`baryon_rt_str_strip`] numbers them: Python's `strip(characters)`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8 and `characters` to `characters_length`,
/// or the length is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_strip_characters(
    text: *const u8,
    length: usize,
    characters: *const u8,
    characters_length: usize,
    side: i32,
) -> OwnedText {
    // SAFETY: the caller guarantees both texts are readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: as for the text.
    let characters = unsafe { self::text(characters, characters_length) };
    owned(strings::strip(text, Side::from_code(side), |character| {
        characters.contains(character)
    }))
}

/// Whether the text at `text` starts with the text at `prefix`, as Python's `startswith`.
///
/// # Safety
///
/// Each pointer points to its length's readable bytes of UTF-8, or the length is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_starts_with(
    text: *const u8,
    length: usize,
    prefix: *const u8,
    prefix_length: usize,
) -> bool {
    // SAFETY: the caller guarantees both are readable.
    let text = unsafe { bytes(text, length) };
    // SAFETY: as for the text.
    text.starts_with(unsafe { bytes(prefix, prefix_length) })
}

/// Whether the text at `text` ends with the text at `suffix`, as Python's `endswith`.
///
/// # Safety
///
/// Each pointer points to its length's readable bytes of UTF-8, or the length is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_ends_with(
    text: *const u8,
    length: usize,
    suffix: *const u8,
    suffix_length: usize,
) -> bool {
    // SAFETY: the caller guarantees both are readable.
    let text = unsafe { bytes(text, length) };
    // SAFETY: as for the text.
    text.ends_with(unsafe { bytes(suffix, suffix_length) })
}

/// The character position of the first occurrence of the text at `needle` in the text at
/// `text`, or -1, as Python's `find`.
///
/// # Safety
///
/// Each pointer points to its length's readable bytes of UTF-8, or the length is 0.
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_str_find;
///
/// let (text, needle) = ("Ångström", "r");
/// // SAFETY: the pointers and lengths are the constants'.
/// let found = unsafe { baryon_rt_str_find(text.as_ptr(), text.len(), needle.as_ptr(), 1) };
/// assert_eq!(found, 5);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_find(
    text: *const u8,
    length: usize,
    needle: *const u8,
    needle_length: usize,
) -> i64 {
    // SAFETY: the caller guarantees both are readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: as for the text.
    let needle = unsafe { self::text(needle, needle_length) };
    strings::find(text, needle)
}

/// The text at `text` with every occurrence of the text at `old` replaced by the text at `new`,
/// as a new `str`, as Python's `replace`. An empty `old` matches between every two characters
/// and at both ends, as in Python.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// Each pointer points to its length's readable bytes of UTF-8, or the length is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_replace(
    text: *const u8,
    length: usize,
    old: *const u8,
    old_length: usize,
    new: *const u8,
    new_length: usize,
) -> OwnedText {
    // SAFETY: the caller guarantees all three are readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: as for the text.
    let old = unsafe { self::text(old, old_length) };
    // SAFETY: as for the text.
    let new = unsafe { self::text(new, new_length) };
    owned(&text.replace(old, new))
}

/// The text at `text` in lower case, as a new `str`, by Unicode's full case mapping, as
/// Python's `lower`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_lower(text: *const u8, length: usize) -> OwnedText {
    // SAFETY: the caller guarantees the text is readable UTF-8.
    owned(&unsafe { self::text(text, length) }.to_lowercase())
}

/// The text at `text` in upper case, as a new `str`, by Unicode's full case mapping, so `"ß"`
/// becomes `"SS"`, as Python's `upper`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_upper(text: *const u8, length: usize) -> OwnedText {
    // SAFETY: the caller guarantees the text is readable UTF-8.
    owned(&unsafe { self::text(text, length) }.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::{
        OwnedText, baryon_rt_str_ends_with, baryon_rt_str_lower, baryon_rt_str_replace,
        baryon_rt_str_starts_with, baryon_rt_str_strip_characters, baryon_rt_str_upper,
    };
    use crate::baryon_rt_deallocate;

    /// The text of an [`OwnedText`] the runtime made, which is then freed.
    #[allow(unsafe_code, reason = "reads and frees text the runtime made")]
    fn read(text: &OwnedText) -> String {
        let length = usize::try_from(text.length).unwrap_or(0);
        // SAFETY: the runtime made `length` bytes of UTF-8 at `bytes`.
        let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) };
        let read = String::from_utf8_lossy(bytes).into_owned();
        // SAFETY: the bytes are freed once, with their length.
        unsafe { baryon_rt_deallocate(text.bytes, length, 1) };
        read
    }

    /// `strip(characters)`, `startswith`, `endswith`, `replace`, `lower` and `upper` give
    /// Python's answers.
    #[test]
    #[allow(unsafe_code, reason = "calls the functions as compiled code does")]
    fn methods_match_python() {
        let (text, x, empty, dash, sharp) = ("xxheliumxx", "x", "", "-", "Straße");
        // SAFETY: every pointer and length is a constant's.
        let stripped =
            unsafe { baryon_rt_str_strip_characters(text.as_ptr(), 10, x.as_ptr(), 1, 1) };
        assert_eq!(read(&stripped), "heliumxx");
        // SAFETY: as above.
        assert!(unsafe { baryon_rt_str_starts_with(text.as_ptr(), 10, x.as_ptr(), 1) });
        // SAFETY: as above.
        assert!(unsafe { baryon_rt_str_ends_with(text.as_ptr(), 10, x.as_ptr(), 1) });
        // SAFETY: as above.
        assert!(!unsafe { baryon_rt_str_ends_with(x.as_ptr(), 1, text.as_ptr(), 10) });
        // SAFETY: as above.
        let replaced =
            unsafe { baryon_rt_str_replace(x.as_ptr(), 1, empty.as_ptr(), 0, dash.as_ptr(), 1) };
        assert_eq!(read(&replaced), "-x-");
        // SAFETY: as above.
        let upper = unsafe { baryon_rt_str_upper(sharp.as_ptr(), sharp.len()) };
        assert_eq!(read(&upper), "STRASSE");
        // SAFETY: as above.
        let lower = unsafe { baryon_rt_str_lower(sharp.as_ptr(), sharp.len()) };
        assert_eq!(read(&lower), "straße");
    }
}
