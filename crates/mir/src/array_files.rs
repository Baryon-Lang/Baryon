//! The operations on array files a [`Builtin::ArrayFile`](crate::Builtin::ArrayFile) call
//! performs (decision 0036, part 4): `Array.fromfile` and `a.tofile`.

/// An operation on an array file, called by [`Builtin::ArrayFile`](crate::Builtin::ArrayFile).
///
/// An array file holds an array's elements and nothing else: each element's bytes as they lie in
/// memory, in row-major order, with no header, as `numpy.fromfile` reads and
/// `numpy.ndarray.tofile` writes. Both operations can fail, so the call has an error edge whose
/// local is a `Raises[None, OSError]`, as a [`FileMethod`](crate::FileMethod) call has. A `str`
/// argument is passed `Borrow`, or `Owned` for a literal. Each costs one pass over the array's
/// bytes, with no work per element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum ArrayFile {
    /// `Array.fromfile(path, dtype, count, offset)`: three arguments, the path, a `str`, then
    /// the count and the offset, each an `i64` passed `Owned`. The destination is a new
    /// `Array[T, 1]`, and its `T` is the element type read: a number, a `bool`, a `Fixed` array
    /// or a `Copy` struct of them. The call skips `offset` bytes, then reads `count` elements,
    /// or every element to the end of the file when `count` is negative. It fails with the kind
    /// `INVALID_DATA` when the bytes after the offset are not a whole number of elements, or
    /// are fewer than `count` elements. A byte read into a `bool` becomes `True` unless it is 0.
    FromFile,
    /// `a.tofile(path)`: two arguments, the array, passed `Borrow`, and the path, a `str`; the
    /// destination has type `None`. The call makes the file, or empties the one there, and
    /// writes every element the array sees, so a view writes only its own elements, in
    /// row-major order.
    ToFile,
}

impl ArrayFile {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::ArrayFile::FromFile.name(), "Array.fromfile");
    /// assert_eq!(mir::ArrayFile::ToFile.name(), "Array.tofile");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::FromFile => "Array.fromfile",
            Self::ToFile => "Array.tofile",
        }
    }
}
