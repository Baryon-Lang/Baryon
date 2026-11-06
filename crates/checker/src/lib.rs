//! The type checker: works out what every definition declares in types, and checks programs
//! against it.
//!
//! It runs after name resolution, in two parts:
//!
//! - **Declarations** (roadmap 1.7.2): each function's signature, each struct's fields and
//!   methods, each enum's variants, the type each alias names and the type and value of each
//!   `Final` constant. It reports annotations that mean no type, such as `Array[f64]` without a
//!   rank, and constants it cannot compute.
//! - **Function bodies** (roadmap 1.7.3): the type of every expression, inferred locally, and
//!   the errors in how values are used: numbers that do not mix, conditions that are not
//!   `bool`, calls with the wrong arguments, missing fields, and a variable given a value of
//!   another type. See the `body` module for what later slices add.
//!
//! It is an implementation crate. It reads trees and names only through the `database`
//! interface and publishes its results through the `types` interface, whose
//! [`TypeDb::declaration`] query the application wires to [`declaration`] (decision 0016).
//!
//! # The rules
//!
//! - **Annotations** lower to [`Type`](types::Type)s by what their names resolve to: the
//!   language's type names (`f64`, `int`, `str`, `list`, `Array`, `Fixed` and the rest), classes
//!   and aliases from any module, and type parameters in scope. `T | None` is the only union;
//!   `InOut[T]` and `Owned[T]` wrap only a parameter's type and set its mode.
//! - **Shapes and ranks** are constants: `Fixed[f64, (N, N)]` with `N: Final = 3` is a 3 × 3
//!   matrix.
//! - **Constants** follow the language's arithmetic, computed by the compiler; see the
//!   `constant` module.
//! - **A method's `self`** may leave out its type: it is a read-only borrow of the class.
//! - **One error per mistake**: a name the resolver could not find, or a definition whose own
//!   declaration had an error, lowers to an unknown type without a second message. Each alias
//!   or constant in a cycle, such as `A: Final = B + 1` with `B: Final = A`, gets one error,
//!   from [`cycle`].
//!
//! # Example
//!
//! The checker runs inside a database, which the `driver` application provides with the real
//! parser and resolver. Here a tiny database hands it the tree of `N: Final = 64`, built by
//! hand, and resolves `Final` to the language's name:
//!
//! ```
//! use database::{Db, Definition, File, Package, ParsedFile, Resolution};
//! use source::{FileId, SourceFile};
//! use syntax::{Role, SyntaxKind, TreeBuilder};
//! use types::{Checked, Declaration, Item, TypeDb, Value};
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
//! #[salsa::db]
//! impl TypeDb for Database {
//!     fn declaration<'db>(&'db self, item: Item<'db>) -> &'db Checked<'db> {
//!         declaration(self, item)
//!     }
//! }
//!
//! /// The tree of `N: Final = 64`.
//! #[salsa::tracked(returns(ref), no_eq)]
//! fn parse(db: &dyn Db, file: File) -> ParsedFile {
//!     let mut builder = TreeBuilder::new(file.id(db));
//!     builder.open(SyntaxKind::Module, Role::Root, 0, 14);
//!     builder.open(SyntaxKind::AnnotatedAssign, Role::Body, 0, 13);
//!     builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
//!     builder.leaf(SyntaxKind::Name, Role::Annotation, 3, 8);
//!     builder.leaf(SyntaxKind::Integer, Role::Value, 11, 13);
//!     builder.close();
//!     builder.close();
//!     ParsedFile::new(builder.finish(), Vec::new())
//! }
//!
//! /// `Final` is the language's own name.
//! #[salsa::tracked(returns(ref))]
//! fn resolve(db: &dyn Db, package: Package, file: File) -> Resolution {
//!     let tree = db.parse(file).tree();
//!     let statement = tree.children(tree.root()).next().unwrap();
//!     let annotation = tree.child(statement, Role::Annotation).unwrap();
//!     Resolution::new(vec![(annotation, Definition::Builtin("Final"))], Vec::new())
//! }
//!
//! #[salsa::tracked(returns(ref), cycle_result = cycle)]
//! fn declaration<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Checked<'db> {
//!     checker::declaration(db, item)
//! }
//!
//! fn cycle<'db>(db: &'db dyn TypeDb, _: salsa::Id, item: Item<'db>) -> Checked<'db> {
//!     checker::cycle(db, item)
//! }
//!
//! let database = Database::default();
//! let source = SourceFile::new("main.npl", "N: Final = 64\n")?;
//! let file = File::new(&database, FileId::from_index(0), source);
//! let package = Package::new(&database, vec![file]);
//!
//! assert!(checker::check_file(&database, package, file).is_empty());
//! let tree = database.parse(file).tree();
//! let statement = tree.children(tree.root()).next().unwrap();
//! let name = tree.child(statement, Role::Target).unwrap();
//! let checked = database.declaration(Item::new(&database, package, file, name));
//! let Declaration::Constant { ty, value } = &checked.declaration else {
//!     unreachable!("`N` is a constant");
//! };
//! assert_eq!(ty.display(&database).to_string(), "i64");
//! assert_eq!(*value, Some(Value::Integer(64)));
//! # Ok::<(), source::SourceError>(())
//! ```

