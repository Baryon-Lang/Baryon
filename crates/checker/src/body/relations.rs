//! Relations between types: when a value of one type may stand where another is expected, and
//! substituting a generic class's type arguments into its fields and methods.

use database::File;
use syntax::NodeId;
use types::{Nominal, Type, TypeDb, TypeKind};

/// Whether a value of type `found` may stand where `expected` is needed.
///
/// Equal types match. A type that contains an unknown type, from an error already reported,
/// matches anything, so one mistake gives one message. A type parameter is rigid: inside a
/// generic body, `T` matches only `T`. `None` and a `T` match `T | None`. A `T | None` also
/// matches `T` until the `None` check that narrows it is checked (roadmap 1.7.8).
pub(crate) fn compatible<'db>(db: &'db dyn TypeDb, expected: Type<'db>, found: Type<'db>) -> bool {
    let unknown = |ty: Type<'db>| matches!(ty.kind(db), TypeKind::Unknown);
    if expected == found || contains(db, expected, &unknown) || contains(db, found, &unknown) {
        return true;
    }
    match (expected.kind(db), found.kind(db)) {
        (TypeKind::Optional(_), TypeKind::None) => true,
        (TypeKind::Optional(value), _) => compatible(db, *value, found),
        (_, TypeKind::Optional(value)) => compatible(db, expected, *value),
        _ => false,
    }
}

/// Whether `ty` contains an unknown type or a type parameter anywhere: a type whose operations
/// the checker cannot judge on its own.
pub(crate) fn is_open<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    contains(db, ty, &|ty| {
        matches!(ty.kind(db), TypeKind::Unknown | TypeKind::Parameter { .. })
    })
}

/// Whether `ty` contains `parameter` anywhere, so `list[T]` mentions `T`.
pub(crate) fn mentions<'db>(db: &'db dyn TypeDb, ty: Type<'db>, parameter: Type<'db>) -> bool {
    contains(db, ty, &|ty| ty == parameter)
}

/// Whether `ty` or any type inside it passes `test`.
fn contains<'db>(db: &'db dyn TypeDb, ty: Type<'db>, test: &dyn Fn(Type<'db>) -> bool) -> bool {
    if test(ty) {
        return true;
    }
    let all = |types: &[Type<'db>]| types.iter().any(|&ty| contains(db, ty, test));
    match ty.kind(db) {
        TypeKind::Unknown
        | TypeKind::Parameter { .. }
        | TypeKind::Primitive(_)
        | TypeKind::Str
        | TypeKind::None => false,
        TypeKind::Tuple(items) => all(items),
        TypeKind::List(item) | TypeKind::Optional(item) => contains(db, *item, test),
        TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => {
            contains(db, *element, test)
        }
        TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => all(&nominal.arguments),
        TypeKind::Callable {
            parameters,
            returns,
        } => all(parameters) || contains(db, *returns, test),
        TypeKind::Fallible { value, errors } => contains(db, *value, test) || all(errors),
    }
}

/// `ty` with each type parameter in `parameters`, declared in `file`, replaced by the matching
/// type in `arguments`: a field `first: T` of `Pair[f64]` has the type `f64`.
pub(crate) fn substitute<'db>(
    db: &'db dyn TypeDb,
    ty: Type<'db>,
    file: File,
    parameters: &[NodeId],
    arguments: &[Type<'db>],
) -> Type<'db> {
    if parameters.is_empty() {
        return ty;
    }
    let map = |ty: Type<'db>| substitute(db, ty, file, parameters, arguments);
    let all = |types: &[Type<'db>]| types.iter().map(|&ty| map(ty)).collect::<Vec<_>>();
    let kind = match ty.kind(db) {
        TypeKind::Parameter {
            file: declared,
            name,
        } if *declared == file => {
            return parameters
                .iter()
                .position(|parameter| parameter == name)
                .and_then(|index| arguments.get(index).copied())
                .unwrap_or(ty);
        }
        TypeKind::Primitive(_)
        | TypeKind::Str
        | TypeKind::None
        | TypeKind::Parameter { .. }
        | TypeKind::Unknown => return ty,
        TypeKind::Tuple(items) => TypeKind::Tuple(all(items)),
        TypeKind::List(item) => TypeKind::List(map(*item)),
        TypeKind::Optional(value) => TypeKind::Optional(map(*value)),
        TypeKind::Array { element, rank } => TypeKind::Array {
            element: map(*element),
            rank: *rank,
        },
        TypeKind::Fixed { element, shape } => TypeKind::Fixed {
            element: map(*element),
            shape: shape.clone(),
        },
        TypeKind::Struct(nominal) => TypeKind::Struct(Nominal {
            arguments: all(&nominal.arguments),
            ..nominal.clone()
        }),
        TypeKind::Enum(nominal) => TypeKind::Enum(Nominal {
            arguments: all(&nominal.arguments),
            ..nominal.clone()
        }),
        TypeKind::Callable {
            parameters: callable,
            returns,
        } => TypeKind::Callable {
            parameters: all(callable),
            returns: map(*returns),
        },
        TypeKind::Fallible { value, errors } => TypeKind::Fallible {
            value: map(*value),
            errors: all(errors),
        },
    };
    Type::new(db, kind)
}
