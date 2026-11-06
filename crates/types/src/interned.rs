//! The interned ids: [`Type`] and [`Item`].
//!
//! Salsa writes each struct's constructor (`new`) and getters. It writes the constructor without
//! documentation, so this module allows that lint; each struct's documentation describes it, and
//! the module holds nothing else.
#![expect(missing_docs, reason = "salsa generates `new` without documentation")]

use crate::TypeKind;
use database::{File, Package};
use syntax::NodeId;

/// One type, interned in the database. Two `Type`s are equal exactly when they are the same
/// type. Build one with `Type::new(db, kind)`; read what it is with `kind(db)`.
///
/// The `'db` lifetime ties the id to the database revision that created it, because the
/// database may free interned types that a new revision no longer uses.
#[salsa::interned(debug)]
pub struct Type<'db> {
    /// What the type is.
    #[returns(ref)]
    pub kind: TypeKind<'db>,
}

/// One top-level definition or method, interned: a function, struct, enum, error type, type
/// alias or constant. Build one with `Item::new(db, package, file, name)`; the type checker's
/// results for it are read with [`TypeDb::declaration`](crate::TypeDb::declaration).
#[salsa::interned(debug)]
pub struct Item<'db> {
    /// The package the definition belongs to, which its names are resolved in.
    #[returns(copy)]
    pub package: Package,
    /// The file that holds the definition.
    #[returns(copy)]
    pub file: File,
    /// The definition's `Name` node in that file: the name after `def` or `class`, or the
    /// target of the alias or constant.
    #[returns(copy)]
    pub name: NodeId,
}
