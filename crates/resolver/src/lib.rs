//! Name resolution: finds the definition each name in a file refers to.
//!
//! This stage runs after the parser. For every name a file reads, it finds what the name means:
//! a parameter or local variable, a function, class or constant at the top of this or another
//! module, an imported module, a standard library name, or a name the language provides. The
//! result is a [`Resolution`] per file, which the type checker and the
//! language server read through [`Db::resolve`]. It is an implementation
//! crate: it reads parse trees and the module map only through the `database` interface
//! (decisions 0001 and 0016).
//!
//! # What it reports
//!
//! - A name that refers to nothing, such as a misspelled variable.
//! - An import that finds nothing: a module that does not exist, or a member the module does
//!   not define. A member whose name starts with `_` is private to its module.
//! - A relative import, `from . import x`, which is not supported: modules are named from the
//!   package root.
//! - A top-level name defined twice in one module.
//!
//! # The rules
//!
//! Scopes follow Python: see the `scopes` module. One file is one module, named by its path
//! ([`database::module_name()`]). `import a.b` binds `a`, and `a.b.f` then names `f` in module
//! `a.b`; `import a.b as c` binds `c` to `a.b`; `from a.b import f` binds `f`. Top-level names
//! are visible throughout the module, whatever their order, because a module holds only
//! definitions.
//!
//! The standard library modules (`math`, `random`, `sys`, `os`) are accepted by name. A member
//! that the library's own source file for the module defines, such as `os.ErrorKind` in
//! `<std>/os.byn`, resolves to that definition, and a name every module can use that
//! `<std>/builtins.byn` defines, such as `OSError`, resolves to its definition there (decision
//! 0038). Any other member is built into the compiler and checked by the type checker.
//!
//! What it does not do: resolve attributes of values, such as `particle.mass`, which needs the
//! value's type, or check that a variable is assigned before it is read, which needs the
//! order of execution. The type checker does both.
//!
//! # Example
//!
//! The resolver runs inside a database, which the `driver` application normally provides with
//! the real parser. Here a tiny database hands it a tree built by hand for the one-line file
//! `total`, which reads a name nothing defines:
//!
//! ```
//! use database::{Db, File, Package, ParsedFile, Resolution};
//! use diagnostics::render;
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
//!         parse(self, file)
//!     }
//!
//!     fn resolve(&self, package: Package, file: File) -> &Resolution {
//!         resolve(self, package, file)
//!     }
//! }
//!
//! /// The tree of `total`: a module holding one expression statement, a name.
//! #[salsa::tracked(returns(ref), no_eq)]
//! fn parse(db: &dyn Db, file: File) -> ParsedFile {
//!     let mut builder = TreeBuilder::new(file.id(db));
//!     builder.open(SyntaxKind::Module, Role::Root, 0, 6);
//!     builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 5);
//!     builder.leaf(SyntaxKind::Name, Role::Value, 0, 5);
//!     builder.close();
//!     builder.close();
//!     ParsedFile::new(builder.finish(), Vec::new())
//! }
//!
//! #[salsa::tracked(returns(ref))]
//! fn resolve(db: &dyn Db, package: Package, file: File) -> Resolution {
//!     resolver::resolve(db, package, file)
//! }
//!
//! struct Sources(SourceFile);
//! # impl std::fmt::Debug for Sources {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Sources") }
//! # }
//! impl source::Sources for Sources {
//!     fn file(&self, _: FileId) -> &SourceFile {
//!         &self.0
//!     }
//! }
//!
//! let database = Database::default();
//! let source = || SourceFile::new("main.byn", "total\n");
//! let file = File::new(&database, FileId::from_index(0), source()?);
//! let package = Package::new(&database, vec![file]);
//!
//! let resolution = database.resolve(package, file);
//! assert_eq!(
//!     render(&resolution.diagnostics()[0], &Sources(source()?)).to_string(),
//!     "error: undefined name `total`\n --> main.byn:1:1\n  |\n1 | total\n  | ^^^^^ not defined in this scope",
//! );
//! # Ok::<(), source::SourceError>(())
//! ```

mod imports;
mod items;
mod scopes;
mod walk;

use database::{Db, File, Package, Resolution};
use imports::Importer;
use walk::Walker;

/// Resolves every name in `file`, a file of `package`.
///
/// The application calls this from its implementation of [`Db::resolve`], inside a
/// `#[salsa::tracked]` function, so the result is cached until something it read changes: the
/// file's tree, the package's module map, or the top-level names of a module it imports from.
///
/// Cost: one pass over the file's tree, plus a lookup in each imported module's top-level
/// names, which are themselves computed once per module and cached.
#[must_use]
pub fn resolve(db: &dyn Db, package: Package, file: File) -> Resolution {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let modules = database::modules(db, package);
    let items = items::module_items(db, file);
    let importer = Importer::new(db, modules);
    let (references, diagnostics) = Walker::new(tree, text, file, items, importer).run();
    Resolution::new(references, diagnostics)
}
