//! Facts about types that lowering decides by: which values copy and which own memory, and the
//! type two numbers meet at.

use crate::builder::Builder;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether a value of type `ty` is copied rather than moved: numbers, `bool`, `None`,
    /// enums, `Fixed` arrays, functions, `Copy` structs, and tuples, optionals and unions of
    /// those.
    /// The same rule as the checker's moves (decision 0006). A type parameter copies when its
    /// bound is `Copy`, `Number` or `Float`.
    pub(crate) fn is_copy(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Primitive(_)
            | TypeKind::None
            | TypeKind::Enum(_)
            | TypeKind::Fixed { .. }
            | TypeKind::Callable { .. }
            | TypeKind::Function { .. }
            | TypeKind::Unknown => true,
            TypeKind::Str
            | TypeKind::TextArray
            | TypeKind::List(_)
            | TypeKind::Dict { .. }
            | TypeKind::Array { .. }
            | TypeKind::DataFrame { .. }
            | TypeKind::Standard(_) => false,
            TypeKind::Tuple(items) | TypeKind::Union(items) => {
                items.iter().all(|&item| self.is_copy(item))
            }
            TypeKind::Optional(value) => self.is_copy(*value),
            TypeKind::Fallible { value, errors } => {
                self.is_copy(*value) && errors.iter().all(|&error| self.is_copy(error))
            }
            TypeKind::Struct(nominal) => {
                let item = Item::new(self.db, self.package, nominal.file, nominal.name);
                matches!(
                    self.db.declaration(item).declaration,
                    Declaration::Struct { copy: true, .. }
                )
            }
            TypeKind::Parameter { file, name } => *file == self.file && self.copy_bound(*name),
        }
    }

    /// Whether `ty` names a type parameter anywhere inside it, as a generic struct's field
    /// `T | None` does. Such a type means nothing in a body that is not generic, so a value is
    /// not converted to it.
    pub(crate) fn is_generic(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Parameter { .. } => true,
            TypeKind::Tuple(items) => items.iter().any(|&item| self.is_generic(item)),
            TypeKind::List(element)
            | TypeKind::Optional(element)
            | TypeKind::Array { element, .. }
            | TypeKind::Fixed { element, .. } => self.is_generic(*element),
            TypeKind::Dict { key, value } => self.is_generic(*key) || self.is_generic(*value),
            TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => nominal
                .arguments
                .iter()
                .any(|&argument| self.is_generic(argument)),
            TypeKind::Callable {
                parameters,
                returns,
            } => {
                parameters
                    .iter()
                    .any(|&parameter| self.is_generic(parameter))
                    || self.is_generic(*returns)
            }
            TypeKind::Fallible { value, errors } => {
                self.is_generic(*value) || errors.iter().any(|&error| self.is_generic(error))
            }
            _ => false,
        }
    }

    /// Whether a value of type `ty` owns memory that a drop frees. Every type that moves does.
    pub(crate) fn needs_drop(&self, ty: Type<'db>) -> bool {
        !self.is_copy(ty)
    }

    /// Whether the type parameter whose `Name` node is `name`, declared by this function or its
    /// class, has a bound whose types all copy.
    fn copy_bound(&self, name: NodeId) -> bool {
        matches!(
            self.parameter_bound(name),
            Some("Copy" | "Number" | "Float")
        )
    }

    /// Whether `ty` is a type parameter of the function being lowered, or of its class, bounded
    /// by `Float`, so every instance is `f32` or `f64` and float arithmetic, comparisons and
    /// `math` functions apply to it as they do to those types.
    pub(crate) fn is_float_parameter(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Parameter { file, name } => {
                *file == self.file && self.parameter_bound(*name) == Some("Float")
            }
            _ => false,
        }
    }

    /// Whether `ty` is a type parameter of the function being lowered, or of its class, bounded
    /// by `Number` or `Float`, so every instance is a number type and has a zero and a one.
    pub(crate) fn is_number_parameter(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Parameter { file, name } => {
                *file == self.file
                    && matches!(self.parameter_bound(*name), Some("Number" | "Float"))
            }
            _ => false,
        }
    }

    /// The name of the built-in trait that bounds the type parameter `name`, such as `"Float"`
    /// in `def f[T: Float]`, looked up in the function being lowered and then its class.
    fn parameter_bound(&self, name: NodeId) -> Option<&'static str> {
        let tree = self.tree;
        let bound = [Some(self.function), self.class]
            .into_iter()
            .flatten()
            .filter_map(|definition| tree.child(definition, Role::TypeParameters))
            .flat_map(|parameters| tree.children(parameters))
            .filter(|&variable| tree.kind(variable) == SyntaxKind::TypeVariable)
            .find(|&variable| tree.child(variable, Role::Name) == Some(name))
            .and_then(|variable| tree.child(variable, Role::Bound))?;
        match self.definition(bound) {
            Some(Definition::Builtin(builtin)) => Some(builtin),
            _ => None,
        }
    }

    /// The number type two operands of these types convert to before they are combined or
    /// compared (decision 0014): the same type, the float for an integer and a float, the
    /// complex type for a real number and a complex one. The checker accepted the program, so
    /// any other pair is already the same type, and the left one is returned.
    pub(crate) fn common(&self, left: Type<'db>, right: Type<'db>) -> Type<'db> {
        let rank = |ty: Type<'db>| match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Integer(_)) => 1,
            TypeKind::Primitive(Primitive::Float(_)) => 2,
            TypeKind::Primitive(Primitive::Complex(_)) => 3,
            _ => 0,
        };
        if rank(right) > rank(left) {
            right
        } else {
            left
        }
    }

    /// Whether `ty` is an integer type.
    pub(crate) fn is_integer(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Integer(_)))
    }

    /// The type a `T | None` holds, or `None` when `ty` is not optional.
    pub(crate) fn optional_value(&self, ty: Type<'db>) -> Option<Type<'db>> {
        match ty.kind(self.db) {
            TypeKind::Optional(value) => Some(*value),
            _ => None,
        }
    }

    /// Whether a value of type `from` changes representation when used as a `to`, as
    /// `coerce` does it: a number converted, or a value wrapped as a `T | None` or a union.
    pub(crate) fn converts(&self, from: Type<'db>, to: Type<'db>) -> bool {
        if from == to {
            return false;
        }
        if let Some(value) = self.optional_value(to)
            && (value == from || matches!(from.kind(self.db), TypeKind::None))
        {
            return true;
        }
        if self
            .union_members(to)
            .is_some_and(|members| members.contains(&from))
        {
            return true;
        }
        matches!(
            (from.kind(self.db), to.kind(self.db)),
            (TypeKind::Primitive(_), TypeKind::Primitive(_))
        )
    }
}
