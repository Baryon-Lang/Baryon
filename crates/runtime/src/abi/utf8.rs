//! The functions compiled code calls for `text.encode()` and `data.decode()` (decision 0064):
//! a `str` turned into an `Array[u8, 1]` of its UTF-8 bytes, and back.
//!
//! A `str` is UTF-8 already (decision 0022), so encoding is one copy. Decoding checks the bytes
//! and follows the convention of a compiled function that raises (decision 0005), as
//! `int(text)` does: its last parameter is the address of the caller's
//! `Raises[None, ValueError]`, where it writes Python's message and returns `true` when the
//! bytes are not UTF-8.

use super::array_files::ArrayHeader;
use super::numbers::{RaisedValueError, ValueError};
use super::strings::text;
use super::text::{OwnedText, owned};

/// `text.encode()`: writes the header of a new rank-1 `Array[u8, 1]` holding a copy of the
/// `length` bytes at `text` to `*out`.
///
/// Cost: one allocation and one copy of the bytes.
///
/// # Panics
///
/// Ends the program with a panic when the copy does not fit in memory, as any allocation does.
///
/// # Safety
///
/// `text` points to `length` readable bytes, or `length` is 0; `out` points to a writable
/// [`ArrayHeader`].
///
/// # Example
///
/// ```
/// use runtime::{ArrayHeader, baryon_rt_deallocate, baryon_rt_str_encode};
///
/// let text = "é1";
/// let mut array = ArrayHeader { data: std::ptr::null_mut(), length: 0, stride: 0 };
/// // SAFETY: the pointer and length are the constant's, and the header is writable.
/// unsafe { baryon_rt_str_encode(text.as_ptr(), text.len(), &mut array) };
/// assert_eq!((array.length, array.stride), (3, 1));
/// // SAFETY: the runtime made three bytes at `data`.
/// let bytes = unsafe { std::slice::from_raw_parts(array.data, 3) };
/// assert_eq!(bytes, [0xc3, 0xa9, b'1']);
/// // SAFETY: freed once, with its length.
/// unsafe { baryon_rt_deallocate(array.data, 3, 1) };
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_str_encode(
    text: *const u8,
    length: usize,
    out: *mut ArrayHeader,
) {
    // SAFETY: the caller guarantees the text is readable.
    let text = unsafe { self::text(text, length) };
    let copy = owned(text);
    let header = ArrayHeader {
        data: copy.bytes,
        length: i64::try_from(copy.length).unwrap_or(i64::MAX),
        stride: 1,
    };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { out.write(header) };
}

