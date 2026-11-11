//! The operations on paths and directories a [`Builtin::Path`](crate::Builtin::Path) call
//! performs (decision 0036, parts 1 and 7): the parts of a `pathlib.Path`, what it asks the file
//! system, and the directory functions of `os`.

/// An operation on a path, called by [`Builtin::Path`](crate::Builtin::Path).
///
/// A path argument is the path's text, a `str`, passed `Borrow`, or `Owned` for a literal: a
/// `pathlib.Path` is a struct that holds one `str`, and the stage that writes the call passes
/// that field. A path's text is in the form `pathlib` keeps it: no empty or `.` component and no
/// `/` at the end, with `.` for the empty path. [`New`](Self::New) and [`Join`](Self::Join) make
/// that form, and the methods that take a path's parts rely on it.
///
/// The methods fall in three groups. Those that only read text, [`New`](Self::New) to
/// [`Parent`](Self::Parent), never touch the file system and cannot fail. Those that ask whether
/// something exists, [`Exists`](Self::Exists) to [`IsDirectory`](Self::IsDirectory), cost one
/// system call and cannot fail: any error means `False`, as in Python. The rest can fail, so the
/// call of each has an error edge whose local is a `Raises[None, OSError]`, as a
/// [`FileMethod`](crate::FileMethod) has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum PathMethod {
    /// `Path(text)`: one argument, any text; the destination is a new `str`, the text in the
    /// form `pathlib` keeps, so `"runs//out/"` gives `"runs/out"` and `""` gives `"."`.
    New,
    /// `path / other`: two arguments, a path's text and any text; the destination is a new
    /// `str`, the two joined with `/` in the form `pathlib` keeps. An `other` that starts with
    /// `/` replaces the path, as in Python.
    Join,
    /// `path.name`: one argument; the destination is a new `str`, the last component, or `""`
    /// for `.` and for a root.
    Name,
    /// `path.stem`: one argument; the destination is a new `str`, the name without its suffix.
    Stem,
    /// `path.suffix`: one argument; the destination is a new `str`, the name's last `.` and
    /// what follows, such as `".csv"`, or `""` when the name has no `.` after its first
    /// character or ends with one.
    Suffix,
    /// `path.parent`: one argument; the destination is a new `str`, the path without its last
    /// component: `.` for a single name, and a root for a root.
    Parent,
    /// `path.exists()`: one argument; the destination is a `bool`, whether the path names
    /// anything, following symbolic links.
    Exists,
    /// `path.is_file()`: one argument; the destination is a `bool`, whether the path names a
    /// regular file, following symbolic links.
    IsFile,
    /// `path.is_dir()`: one argument; the destination is a `bool`, whether the path names a
    /// directory, following symbolic links.
    IsDirectory,
    /// `path.iterdir()`: one argument, the directory; the destination is a new `list` of paths,
    /// each the directory joined with one entry's name, in the order the operating system gives.
    /// A name that is not UTF-8 fails with the kind `INVALID_DATA`. It reads the whole
    /// directory: one allocation per entry.
    IterateDirectory,
    /// `os.listdir(path)`: one argument, the directory; the destination is a new `list[str]` of
    /// the entries' names, in the order the operating system gives, without `.` and `..`. A
    /// name that is not UTF-8 fails with the kind `INVALID_DATA`.
    ListDirectory,
    /// `os.remove(path)`: one argument, a file; the destination has type `None`. A directory
    /// fails, as in Python.
    Remove,
    /// `os.rename(source, destination)`: two arguments; the destination has type `None`. A
    /// file at the new path is replaced, as on Linux and macOS in Python.
    Rename,
    /// `os.mkdir(path)`: one argument; the destination has type `None`. A path that exists
    /// fails with the kind `FILE_EXISTS`, and a missing parent with `FILE_NOT_FOUND`.
    MakeDirectory,
    /// `os.makedirs(path, exist_ok)`: two arguments, the path and a `bool` passed `Owned`; the
    /// destination has type `None`. It makes every missing directory on the way. A path that
    /// is already a directory fails with the kind `FILE_EXISTS` unless the `bool` is true.
    MakeDirectories,
    /// `os.getcwd()`: no arguments; the destination is a new `str`, the directory the program
    /// runs in. One that is not UTF-8 fails with the kind `INVALID_DATA`.
    CurrentDirectory,
}

impl PathMethod {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::PathMethod::Join.name(), "Path /");
    /// assert_eq!(mir::PathMethod::IterateDirectory.name(), "Path.iterdir");
    /// assert_eq!(mir::PathMethod::MakeDirectories.name(), "os.makedirs");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::New => "pathlib.Path",
            Self::Join => "Path /",
            Self::Name => "Path.name",
            Self::Stem => "Path.stem",
            Self::Suffix => "Path.suffix",
            Self::Parent => "Path.parent",
            Self::Exists => "Path.exists",
            Self::IsFile => "Path.is_file",
            Self::IsDirectory => "Path.is_dir",
            Self::IterateDirectory => "Path.iterdir",
            Self::ListDirectory => "os.listdir",
            Self::Remove => "os.remove",
            Self::Rename => "os.rename",
            Self::MakeDirectory => "os.mkdir",
            Self::MakeDirectories => "os.makedirs",
            Self::CurrentDirectory => "os.getcwd",
        }
    }

    /// Whether the method can fail with `OSError`, so its call has an error edge.
    ///
    /// ```
    /// assert!(mir::PathMethod::ListDirectory.can_fail());
    /// assert!(!mir::PathMethod::Exists.can_fail());
    /// ```
    #[must_use]
    pub fn can_fail(self) -> bool {
        matches!(
            self,
            Self::IterateDirectory
                | Self::ListDirectory
                | Self::Remove
                | Self::Rename
                | Self::MakeDirectory
                | Self::MakeDirectories
                | Self::CurrentDirectory
        )
    }
}
