//! Relations between types: when a value of one type may stand where another is expected, and
//! substituting a generic class's type arguments into its fields and methods.

use database::File;
use syntax::NodeId;
use types::{Nominal, Parameter, Signature, Type, TypeDb, TypeKind};

/// Whether a value of type `found` may stand where `expected` is needed.
///
/// Equal types match. A type that contains an unknown type, from an error already reported,
/// matches anything, so one mistake gives one message. A type parameter is rigid: inside a
/// generic body, `T` matches only `T`. `None` and a `T` match `T | None`; a `T | None` does not
/// match `T` until a `None` check narrows it (the `flow` module). A function's own type matches
/// a `Callable` with its signature (decision 0026). A member of a union, such as a `Dirichlet`,
/// matches the union, `Periodic | Dirichlet`; the reverse needs a `match` (decision 0032).
pub(crate) fn compatible<'db>(db: &'db dyn TypeDb, expected: Type<'db>, found: Type<'db>) -> bool {
    let unknown = |ty: Type<'db>| matches!(ty.kind(db), TypeKind::Unknown);
    if expected == found || contains(db, expected, &unknown) || contains(db, found, &unknown) {
        return true;
    }
    match (expected.kind(db), found.kind(db)) {
        (TypeKind::Optional(_), TypeKind::None) => true,
        (TypeKind::Optional(value), _) => compatible(db, *value, found),
        // A member of a union stands where the union is expected (decision 0032). A narrower
        // union does not yet: its tags differ, and lowering does not renumber them.
        (TypeKind::Union(members), _) => members.contains(&found),
        (TypeKind::Callable { .. }, TypeKind::Function { callable, .. }) => {
            compatible(db, expected, *callable)
        }
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

/// Whether `ty` contains an unknown type anywhere, from an error already reported.
pub(crate) fn is_unknown<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    contains(db, ty, &|ty| matches!(ty.kind(db), TypeKind::Unknown))
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
        | TypeKind::None
        | TypeKind::Function { .. }
        | TypeKind::Standard(_) => false,
        TypeKind::Tuple(items) | TypeKind::Union(items) => all(items),
        TypeKind::List(item) | TypeKind::Optional(item) => contains(db, *item, test),
        TypeKind::Dict { key, value } => contains(db, *key, test) || contains(db, *value, test),
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
        | TypeKind::Function { .. }
        | TypeKind::Standard(_)
        | TypeKind::Unknown => return ty,
        TypeKind::Tuple(items) => TypeKind::Tuple(all(items)),
        TypeKind::List(item) => TypeKind::List(map(*item)),
        TypeKind::Dict { key, value } => TypeKind::Dict {
            key: map(*key),
            value: map(*value),
        },
        TypeKind::Optional(value) => TypeKind::Optional(map(*value)),
        // Rebuilt through `types::union`, since two members may become one type.
        TypeKind::Union(members) => return types::union(db, all(members)),
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

/// `signature` with [`substitute`] applied to each parameter's type and the return type: a
/// method of `Pair[T]` called on a `Pair[f64]`, or a trait's method with `Self` replaced by the
/// type that implements it.
pub(crate) fn substitute_signature<'db>(
    db: &'db dyn TypeDb,
    signature: &Signature<'db>,
    file: File,
    parameters: &[NodeId],
    arguments: &[Type<'db>],
) -> Signature<'db> {
    let map = |ty| substitute(db, ty, file, parameters, arguments);
    Signature {
        type_parameters: signature.type_parameters.clone(),
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| Parameter {
                ty: map(parameter.ty),
                ..parameter.clone()
            })
            .collect(),
        returns: map(signature.returns),
    }
}
