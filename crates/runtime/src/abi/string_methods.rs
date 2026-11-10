//! The functions compiled code calls for `str`'s methods (roadmap 1.13.12): indexing and
//! slicing, `split`, `strip`, `lstrip`, `rstrip`, `join`, `startswith`, `endswith`, `find`,
//! `replace`, `lower` and `upper`.
//!
//! A `str` argument arrives as the address of its UTF-8 bytes and their count, with the static
//! flag of decision 0022 already masked off. A new `str` goes back as an [`OwnedText`] the caller
//! owns and frees; a list of them is written to an [`OwnedList`] the caller passes. Every result
//! is a copy, never a view into an argument, so it outlives the text it came from.
//!
//! ```
//! use runtime::{OwnedList, OwnedText, baryon_rt_deallocate, baryon_rt_str_split_whitespace};
//!
//! let line = " 1.5  2.5\n";
//! let mut words = OwnedList { elements: std::ptr::null_mut(), length: 0, capacity: 0 };
//! // SAFETY: the pointer and length are the constant's; each word and the list's buffer are
//! // freed once, with the sizes they were allocated with.
//! unsafe {
//!     baryon_rt_str_split_whitespace(line.as_ptr(), line.len(), &mut words);
//!     assert_eq!(words.length, 2);
//!     for index in 0..2 {
//!         let word: &OwnedText = &*words.elements.add(index);
//!         assert_eq!(std::slice::from_raw_parts(word.bytes, 3), [b"1.5", b"2.5"][index]);
//!         baryon_rt_deallocate(word.bytes, word.length as usize, 1);
//!     }
//!     baryon_rt_deallocate(words.elements.cast(), 2 * size_of::<OwnedText>(), 8);
//! }
//! ```

use super::strings::bytes;
use super::text::{OwnedText, owned};
use crate::memory;
use crate::panic::panic;
use crate::strings::{self, Side, is_python_whitespace};

/// A `list[str]` header as compiled code holds it: the elements, how many there are, and how
/// many the buffer has room for. A list the runtime makes has room for exactly its elements, in
/// one buffer of `capacity` [`OwnedText`]s aligned to 8, which a drop frees after each element.
#[repr(C)]
#[derive(Debug)]
pub struct OwnedList {
    /// The first element.
    pub elements: *mut OwnedText,
    /// How many elements there are.
    pub length: u64,
    /// How many the buffer has room for.
    pub capacity: u64,
}

/// A `str` value inside a list, as compiled code stores it: the bytes and the length word, whose
/// top bit says the bytes are not owned (decision 0022).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TextValue {
    /// The UTF-8 bytes.
    pub bytes: *const u8,
    /// The byte count in the low 63 bits, and the "not owned" flag in the top bit.
    pub word: u64,
}

/// The `length` bytes of UTF-8 at `pointer` as a `str`; empty when `length` is 0.
///
/// # Safety
///
/// `pointer` points to `length` readable bytes of UTF-8 that nothing writes while the `str`
/// lives, or `length` is 0.
#[allow(unsafe_code, reason = "reads text compiled code points to")]
unsafe fn text<'a>(pointer: *const u8, length: usize) -> &'a str {
    // SAFETY: the caller guarantees `length` readable bytes at `pointer`.
    let bytes = unsafe { bytes(pointer, length) };
    // SAFETY: the caller guarantees the bytes are UTF-8, which every `str` value holds.
    unsafe { std::str::from_utf8_unchecked(bytes) }
}

/// Writes a new list holding a copy of each of `pieces` to `out`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `out` points to a writable [`OwnedList`].
#[allow(unsafe_code, reason = "writes a list compiled code owns")]
unsafe fn write_list(pieces: &[&str], out: *mut OwnedList) {
    let size = size_of::<OwnedText>();
    let count = i64::try_from(pieces.len()).unwrap_or(i64::MAX);
    let elements = match memory::array_layout(count, size, align_of::<OwnedText>())
        .and_then(memory::allocate)
    {
        // The layout's alignment is `OwnedText`'s, so the pointer is aligned for one.
        #[allow(
            clippy::cast_ptr_alignment,
            reason = "allocated with `OwnedText`'s alignment"
        )]
        Ok(pointer) => pointer.as_ptr().cast::<OwnedText>(),
        Err(failure) => panic(failure, None),
    };
    for (index, piece) in pieces.iter().enumerate() {
        let piece = owned(piece);
        // SAFETY: the buffer has room for `pieces.len()` elements.
        let slot = unsafe { elements.add(index) };
        // SAFETY: `slot` is inside the buffer and aligned for an `OwnedText`.
        unsafe { slot.write(piece) };
    }
    let length = u64::try_from(pieces.len()).unwrap_or(u64::MAX);
    // SAFETY: the caller guarantees `out` is writable.
    unsafe {
        out.write(OwnedList {
            elements,
            length,
            capacity: length,
        });
    }
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

