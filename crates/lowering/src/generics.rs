//! Generic calls: a call site's view of a generic function's parameter types.
//!
//! A generic function's signature names its type parameters, `x: Owned[T]`. Every value a
//! caller builds for the call must have the type the call gives that parameter: the `int` that
//! `T` stands for at `first(3)`, or the `int | None` that `T | None` stands for at `show(3)`.
//! Monomorphization replaces the callee's type parameters only inside the callee's body, never
//! in its callers, so lowering instantiates the parameter types itself before it moves,
//! converts or borrows an argument. In a caller that is generic itself, a type argument may be
//! the caller's own parameter, `U` at `first(u)` in `def wrap[U](u: Owned[U])`; the argument
//! then has type `U`, which the caller's own instances replace.

use crate::builder::Builder;
use syntax::Role;
use types::{Item, Signature, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The type parameters a call of `item` gives `type_arguments` for, in the same order: the
    /// class's first for a method of a generic class (roadmap 1.9.18), then the function's own.
    /// The implicit `Callable` type parameters of decision 0026 follow in `type_arguments` and
    /// are left out here; a `Callable` parameter keeps its own type at the call.
    pub(crate) fn type_parameters(
        &self,
        item: Item<'db>,
        signature: &Signature<'db>,
    ) -> Vec<Type<'db>> {
        let file = item.file(self.db);
        let tree = self.db.parse(file).tree();
        let class = crate::definition(tree, item.name(self.db)).and_then(|(_, class)| class);
        class
            .and_then(|class| tree.child(class, Role::TypeParameters))
            .into_iter()
            .flat_map(|parameters| tree.children_with(parameters, Role::Item))
            .filter_map(|parameter| tree.child(parameter, Role::Name))
            .map(|name| Type::new(self.db, TypeKind::Parameter { file, name }))
            .chain(signature.type_parameters.iter().copied())
            .collect()
    }

    /// `ty` with each of `parameters` replaced by the type at the same position in
    /// `arguments`: `T | None` becomes `int | None` when `T` is `int`.
    ///
    /// A union is returned as it is: version 0 has no generic unions (decision 0032), so a
    /// union never mentions a type parameter. A `T | None` is an optional, not a union.
    ///
    /// Cost: one visit of `ty`'s nodes; a type that mentions no parameter is rebuilt into the
    /// same interned type.
    pub(crate) fn instantiate(
        &self,
        ty: Type<'db>,
        parameters: &[Type<'db>],
        arguments: &[Type<'db>],
    ) -> Type<'db> {
        if let Some(position) = parameters.iter().position(|&parameter| parameter == ty) {
            return arguments.get(position).copied().unwrap_or(ty);
        }
        let map = |ty: Type<'db>| self.instantiate(ty, parameters, arguments);
        let list = |types: &[Type<'db>]| types.iter().map(|&ty| map(ty)).collect::<Vec<_>>();
        let kind = match ty.kind(self.db) {
            TypeKind::Primitive(_)
            | TypeKind::Str
            | TypeKind::TextArray
            | TypeKind::None
            | TypeKind::Parameter { .. }
            | TypeKind::Union(_)
            | TypeKind::Function { .. }
            | TypeKind::Standard(_)
            | TypeKind::Unknown => return ty,
            TypeKind::Tuple(items) => TypeKind::Tuple(list(items)),
            TypeKind::List(item) => TypeKind::List(map(*item)),
            TypeKind::Dict { key, value } => TypeKind::Dict {
                key: map(*key),
                value: map(*value),
            },
            TypeKind::Optional(value) => TypeKind::Optional(map(*value)),
            TypeKind::Array { element, rank } => TypeKind::Array {
                element: map(*element),
                rank: *rank,
            },
            TypeKind::Fixed { element, shape } => TypeKind::Fixed {
                element: map(*element),
                shape: shape.clone(),
            },
            // A schema struct can be generic, so its columns and row type are mapped too.
            TypeKind::DataFrame { columns, row } => TypeKind::DataFrame {
                columns: columns.iter().map(|column| column.map(map)).collect(),
                row: row.map(map),
            },
            TypeKind::Struct(nominal) => TypeKind::Struct(types::Nominal {
                arguments: list(&nominal.arguments),
                ..nominal.clone()
            }),
            TypeKind::Enum(nominal) => TypeKind::Enum(types::Nominal {
                arguments: list(&nominal.arguments),
                ..nominal.clone()
            }),
            TypeKind::Callable {
                parameters: callable,
                returns,
            } => TypeKind::Callable {
                parameters: list(callable),
                returns: map(*returns),
            },
            TypeKind::Fallible { value, errors } => TypeKind::Fallible {
                value: map(*value),
                errors: list(errors),
            },
        };
        Type::new(self.db, kind)
    }
}
