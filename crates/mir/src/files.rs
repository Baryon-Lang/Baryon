//! The operations on text files a [`Builtin::File`](crate::Builtin::File) call performs
//! (decision 0036, parts 3 and 4): `open`, and the methods of the `File` it returns.

/// An operation on a text file, called by [`Builtin::File`](crate::Builtin::File).
///
/// Every one can fail, so its call has an error edge whose local is a `Raises[None, OSError]`,
/// with `OSError` the class of the standard library's own source (decision 0038); a backend
/// writes a failure there as a compiled function that raises does. A `File` argument is passed
/// `Borrow`, except to [`Close`](Self::Close), which takes it `Owned`; a `str` argument is passed
/// `Borrow`, or `Owned` for a literal. The file's own buffer is 64 KiB, so a run of small reads
/// or writes costs one system call per 64 KiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum FileMethod {
    /// `open(path, mode)`: two arguments, the path, a `str`, and the mode, an `i64` constant
    /// passed `Owned`: 0 for `"r"`, 1 for `"w"`, 2 for `"a"`, 3 for `"x"`. The destination is
    /// a new `File`.
    Open,
    /// `f.read()`: one argument, the file; the destination is a new `str`, the rest of the file.
    Read,
    /// `f.readline()`: one argument, the file; the destination is a new `str`, the next line
    /// with its `\n`, or `""` at the end.
    ReadLine,
    /// `f.readlines()`: one argument, the file; the destination is a new `list[str]` of the
    /// remaining lines.
    ReadLines,
    /// `f.write(text)`: two arguments, the file and the text; the destination is an `i64`, the
    /// number of characters written.
    Write,
    /// `f.flush()`: one argument, the file; the destination has type `None`.
    Flush,
    /// `f.close()` and the end of a `with` block: one argument, the file, passed `Owned`, which
    /// the call closes whether or not it fails; the destination has type `None`.
    Close,
}

impl FileMethod {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::FileMethod::Open.name(), "open");
    /// assert_eq!(mir::FileMethod::ReadLine.name(), "File.readline");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Read => "File.read",
            Self::ReadLine => "File.readline",
            Self::ReadLines => "File.readlines",
            Self::Write => "File.write",
            Self::Flush => "File.flush",
            Self::Close => "File.close",
        }
    }
}
