//! Splicing a bound variadic type parameter into a list of types (the 0026 amendment).
//!
//! `*Ts` stands for a list of types of any length, zero included. It appears only as the last
//! element of a tuple, `tuple[*Ts]`, or the last parameter of a `Callable`,
//! `Callable[[f64, S, *Ts], S]`, as [`TypeKind::Unpacked`]. An instance binds `Ts` to a tuple
//! of the types passed, so substituting turns `*Ts` into `*tuple[f64, f64]`; [`spliced`] then
//! puts that tuple's elements in its place, which gives `tuple[f64, f64]` and
//! `Callable[[f64, S, f64, f64], S]`. Every stage that substitutes type parameters calls it on
//! the elements of a tuple and the parameters of a `Callable`, so a concrete type holds no
//! `Unpacked`.

use crate::{Type, TypeKind};

/// `types` with each `*tuple[A, B]` replaced by `A, B`. A `*Ts` whose `Ts` is still a type
/// parameter stays as it is, since the list's length is not known yet.
///
/// ```
/// use types::{Float, Primitive, Type, TypeKind};
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
/// let float = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
/// let text = Type::new(&database, TypeKind::Str);
/// let bound = Type::new(&database, TypeKind::Tuple(vec![float, float]));
/// let unpacked = Type::new(&database, TypeKind::Unpacked(bound));
///
/// // `str, *tuple[f64, f64]` is `str, f64, f64`.
/// assert_eq!(types::spliced(&database, vec![text, unpacked]), vec![text, float, float]);
/// ```
pub fn spliced<'db>(db: &'db dyn salsa::Database, types: Vec<Type<'db>>) -> Vec<Type<'db>> {
    let bound = |ty: &Type<'db>| match ty.kind(db) {
        TypeKind::Unpacked(pack) => matches!(pack.kind(db), TypeKind::Tuple(_)),
        _ => false,
    };
    if !types.iter().any(bound) {
        return types;
    }
    let mut flat = Vec::with_capacity(types.len());
    for ty in types {
        match ty.kind(db) {
            TypeKind::Unpacked(pack) => match pack.kind(db) {
                TypeKind::Tuple(items) => flat.extend(items.iter().copied()),
                _ => flat.push(ty),
            },
            _ => flat.push(ty),
        }
    }
    flat
}
