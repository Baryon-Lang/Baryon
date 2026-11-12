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
//!   another type.
//! - **Arrays** (roadmap 1.7.4): copy-type elements, one index per dimension, slices as views,
//!   the version 0 methods, and whole-array arithmetic on matching ranks and shapes.
//! - **Generics** (roadmap 1.7.5): type arguments inferred from a call's arguments and checked
//!   against the bounds `Number`, `Float`, `VectorSpace` and `Copy`, and, inside a generic
//!   body, only the operations a type parameter's bound allows.
//! - **Enums and `match`** (roadmap 1.7.6): each pattern checked against the subject, and the
//!   cases without a guard covering every value, with an error that names a value they miss.
//! - **Errors** (roadmap 1.7.7): every error type a call or `raise` can fail with is caught by
//!   a `try` around it or declared in the function's `Raises`.
//! - **Optional values** (roadmap 1.7.8): a `T | None` is used as a `T` only where a `None`
//!   check, an early exit or an assignment has narrowed it.
//! - **Definite assignment** (roadmap 1.7.9): a variable is read only where every path has
//!   assigned it, with the error pointing at the path that skips it, and a function that
//!   returns a value cannot reach its end. See the `body` module for how each is checked.
//! - **Traits** (roadmap 1.7.10, decision 0031): traits declared as `class Body(Protocol)`
//!   (the `bases` module), a struct's promise to define the methods of each trait it names,
//!   checked at the class (the `classes` and `conformance` modules), and bounds by a trait,
//!   whose methods a generic body may call (the `traits` module and `body::methods`).
//! - **Embedded structs** (roadmap 1.7.12): `Embed[T]` fields whose names the outer struct
//!   reaches, with a name two embedded structs provide at one depth reported at the class (the
//!   `embed` and `classes` modules).
//!
//! It is an implementation crate. It reads trees and names only through the `database`
//! interface and publishes its results through the `types` interface, whose
//! [`TypeDb::declaration`] and [`TypeDb::body`] queries the application wires to
//! [`declaration`] and [`body()`] (decision 0016).
//!
//! # The rules
//!
//! - **Annotations** lower to [`Type`]s by what their names resolve to: the
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
//! use types::{Checked, Declaration, Item, TypeDb, TypedBody, Value};
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
//!
//!     fn body<'db>(&'db self, item: Item<'db>) -> &'db TypedBody<'db> {
//!         body(self, item)
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
//! #[salsa::tracked(returns(ref))]
//! fn body<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> TypedBody<'db> {
//!     checker::body(db, item)
//! }
//!
//! let database = Database::default();
//! let source = SourceFile::new("main.byn", "N: Final = 64\n")?;
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

mod bases;
mod body;
mod classes;
mod conformance;
mod constant;
mod declare;
mod embed;
mod forms;
mod frames;
mod lower;
mod outline;
mod padding;
mod traits;

use database::{File, Package};
use diagnostics::Diagnostic;
use lower::Lowerer;
use outline::Kind;
use types::{Checked, Declaration, Declared, Item, Nominal, Type, TypeDb, TypeKind, TypedBody};

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

/// The checked body of the function or method `item`: the type of every expression, what each
/// call calls, and the errors in the body (see the `body` module). Empty for any other item, and
/// for a trait's method, whose body is `...`.
///
/// The application calls this from its implementation of [`TypeDb::body`], inside a
/// `#[salsa::tracked]` function, so each body is checked once per revision and both
/// [`check_file`] and lowering read the same result. See the crate example.
#[must_use]
pub fn body<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> TypedBody<'db> {
    let (package, file, name) = (item.package(db), item.file(db), item.name(db));
    let outline = outline::outline(db, file);
    match outline.get(name) {
        // A trait's method has the body `...`, which the `classes` module checks.
        Some(entry)
            if entry
                .class
                .is_some_and(|class| traits::is_trait(db, file, class)) =>
        {
            TypedBody::default()
        }
        Some(entry) if entry.kind == Kind::Function => body::check(db, package, file, entry),
        _ => TypedBody::default(),
    }
}

/// What the struct or enum `nominal`, a class of `package`, declares, as [`types::layout()`]
/// and the passes after lowering read it: a struct's field types with its type arguments
/// substituted, or an enum's number of variants. The application passes it, closed over its
/// database and package, to drop elaboration, array expansion and the backend.
///
/// Cost: the class's cached declaration, and one substitution per field of a generic struct.
#[must_use]
pub fn declared<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
) -> Declared<'db> {
    padding::declared(db, package, nominal)
}

/// The method that implements the trait method `method` for the type `implementor`, as each
/// instance of a generic function calls it (decision 0031): the method itself, the positions of
/// the embedded fields its receiver is reached through, outermost first, and the type arguments
/// of its class, which a generic struct's method is compiled at. `None` when `method` is not a
/// trait's method, or `implementor` is not a struct that has it, which the checker reports
/// before any instance is made. The application passes it, closed over its database and
/// package, to monomorphization.
///
/// Cost: one lookup of the method's name on the struct and the structs it embeds.
#[must_use]
pub fn implementation<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    method: Item<'db>,
    implementor: Type<'db>,
) -> Option<(Item<'db>, Vec<u32>, Vec<Type<'db>>)> {
    let file = method.file(db);
    let class = outline::outline(db, file).get(method.name(db))?.class?;
    if !traits::is_trait(db, file, class) {
        return None;
    }
    let TypeKind::Struct(nominal) = implementor.kind(db) else {
        return None;
    };
    let name = db
        .parse(file)
        .tree()
        .text(method.name(db), file.source(db).text());
    match embed::lookup(db, package, nominal, name) {
        embed::Lookup::Found {
            path,
            owner,
            member: embed::Own::Method { item, .. },
        } => Some((item, path, owner.arguments)),
        _ => None,
    }
}

/// Every type error and warning in `file`, a file of `package`: the errors of each definition's
/// declaration and, for a function, of its body, in source order, methods after their class,
/// then the errors that compare classes, such as a struct missing a method of a trait it names
/// (the `classes` module), then a warning for each struct whose fields would take fewer bytes in
/// another order (the `padding` module).
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
                diagnostics.extend(db.body(item).diagnostics.iter().cloned());
            }
            diagnostics
        })
        .chain(classes::check(db, package, file))
        .chain(padding::padding(db, package, file))
        .collect()
}