mod body;
mod constant;
mod declare;
mod lower;
mod outline;
mod padding;

use database::{File, Package};
use diagnostics::Diagnostic;
use lower::Lowerer;
use outline::Kind;
use types::{Checked, Declaration, Item, TypeDb};

/// What `item` declares, and the errors found reading its annotations and computing its value.
///
/// The application calls this from its implementation of [`TypeDb::declaration`], inside a
/// `#[salsa::tracked]` function whose cycle fallback is [`cycle`], so the result is cached until
/// the item's file, its names, or a declaration it reads changes. See the crate example.
///
/// An item that names no definition, which only a hand-made `Item` can, declares an unknown
/// alias.
///
/// Cost: one pass over the definition's annotations; an alias or constant it names is read
/// through the cached query.
#[must_use]
pub fn declaration<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Checked<'db> {
    let (package, file, name) = (item.package(db), item.file(db), item.name(db));
    let Some(entry) = outline::outline(db, file).get(name) else {
        let lowerer = Lowerer::new(db, package, file, None, name);
        let unknown = lowerer.unknown();
        return declare::finish(lowerer, Declaration::Alias(unknown));
    };
    let class = match entry.kind {
        Kind::Class(_) => Some(entry.name),
        _ => entry.class,
    };
    Lowerer::new(db, package, file, class, entry.statement).declare(entry)
}

/// The declaration of `item` when computing it needs its own result: an alias or constant
/// defined in terms of itself. It reports the cycle once, at the item's name, and declares an
/// unknown type, so every definition in the cycle ends without a further error.
///
/// The application passes this as the `cycle_result` of the tracked function behind
/// [`TypeDb::declaration`]. See the crate example.
#[must_use]
pub fn cycle<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Checked<'db> {
    let (package, file, name) = (item.package(db), item.file(db), item.name(db));
    let mut lowerer = Lowerer::new(db, package, file, None, name);
    let declaration = match outline::outline(db, file).get(name) {
        Some(entry) => declare::cyclic(&mut lowerer, entry),
        None => Declaration::Alias(lowerer.unknown()),
    };
    declare::finish(lowerer, declaration)
}

/// Every type error and warning in `file`, a file of `package`: the errors of each definition's
/// declaration and, for a function, of its body, in source order, methods after their class,
/// then a warning for each struct whose fields would take fewer bytes in another order (see
/// the `padding` module).
///
/// The application caches this per file in a tracked function. See the crate example.
#[must_use]
pub fn check_file(db: &dyn TypeDb, package: Package, file: File) -> Vec<Diagnostic> {
    outline::outline(db, file)
        .entries()
        .iter()
        .flat_map(|entry| {
            let item = Item::new(db, package, file, entry.name);
            let mut diagnostics = db.declaration(item).diagnostics.clone();
            if entry.kind == Kind::Function {
                diagnostics.extend(body::check(db, package, file, entry));
            }
            diagnostics
        })
        .chain(padding::padding(db, package, file))
        .collect()
}
