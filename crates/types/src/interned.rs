//! The interned [`Type`] id.
//!
//! Salsa writes the type's constructor (`new`) and getter. It writes the constructor without
//! documentation, so this module allows that lint; the struct's documentation describes it, and
//! the module holds nothing else.
#![expect(missing_docs, reason = "salsa generates `new` without documentation")]

use crate::TypeKind;

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
