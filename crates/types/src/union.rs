//! Building a union type, `A | B | C`, in the one canonical form every stage relies on
//! (decision 0032).
//!
//! A union is the same type however it is written: `Dirichlet | Periodic` is
//! `Periodic | Dirichlet`, `(A | B) | C` is `A | B | C`, and `A | A` is `A`, as in Python. So
//! that two spellings intern to the same [`Type`], and so that a member's tag is the same number
//! in every module, [`union`] puts the members in one order: structs and enums by the path of
//! the file that declares them and then their written name, `None` last.

use crate::{Type, TypeKind};
use database::Db;

/// The union of `members`: one of them, whichever a value holds.
///
/// The members are flattened (a member that is itself a union or a `T | None` adds its own
/// members), repeats are removed, and the rest are put in the canonical order the module
/// describes. What is left decides the type:
///
/// - one member is that member's type, so `A | A` is `A`;
/// - one member and `None` is [`TypeKind::Optional`], the `T | None` the language had before
///   decision 0032, which this does not change;
/// - anything else is [`TypeKind::Union`].
///
/// A member of type [`TypeKind::Unknown`] makes the whole union unknown, so an error already
/// reported in one member gives no second error about the union. Which member types are
/// allowed is the type checker's rule, not this function's: it accepts any.
///
/// ```
/// use types::{Type, TypeKind};
///
/// # #[salsa::db]
/// # #[derive(Clone, Default)]
/// # struct Database { storage: salsa::Storage<Self> }
/// # #[salsa::db]
/// # impl salsa::Database for Database {}
/// # #[salsa::db]
/// # impl database::Db for Database {
/// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
/// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
/// #         unreachable!()
/// #     }
/// # }
/// let database = Database::default();
/// let none = Type::new(&database, TypeKind::None);
/// let text = Type::new(&database, TypeKind::Str);
///
/// // One member and `None` is the optional type that already existed.
/// let optional = types::union(&database, [none, text, text]);
/// assert_eq!(optional, Type::new(&database, TypeKind::Optional(text)));
/// ```
pub fn union<'db>(db: &'db dyn Db, members: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    let mut flat = Vec::new();
    for member in members {
        flatten(db, member, &mut flat);
    }
    if flat
        .iter()
        .any(|member| matches!(member.kind(db), TypeKind::Unknown))
    {
        return Type::new(db, TypeKind::Unknown);
    }
    flat.sort_by_cached_key(|&member| order(db, member));
    flat.dedup();
    match flat.as_slice() {
        [single] => *single,
        [value, none] if matches!(none.kind(db), TypeKind::None) => {
            Type::new(db, TypeKind::Optional(*value))
        }
        _ => Type::new(db, TypeKind::Union(flat)),
    }
}

/// Adds `member` to `flat`, or its members when it is a union or an optional type.
fn flatten<'db>(db: &'db dyn Db, member: Type<'db>, flat: &mut Vec<Type<'db>>) {
    match member.kind(db) {
        TypeKind::Union(members) => flat.extend_from_slice(members),
        TypeKind::Optional(value) => {
            flat.push(*value);
            flat.push(Type::new(db, TypeKind::None));
        }
        _ => flat.push(member),
    }
}

/// Where `member` goes in a union: classes first, by the declaring file's path and then the
/// type as written, then any other type as written, then `None`. The key depends only on the
/// program's text, never on the order types were interned in, so it is the same in every module
/// and every run.
fn order(db: &dyn Db, member: Type<'_>) -> (u8, String, String) {
    let written = member.display(db).to_string();
    match member.kind(db) {
        TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => {
            (0, nominal.file.source(db).name().to_owned(), written)
        }
        TypeKind::None => (2, String::new(), written),
        _ => (1, String::new(), written),
    }
}