/// `data.decode()`: reads the elements of the `Array[u8, 1]` whose header is at `array` and,
/// when they are UTF-8, writes a new `str` of them to `*out` and returns `false`. When they are
/// not, writes a `ValueError` with Python's message to `error` and returns `true`.
///
/// Cost: one pass to gather the bytes, one to check them, and one allocation.
///
/// # Panics
///
/// Ends the program with a panic when the text does not fit in memory.
///
/// # Safety
///
/// `array` points to a readable rank-1 array header whose length and stride reach only
/// readable bytes; `out` points to a writable [`OwnedText`] and `error` to a writable
/// [`RaisedValueError`].
///
/// # Example
///
/// ```
/// use runtime::{ArrayHeader, RaisedValueError, baryon_rt_array_decode, baryon_rt_deallocate};
/// use std::mem::MaybeUninit;
///
/// let mut bytes = [0xc3_u8, 0xa9];
/// let array = ArrayHeader { data: bytes.as_mut_ptr(), length: 2, stride: 1 };
/// let mut text = MaybeUninit::uninit();
/// let mut error = MaybeUninit::<RaisedValueError>::uninit();
/// // SAFETY: the header reaches the two bytes, and both outputs are writable.
/// let failed =
///     unsafe { baryon_rt_array_decode(&array, text.as_mut_ptr(), error.as_mut_ptr()) };
/// assert!(!failed);
/// // SAFETY: the call wrote the text.
/// let text = unsafe { text.assume_init() };
/// // SAFETY: the runtime made `length` bytes at `bytes`.
/// let decoded = unsafe { std::slice::from_raw_parts(text.bytes, 2) };
/// assert_eq!(decoded, "é".as_bytes());
/// // SAFETY: freed once, with its length.
/// unsafe { baryon_rt_deallocate(text.bytes, 2, 1) };
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_array_decode(
    array: *const ArrayHeader,
    out: *mut OwnedText,
    error: *mut RaisedValueError,
) -> bool {
    // SAFETY: the caller guarantees a readable header.
    let header = unsafe { &*array };
    let length = usize::try_from(header.length).unwrap_or(0);
    let bytes: Vec<u8> = if header.stride == 1 {
        // SAFETY: the caller guarantees `length` readable bytes from `data`.
        unsafe { std::slice::from_raw_parts(header.data, length) }.to_vec()
    } else {
        let stride = isize::try_from(header.stride).unwrap_or(1);
        (0..length)
            .map(|index| {
                let offset = isize::try_from(index).unwrap_or(isize::MAX) * stride;
                // SAFETY: the caller guarantees the stride stays inside the array's bytes.
                let element = unsafe { header.data.offset(offset) };
                // SAFETY: the caller guarantees each element the stride reaches is readable.
                unsafe { element.read() }
            })
            .collect()
    };
    match std::str::from_utf8(&bytes) {
        Ok(decoded) => {
            // SAFETY: the caller guarantees `out` is writable.
            unsafe { out.write(owned(decoded)) };
            false
        }
        Err(problem) => {
            let raised = RaisedValueError {
                tag: 0,
                error: ValueError {
                    message: owned(&message(&bytes, &problem)),
                },
            };
            // SAFETY: the caller guarantees `error` is writable.
            unsafe { error.write(raised) };
            true
        }
    }
}

/// Python's message for `bytes`, which are not UTF-8 where `problem` says, such as
/// `'utf-8' codec can't decode byte 0xff in position 0: invalid start byte`. Python names the
/// bytes of the longest start of a character that is cut short or broken, as Rust's
/// [`Utf8Error::error_len`](std::str::Utf8Error::error_len) counts them, and says why: a byte
/// no character starts with, a start followed by a byte that cannot continue it, or the end of
/// the data inside a character.
fn message(bytes: &[u8], problem: &std::str::Utf8Error) -> String {
    let start = problem.valid_up_to();
    let (count, reason) = match problem.error_len() {
        Some(1) if matches!(bytes[start], 0x80..=0xc1 | 0xf5..=0xff) => (1, "invalid start byte"),
        Some(count) => (count, "invalid continuation byte"),
        None => (bytes.len() - start, "unexpected end of data"),
    };
    let which = if count == 1 {
        format!("byte {:#04x} in position {start}", bytes[start])
    } else {
        format!("bytes in position {start}-{}", start + count - 1)
    };
    format!("'utf-8' codec can't decode {which}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::message;

    /// Python's message for `bytes`, which must not be UTF-8.
    fn decoded(bytes: &[u8]) -> String {
        let problem = std::str::from_utf8(bytes).unwrap_err();
        message(bytes, &problem)
    }

    /// Each kind of bad UTF-8 gives the message CPython 3.14 gives for the same bytes.
    #[test]
    fn messages_match_python() {
        let cases: [(&[u8], &str); 8] = [
            (b"\xff", "byte 0xff in position 0: invalid start byte"),
            (b"ab\x80", "byte 0x80 in position 2: invalid start byte"),
            (b"\xc0\x80", "byte 0xc0 in position 0: invalid start byte"),
            (
                b"\xe2\x28\xa1",
                "byte 0xe2 in position 0: invalid continuation byte",
            ),
            (
                b"\xe2\x82\x28",
                "bytes in position 0-1: invalid continuation byte",
            ),
            (
                b"\xed\xa0\x80",
                "byte 0xed in position 0: invalid continuation byte",
            ),
            (b"x\xc3", "byte 0xc3 in position 1: unexpected end of data"),
            (
                b"\xf0\x9f\x98",
                "bytes in position 0-2: unexpected end of data",
            ),
        ];
        for (bytes, expected) in cases {
            assert_eq!(
                decoded(bytes),
                format!("'utf-8' codec can't decode {expected}"),
                "{bytes:?}"
            );
        }
    }
}
