//! The steps a text array is built from and read by, which a
//! [`Builtin::TextArray`](crate::Builtin::TextArray) call performs (decision 0045, roadmap
//! 3.13.3).
//!
//! A text array, `Array[str, 1]`, is stored as Arrow's `large_utf8` lays text out: the bytes of
//! every element in UTF-8, end to end, in one `Array[u8, 1]`, and the offset each element
//! starts at in an `Array[i64, 1]`. Lowering writes the loops over those two arrays itself, so
//! all the mid-level IR adds is the three steps that cross between a `str` and the bytes.

/// A step between a `str` and the buffer of bytes of a text array, called by
/// [`Builtin::TextArray`](crate::Builtin::TextArray).
///
/// None of them checks anything or can fail: the stage that writes the calls sizes the buffer
/// from [`ByteLength`](Self::ByteLength) before it copies, and reads only at the positions
/// [`CopyBytes`](Self::CopyBytes) gave. The buffer holds its elements one after another, with
/// stride 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum TextArrayStep {
    /// The number of bytes a `str` takes in UTF-8: one argument passed `Borrow`, or `Owned` for
    /// a literal; the destination has type `i64`. Unlike
    /// [`Builtin::LengthStr`](crate::Builtin::LengthStr) it counts bytes, not characters, so it
    /// reads only the length word and takes constant time.
    ByteLength,
    /// Copies the bytes of a `str` into the buffer: three arguments, the `str` passed `Borrow`,
    /// or `Owned` for a literal, the buffer, an `Array[u8, 1]` passed `InOut`, and the position
    /// in the buffer of the first byte written, an `i64` passed `Owned`; the destination has
    /// type `i64` and gets the position after the last byte written, which is where the next
    /// string goes. The buffer must have room for the string's bytes from the position. It
    /// costs time linear in the string's bytes.
    CopyBytes,
    /// One element, borrowed: three arguments, the buffer, an `Array[u8, 1]` passed `Borrow`,
    /// then the position of the element's first byte and its length in bytes, each an `i64`
    /// passed `Owned`; the destination is a `str` that points into the buffer, flagged as not
    /// owned (decision 0022), so it is never freed and must not outlive the buffer. The bytes
    /// named must lie inside the buffer and be whole UTF-8 characters. It copies nothing and
    /// takes constant time.
    View,
}

impl TextArrayStep {
    /// A name for the step, for messages about what a backend does not build.
    ///
    /// ```
    /// assert_eq!(mir::TextArrayStep::ByteLength.name(), "bytes of str");
    /// assert_eq!(mir::TextArrayStep::CopyBytes.name(), "str into text array");
    /// assert_eq!(mir::TextArrayStep::View.name(), "text array element");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ByteLength => "bytes of str",
            Self::CopyBytes => "str into text array",
            Self::View => "text array element",
        }
    }
}
