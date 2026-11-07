//! The incremental query database: the compiler's memory between one edit and the next.
//!
//! Every stage after reading files runs as a *query*: a function of the database whose result
//! is cached, along with the inputs and other queries it read. When an input changes, only the
//! queries that read it, directly or through other queries, run again; everything else answers
//! from the cache. That is what keeps a one-function edit cheap in the language server and in
//! incremental builds, and the batch compiler and the language server share the same engine.
//! The engine is the `salsa` crate (decision 0016).
//!
//! This is an interface crate. It holds:
//!
//! - the **inputs**: each source [`File`], and the [`Package`] that lists them;
//! - the **file and module database**: the [`ModuleName`] of each file and the [`Modules`] map of
//!   a package, which name resolution reads to find what an `import` refers to;
//! - the results stages exchange through the database: a [`ParsedFile`], and a file's
//!   [`Resolution`], which says what each name refers to;
//! - the [`Db`] trait, with one method per stage query that an implementation crate provides,
//!   such as [`Db::parse`] and [`Db::resolve`].
//!
//! # How stages plug in
//!
//! A stage that needs another stage's result calls the [`Db`] method for it and never the other
//! stage's crate, so implementations stay replaceable (decision 0001). The application crate
//! owns the concrete database: it implements each [`Db`] method by calling an implementation
//! inside a `#[salsa::tracked]` function, which is what caches the result.
//!
//! # Example
//!
//! A database whose "parser" returns an empty module, enough to show the wiring. The real one,
//! which calls the `parser` crate, lives in the `driver` application.
//!
//! ```
//! use database::{Db, File, Package, ParsedFile, Resolution};
//! use source::{FileId, SourceFile};
//! use syntax::{Role, SyntaxKind, TreeBuilder};
//!
//! #[salsa::db]
//! #[derive(Clone, Default)]
//! struct Database {
//!     storage: salsa::Storage<Self>,
//! }
//!
//! #[salsa::db]
//! impl salsa::Database for Database {}
//!
//! #[salsa::db]
//! impl Db for Database {
//!     fn parse(&self, file: File) -> &ParsedFile {
//!         parse_empty(self, file)
//!     }
//!
//!     fn resolve(&self, package: Package, file: File) -> &Resolution {
//!         resolve_nothing(self, package, file)
//!     }
//! }
//!
//! /// Cached per file; runs again only when that file's source changes.
//! #[salsa::tracked(returns(ref), no_eq)]
//! fn parse_empty(db: &dyn Db, file: File) -> ParsedFile {
//!     let length = file.source(db).text().len() as u32;
//!     let mut builder = TreeBuilder::new(file.id(db));
//!     builder.open(SyntaxKind::Module, Role::Root, 0, length);
//!     builder.close();
//!     ParsedFile::new(builder.finish(), Vec::new())
//! }
//!
//! /// An empty module has no names to resolve.
//! #[salsa::tracked(returns(ref))]
//! fn resolve_nothing(db: &dyn Db, package: Package, file: File) -> Resolution {
//!     Resolution::new(Vec::new(), Vec::new())
//! }
//!
//! let database = Database::default();
//! let source = SourceFile::new("physics/nbody.byn", "")?;
//! let file = File::new(&database, FileId::from_index(0), source);
//! let package = Package::new(&database, vec![file]);
//!
//! let modules = database::modules(&database, package);
//! assert!(modules.diagnostics().is_empty());
//! let name = database::ModuleName::from_dotted("physics.nbody").unwrap();
//! assert_eq!(modules.get(&name), Some(file));
//! assert!(database.parse(file).diagnostics().is_empty());
//! # Ok::<(), source::SourceError>(())
//! ```

mod inputs;
mod module_map;
mod names;

pub use inputs::{File, Package};
pub use module_map::{ModuleName, Modules, module_name, modules};
pub use names::{BUILTINS, Definition, Resolution, STANDARD_MODULES};

use diagnostics::Diagnostic;
use syntax::SyntaxTree;

/// The queries every stage can ask of the database.
///
/// Each method is one stage's result, provided by the application, which wires it to an
/// implementation crate and caches it. A stage calls these methods instead of depending on the
/// crate that computes them.
#[salsa::db]
pub trait Db: salsa::Database {
    /// The syntax tree and syntax errors of `file`.
    ///
    /// The result is cached until the file's source changes. The tree is complete even when the
    /// file has errors.
    fn parse(&self, file: File) -> &ParsedFile;

    /// What each name in `file`, a file of `package`, refers to, and the errors found: names
    /// that refer to nothing, imports that find nothing, and clashing definitions.
    ///
    /// The result is cached until something it read changes: the file's tree, the package's
    /// module map, or the top-level names of a module it imports from.
    fn resolve(&self, package: Package, file: File) -> &Resolution;
}

/// A parsed file: its syntax tree and its syntax errors. The result of [`Db::parse`].
#[derive(Clone, Debug)]
pub struct ParsedFile {
    /// The file's syntax tree, complete even when the file has errors.
    tree: SyntaxTree,
    /// One error per syntax error, in source order. Empty for a valid file.
    diagnostics: Vec<Diagnostic>,
}

impl ParsedFile {
    /// Bundles a tree with the syntax errors found while building it.
    ///
    /// ```
    /// use database::ParsedFile;
    /// use source::FileId;
    /// use syntax::{Role, SyntaxKind, TreeBuilder};
    ///
    /// let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// builder.open(SyntaxKind::Module, Role::Root, 0, 0);
    /// builder.close();
    /// let parsed = ParsedFile::new(builder.finish(), Vec::new());
    /// assert_eq!(parsed.tree().len(), 1);
    /// ```
    #[must_use]
    pub fn new(tree: SyntaxTree, diagnostics: Vec<Diagnostic>) -> Self {
        Self { tree, diagnostics }
    }

    /// The file's syntax tree, complete even when the file has errors.
    #[must_use]
    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    /// One error per syntax error, in source order. Empty for a valid file.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
