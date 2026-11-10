//! The functions compiled code calls for the operations on `str` that need a loop: comparing two
//! strings, finding one inside another, and counting characters (roadmap 1.13.2).
//!
//! A `str` reaches each as the address of its UTF-8 bytes and their count, with the static flag
//! of decision 0022 already masked off. Every function only reads.

/// Compares the text `a` with the text `b` as Python compares two `str`s: character by
/// character by code point, and a string that is a prefix of the other first. UTF-8 keeps code
/// point order in byte order, so comparing the bytes gives the same answer. Returns -1 when `a`
/// comes first, 0 when they are equal and 1 when `b` comes first.
///
/// # Safety
///
/// `a` points to `a_length` readable bytes and `b` to `b_length`, or the length is 0.
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_str_compare;
///
/// let (a, b) = ("argon", "argument");
/// // SAFETY: the pointers and lengths are the constants'.
/// let order = unsafe { baryon_rt_str_compare(a.as_ptr(), a.len(), b.as_ptr(), b.len()) };
/// assert_eq!(order, -1);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_compare(
    a: *const u8,
    a_length: usize,
    b: *const u8,
    b_length: usize,
) -> i32 {
    // SAFETY: the caller guarantees `a_length` readable bytes at `a`.
    let a = unsafe { bytes(a, a_length) };
    // SAFETY: the caller guarantees `b_length` readable bytes at `b`.
    let b = unsafe { bytes(b, b_length) };
    match a.cmp(b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Whether the text `needle` occurs in the text `haystack`, as Python's `needle in haystack`
/// on two `str`s. The empty string occurs in every string. The search takes time linear in the
/// two lengths.
///
/// # Safety
///
/// `haystack` points to `haystack_length` readable bytes of UTF-8 and `needle` to
/// `needle_length`, or the length is 0. A `str` value always holds UTF-8.
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_str_contains;
///
/// let (haystack, needle) = ("lennard-jones", "jones");
/// // SAFETY: the pointers and lengths are the constants'.
/// let found = unsafe {
///     baryon_rt_str_contains(haystack.as_ptr(), haystack.len(), needle.as_ptr(), needle.len())
/// };
/// assert!(found);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_contains(
    haystack: *const u8,
    haystack_length: usize,
    needle: *const u8,
    needle_length: usize,
) -> bool {
    // SAFETY: the caller guarantees `haystack_length` readable bytes of UTF-8 at `haystack`.
    let haystack = unsafe { text(haystack, haystack_length) };
    // SAFETY: the caller guarantees `needle_length` readable bytes of UTF-8 at `needle`.
    let needle = unsafe { text(needle, needle_length) };
    haystack.contains(needle)
}

/// The number of characters in the text at `text`, as Python's `len` of a `str` counts them:
/// code points, not bytes, so `len("é")` is 1. Counts every byte that does not continue a
/// character, so it takes time linear in the length.
///
/// # Safety
///
/// `text` points to `length` readable bytes, or `length` is 0.
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_str_length;
///
/// let name = "Ångström";
/// // SAFETY: the pointer and length are the constant's.
/// assert_eq!(unsafe { baryon_rt_str_length(name.as_ptr(), name.len()) }, 8);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_length(text: *const u8, length: usize) -> i64 {
    // SAFETY: the caller guarantees the bytes are readable.
    let bytes = unsafe { bytes(text, length) };
    let characters = bytes.iter().filter(|&&byte| byte & 0xC0 != 0x80).count();
    i64::try_from(characters).unwrap_or(i64::MAX)
}

/// The `length` bytes at `pointer` as a slice; empty when `length` is 0, whatever the pointer.
///
/// # Safety
///
/// When `length` is above 0, `pointer` points to `length` readable bytes that nothing writes
/// while the slice lives.
#[allow(unsafe_code, reason = "reads text compiled code points to")]
unsafe fn bytes<'a>(pointer: *const u8, length: usize) -> &'a [u8] {
    if length == 0 {
        return &[];
    }
    // SAFETY: the caller guarantees `length` readable bytes.
    unsafe { std::slice::from_raw_parts(pointer, length) }
}

/// The `length` bytes of UTF-8 at `pointer` as a `str`; empty when `length` is 0.
///
/// # Safety
///
/// As for [`bytes`], and the bytes are UTF-8, which every `str` value holds.
#[allow(unsafe_code, reason = "reads text compiled code points to")]
unsafe fn text<'a>(pointer: *const u8, length: usize) -> &'a str {
    // SAFETY: the caller guarantees `length` readable bytes at `pointer`.
    let bytes = unsafe { bytes(pointer, length) };
    // SAFETY: the caller guarantees the bytes are UTF-8.
    unsafe { std::str::from_utf8_unchecked(bytes) }
}

#[cfg(test)]
mod tests {
    use super::{baryon_rt_str_compare, baryon_rt_str_contains, baryon_rt_str_length};

    /// `baryon_rt_str_compare` of two Rust strings.
    #[allow(unsafe_code, reason = "calls the function as compiled code does")]
    fn compare(a: &str, b: &str) -> i32 {
        // SAFETY: the pointers and lengths are the strings'.
        unsafe { baryon_rt_str_compare(a.as_ptr(), a.len(), b.as_ptr(), b.len()) }
    }

    /// `baryon_rt_str_contains` of two Rust strings.
    #[allow(unsafe_code, reason = "calls the function as compiled code does")]
    fn contains(haystack: &str, needle: &str) -> bool {
        // SAFETY: the pointers and lengths are the strings'.
        unsafe {
            baryon_rt_str_contains(
                haystack.as_ptr(),
                haystack.len(),
                needle.as_ptr(),
                needle.len(),
            )
        }
    }

    /// Strings order as Python orders them: by code point, a prefix first, and the empty
    /// string before every other.
    #[test]
    fn strings_compare_as_python_does() {
        assert_eq!(compare("abc", "abc"), 0);
        assert_eq!(compare("abc", "abd"), -1);
        assert_eq!(compare("abd", "abc"), 1);
        assert_eq!(compare("ab", "abc"), -1);
        assert_eq!(compare("", "a"), -1);
        assert_eq!(compare("", ""), 0);
        assert_eq!(compare("Z", "a"), -1);
        assert_eq!(compare("é", "z"), 1);
        assert_eq!(compare("\u{ffff}", "\u{10000}"), -1);
    }

    /// `in` finds text anywhere, the empty string everywhere, and nothing longer than the
    /// text it searches.
    #[test]
    fn contains_finds_substrings() {
        assert!(contains("temperature", "temp"));
        assert!(contains("temperature", "rat"));
        assert!(contains("temperature", ""));
        assert!(contains("", ""));
        assert!(!contains("", "a"));
        assert!(!contains("temp", "temperature"));
        assert!(contains("Ångström", "ö"));
    }

    /// `len` counts characters, not bytes, and a null pointer with no bytes is empty.
    #[test]
    #[allow(unsafe_code, reason = "calls the function as compiled code does")]
    fn length_counts_characters() {
        let text = "a€😀";
        // SAFETY: the pointer and length are the string's.
        let characters = unsafe { baryon_rt_str_length(text.as_ptr(), text.len()) };
        assert_eq!(characters, 3);
        // SAFETY: no bytes are read when the length is 0.
        let empty = unsafe { baryon_rt_str_length(std::ptr::null(), 0) };
        assert_eq!(empty, 0);
    }
}
