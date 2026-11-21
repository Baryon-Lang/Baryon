//! The database's inputs: the values the application sets, from which every query is computed.
//!
//! Salsa writes each input's constructor (`new`), getters and setters (`set_<field>`). It writes
//! the constructor and setters without documentation, so this module allows that lint. Each
//! struct's documentation describes them instead, and the module holds nothing else.
#![expect(
    missing_docs,
    reason = "salsa generates `new`, `builder` and setters without documentation"
)]

use source::{FileId, SourceFile};

/// One source file: an input to the database.
///
/// Its name is its path relative to the package root, with `/` between directories, such as
/// `physics/nbody.byn`; the name decides the file's [`ModuleName`](crate::ModuleName). The
/// application loads the file and creates this input with `File::new`; an editor changes the
/// text with `set_source`. The `id` never changes once the file exists, because spans in cached
/// results carry it.
#[salsa::input(debug)]
pub struct File {
    /// The id that spans in this file carry. Unique within the database.
    #[returns(copy)]
    pub id: FileId,
    /// The file's name and text.
    #[returns(ref)]
    pub source: SourceFile,
}

/// A package: the set of source files compiled together. An input to the database.
///
/// The application lists the files under a package root. Adding or removing a file means
/// setting a new list with `set_files`. `Package::new` takes the files only; `blas` starts
/// `false` and is changed with `set_blas`.
#[salsa::input(debug)]
pub struct Package {
    /// Every source file in the package, in any order.
    #[returns(ref)]
    pub files: Vec<File>,
    /// Whether the package is built with `--blas` (decision 0040): `@` of two `f32` or `f64`
    /// matrices then calls the system BLAS instead of compiled loops. `false` by default, and
    /// for checking, where it changes nothing; only lowering reads it, so changing it rebuilds
    /// the bodies and nothing before them.
    #[default]
    pub blas: bool,
}