/// Writes the pieces of the text at `text` between occurrences of the text at `separator` to
/// `out`, as Python's `text.split(separator)`. The separator is not empty: compiled code panics
/// first, where Python raises `ValueError`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8 and `separator` to `separator_length`, or
/// the length is 0, and `out` points to a writable [`OwnedList`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_split(
    text: *const u8,
    length: usize,
    separator: *const u8,
    separator_length: usize,
    out: *mut OwnedList,
) {
    // SAFETY: the caller guarantees both texts are readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: as for the text.
    let separator = unsafe { self::text(separator, separator_length) };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { write_list(&strings::split(text, separator), out) };
}

/// Writes the words of the text at `text` to `out`, as Python's `text.split()`: runs of
/// whitespace separate them, and none is empty.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `text` points to `length` readable bytes of UTF-8, or `length` is 0, and `out` points to a
/// writable [`OwnedList`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_split_whitespace(
    text: *const u8,
    length: usize,
    out: *mut OwnedList,
) {
    // SAFETY: the caller guarantees the text is readable UTF-8.
    let text = unsafe { self::text(text, length) };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { write_list(&strings::split_whitespace(text), out) };
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

/// The `count` `str` values at `pieces` joined into a new `str` with the text at `separator`
/// between each two, as Python's `separator.join(pieces)`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left.
///
/// # Safety
///
/// `separator` points to `separator_length` readable bytes of UTF-8, or the length is 0;
/// `pieces` points to `count` readable [`TextValue`]s, each pointing to readable UTF-8, or
/// `count` is 0.
///
/// # Example
///
/// ```
/// use runtime::{TextValue, baryon_rt_deallocate, baryon_rt_str_join};
///
/// let (separator, a, b) = (", ", "x", "y");
/// let flag = 1 << 63;
/// let pieces = [
///     TextValue { bytes: a.as_ptr(), word: 1 | flag },
///     TextValue { bytes: b.as_ptr(), word: 1 | flag },
/// ];
/// // SAFETY: the pointers and lengths are the constants'; the result is freed once.
/// unsafe {
///     let joined = baryon_rt_str_join(separator.as_ptr(), separator.len(), pieces.as_ptr(), 2);
///     assert_eq!(std::slice::from_raw_parts(joined.bytes, 4), b"x, y");
///     baryon_rt_deallocate(joined.bytes, joined.length as usize, 1);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_join(
    separator: *const u8,
    separator_length: usize,
    pieces: *const TextValue,
    count: usize,
) -> OwnedText {
    // SAFETY: the caller guarantees the separator is readable UTF-8.
    let separator = unsafe { text(separator, separator_length) };
    let pieces: &[TextValue] = if count == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `count` readable values.
        unsafe { std::slice::from_raw_parts(pieces, count) }
    };
    let mut joined = String::new();
    for (index, piece) in pieces.iter().enumerate() {
        if index > 0 {
            joined.push_str(separator);
        }
        let length = usize::try_from(piece.word & !(1 << 63)).unwrap_or(0);
        // SAFETY: the caller guarantees each piece points to `length` readable bytes of UTF-8.
        joined.push_str(unsafe { text(piece.bytes, length) });
    }
    owned(&joined)
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
        OwnedList, OwnedText, baryon_rt_str_ends_with, baryon_rt_str_lower, baryon_rt_str_replace,
        baryon_rt_str_split, baryon_rt_str_starts_with, baryon_rt_str_strip_characters,
        baryon_rt_str_upper,
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

    /// `text.split(separator)` through the runtime, as Rust strings.
    #[allow(unsafe_code, reason = "calls the function as compiled code does")]
    fn split(text: &str, separator: &str) -> Vec<String> {
        let mut list = OwnedList {
            elements: std::ptr::null_mut(),
            length: 0,
            capacity: 0,
        };
        // SAFETY: the pointers and lengths are the strings'.
        unsafe {
            baryon_rt_str_split(
                text.as_ptr(),
                text.len(),
                separator.as_ptr(),
                separator.len(),
                &raw mut list,
            );
        }
        let count = usize::try_from(list.length).unwrap_or(0);
        assert_eq!(list.capacity, list.length);
        // SAFETY: the runtime wrote `count` elements.
        let elements = unsafe { std::slice::from_raw_parts(list.elements, count) };
        let pieces = elements.iter().map(read).collect();
        // SAFETY: the buffer is freed once, with the size it was allocated with.
        unsafe { baryon_rt_deallocate(list.elements.cast(), count * size_of::<OwnedText>(), 8) };
        pieces
    }

    /// `split` on a separator keeps empty pieces, as Python's does.
    #[test]
    fn split_keeps_empty_pieces() {
        assert_eq!(split("a,,b", ","), ["a", "", "b"]);
        assert_eq!(split("", ","), [""]);
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
