//! Generics (language reference, "Types", "Generics" and "Traits"): inferring a generic call's
//! type arguments from its arguments, checking them against the type parameters' bounds, and
//! the operations a bound allows inside a generic body.
//!
//! A type argument is never written at a call; each type parameter takes the type of the first
//! argument that fixes it, read left to right, number literals last so that `largest(x, 0)`
//! with `x: f32` makes the `0` an `f32`. Version 0 has only the standard traits:
//!
//! | Trait | Satisfied by | Allows, inside a generic body |
//! | --- | --- | --- |
//! | `Number` | every number type | `+ - * / // % **`, `-x` and ordering with itself, and integer literals |
//! | `Float` | `f32`, `f64` | the same as `Number`, and float literals too |
//! | `VectorSpace` | `f64`, and `Fixed` and `Array` arrays of `f64` | `+` and `-` with itself, `-x`, and `*` by an `f64` on either side |
//! | `Copy` | copy types | copying |

use super::Body;
use super::relations;
use crate::forms::is_copy;
use crate::traits::{Trait, bound};
use database::File;
use syntax::{NodeId, Operator};
use types::{Float, Parameter, Primitive, Signature, Type, TypeKind, Value};

impl<'db> Body<'db> {
    /// The bound of `ty` when it is a type parameter.
    pub(super) fn trait_of(&self, ty: Type<'db>) -> Option<Trait> {
        match ty.kind(self.db) {
            TypeKind::Parameter { file, name } => bound(self.db, *file, *name),
            _ => None,
        }
    }

    /// Whether `ty` satisfies `bound`.
    fn satisfies(&self, bound: Trait, ty: Type<'db>) -> bool {
        if let Some(own) = self.trait_of(ty) {
            // Every float is a number, and every number is a copy type.
            return own == bound
                || (bound == Trait::Number && own == Trait::Float)
                || (bound == Trait::Copy && own.is_numeric());
        }
        let f64 = |ty: Type<'db>| {
            matches!(
                ty.kind(self.db),
                TypeKind::Primitive(Primitive::Float(Float::F64)) | TypeKind::Unknown
            )
        };
        match (bound, ty.kind(self.db)) {
            (_, TypeKind::Unknown) | (Trait::Float, TypeKind::Primitive(Primitive::Float(_))) => {
                true
            }
            (Trait::Number, TypeKind::Primitive(primitive)) => *primitive != Primitive::Bool,
            (
                Trait::VectorSpace,
                TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. },
            ) => f64(*element),
            (Trait::VectorSpace, _) => f64(ty),
            (Trait::Copy, _) => is_copy(self.db, ty),
            _ => false,
        }
    }

    /// A call to a generic function or constructor: infers the type arguments from the
    /// arguments, checks each argument against its parameter and each type argument against
    /// its bound, and returns the return type with the type arguments substituted, the type of
    /// each argument, and the type arguments. `arguments` pairs each argument node with the index of its
    /// parameter in `parameters`.
    pub(super) fn generic_call(
        &mut self,
        node: NodeId,
        callee: &str,
        signature: &Signature<'db>,
        parameters: &[Parameter<'db>],
        arguments: &[(NodeId, usize)],
    ) -> (Type<'db>, Vec<Type<'db>>, Vec<Type<'db>>) {
        let names: Vec<(File, NodeId)> = signature
            .type_parameters
            .iter()
            .filter_map(|ty| match ty.kind(self.db) {
                TypeKind::Parameter { file, name } => Some((*file, *name)),
                _ => None,
            })
            .collect();
        let mut bindings: Vec<Option<Type<'db>>> = vec![None; names.len()];
        let mut found: Vec<Option<Type<'db>>> = vec![None; arguments.len()];
        for literals in [false, true] {
            for (slot, &(argument, index)) in arguments.iter().enumerate() {
                if self.is_number_literal(argument) != literals {
                    continue;
                }
                let declared = parameters[index].ty;
                let expected = self.instantiate(declared, &names, &bindings, false);
                let ty = self.expression(argument, expected);
                self.unify(declared, ty, &names, &mut bindings);
                found[slot] = Some(ty);
            }
        }
        for (slot, &(argument, index)) in arguments.iter().enumerate() {
            let parameter = &parameters[index];
            let expected = self
                .instantiate(parameter.ty, &names, &bindings, true)
                .unwrap_or_else(|| self.lowerer.unknown());
            if let Some(ty) = found[slot]
                && !relations::compatible(self.db, expected, ty)
            {
                let message = format!(
                    "`{}` has type `{}` here, found `{}`",
                    parameter.name,
                    self.show(expected),
                    self.show(ty)
                );
                let label = format!("type `{}`", self.show(ty));
                self.lowerer.error(argument, &message, &label, None);
            }
        }
        for (&(file, name), binding) in names.iter().zip(&bindings) {
            let (Some(bound), Some(ty)) = (bound(self.db, file, name), *binding) else {
                continue;
            };
            if !self.satisfies(bound, ty) {
                let parameter = self
                    .db
                    .parse(file)
                    .tree()
                    .text(name, file.source(self.db).text());
                let message = format!(
                    "`{callee}` needs `{parameter}` to be a `{}`, found `{}`",
                    bound.name(),
                    self.show(ty)
                );
                self.lowerer
                    .error(node, &message, "bound not satisfied", None);
            }
        }
        let returns = self
            .instantiate(signature.returns, &names, &bindings, true)
            .unwrap_or_else(|| self.lowerer.unknown());
        let found = found
            .into_iter()
            .map(|ty| ty.unwrap_or_else(|| self.lowerer.unknown()))
            .collect();
        let type_arguments = bindings
            .into_iter()
            .map(|ty| ty.unwrap_or_else(|| self.lowerer.unknown()))
            .collect();
        (returns, found, type_arguments)
    }

    /// `ty` with the type arguments found so far substituted. With `fill`, a type parameter
    /// not yet found becomes unknown; without it, `None` is returned while any remains, so a
    /// literal is not typed against a half-known type.
    fn instantiate(
        &self,
        ty: Type<'db>,
        names: &[(File, NodeId)],
        bindings: &[Option<Type<'db>>],
        fill: bool,
    ) -> Option<Type<'db>> {
        let mut result = ty;
        for (&(file, name), binding) in names.iter().zip(bindings) {
            let argument = match binding {
                Some(argument) => *argument,
                None if fill => self.lowerer.unknown(),
                None => continue,
            };
            result = relations::substitute(self.db, result, file, &[name], &[argument]);
        }
        // A parameter bound to itself is resolved: inside a method of `Pair[T]`, the
        // constructor call `Pair(self.b, self.a)` binds the class's `T` to that same `T`.
        let unresolved = names
            .iter()
            .zip(bindings)
            .filter(|(_, binding)| binding.is_none())
            .any(|(&(file, name), _)| {
                let parameter = Type::new(self.db, TypeKind::Parameter { file, name });
                relations::mentions(self.db, result, parameter)
            });
        (!unresolved).then_some(result)
    }

    /// Records in `bindings` the type arguments that make `declared` match `found`.
    fn unify(
        &self,
        declared: Type<'db>,
        found: Type<'db>,
        names: &[(File, NodeId)],
        bindings: &mut [Option<Type<'db>>],
    ) {
        let db = self.db;
        match (declared.kind(db), found.kind(db)) {
            (_, TypeKind::Unknown) => {}
            (_, TypeKind::Function { callable, .. }) => {
                self.unify(declared, *callable, names, bindings);
            }
            (TypeKind::Parameter { file, name }, _) => {
                if let Some(index) = names.iter().position(|entry| *entry == (*file, *name))
                    && bindings[index].is_none()
                {
                    bindings[index] = Some(found);
                }
            }
            (TypeKind::List(a), TypeKind::List(b))
            | (TypeKind::Optional(a), TypeKind::Optional(b))
            | (TypeKind::Array { element: a, .. }, TypeKind::Array { element: b, .. })
            | (TypeKind::Fixed { element: a, .. }, TypeKind::Fixed { element: b, .. }) => {
                self.unify(*a, *b, names, bindings);
            }
            (TypeKind::Optional(a), _) => self.unify(*a, found, names, bindings),
            (TypeKind::Tuple(a), TypeKind::Tuple(b)) if a.len() == b.len() => {
                for (&a, &b) in a.iter().zip(b) {
                    self.unify(a, b, names, bindings);
                }
            }
            (TypeKind::Struct(a), TypeKind::Struct(b)) | (TypeKind::Enum(a), TypeKind::Enum(b))
                if a.file == b.file && a.name == b.name =>
            {
                for (&a, &b) in a.arguments.iter().zip(&b.arguments) {
                    self.unify(a, b, names, bindings);
                }
            }
            (
                TypeKind::Callable {
                    parameters: a,
                    returns: r,
                },
                TypeKind::Callable {
                    parameters: b,
                    returns: s,
                },
            ) if a.len() == b.len() => {
                for (&a, &b) in a.iter().zip(b) {
                    self.unify(a, b, names, bindings);
                }
                self.unify(*r, *s, names, bindings);
            }
            _ => {}
        }
    }

    /// `left op right` when either side is a type parameter: what its bound allows, or an
    /// error. `None` when neither side is a type parameter.
    pub(super) fn generic_arithmetic(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        let (left_bound, right_bound) = (self.trait_of(left), self.trait_of(right));
        let is_parameter = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Parameter { .. });
        if !is_parameter(left) && !is_parameter(right) {
            return None;
        }
        if matches!(left.kind(self.db), TypeKind::Unknown)
            || matches!(right.kind(self.db), TypeKind::Unknown)
        {
            return Some(self.lowerer.unknown());
        }
        let arithmetic = matches!(
            operator,
            Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::FloorDivide
                | Operator::Modulo
                | Operator::Power
        );
        let f64 = |ty: Type<'db>| {
            matches!(
                ty.kind(self.db),
                TypeKind::Primitive(Primitive::Float(Float::F64))
            )
        };
        let result = match (left_bound, right_bound) {
            (Some(bound), _) if left == right && bound.is_numeric() && arithmetic => Some(left),
            (Some(Trait::VectorSpace), _)
                if left == right && matches!(operator, Operator::Add | Operator::Subtract) =>
            {
                Some(left)
            }
            (Some(Trait::VectorSpace), _) if operator == Operator::Multiply && f64(right) => {
                Some(left)
            }
            (_, Some(Trait::VectorSpace)) if operator == Operator::Multiply && f64(left) => {
                Some(right)
            }
            _ => None,
        };
        Some(result.unwrap_or_else(|| {
            let symbol = operator.symbol();
            let message = format!(
                "`{symbol}` is not allowed between `{}` and `{}`",
                self.show(left),
                self.show(right)
            );
            self.lowerer.error(
                node,
                &message,
                "the type parameter's bound does not allow it",
                Some("a `Number` or `Float` type parameter combines with itself and literals; a `VectorSpace` one adds to itself and scales by an `f64`"),
            )
        }))
    }

    /// The type a number literal next to a type parameter takes: the parameter itself under a
    /// `Number` or `Float` bound, an `f64` under `VectorSpace`.
    pub(super) fn generic_literal(&self, ty: Type<'db>) -> Option<Type<'db>> {
        match self.trait_of(ty)? {
            bound if bound.is_numeric() => Some(ty),
            Trait::VectorSpace => Some(Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Float(Float::F64)),
            )),
            _ => None,
        }
    }

    /// The type of a number literal where a type parameter's value is expected. Under
    /// `Float`, any real literal takes the type parameter; under `Number`, only an integer
    /// literal does, as the type argument may be an integer type; under `VectorSpace`, the
    /// literal is an `f64`. Anything else is an error.
    pub(super) fn generic_literal_type(&mut self, node: NodeId, parameter: Type<'db>) -> Type<'db> {
        let bound = self.trait_of(parameter);
        if bound == Some(Trait::VectorSpace) {
            let f64 = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
            return self.literal(node, Some(f64));
        }
        let float = matches!(self.lowerer.evaluate_quietly(node), Some(Value::Float(_)));
        let allowed = !self.is_imaginary(node)
            && match bound {
                Some(Trait::Float) => true,
                Some(Trait::Number) => !float,
                _ => false,
            };
        if allowed {
            return parameter;
        }
        let shown = self.show(parameter);
        let message = format!("a number literal cannot have the type `{shown}`");
        let hint = match bound {
            Some(Trait::Number) if float => {
                format!("`{shown}` may be an integer type; bound it by `Float` to use this literal")
            }
            _ => format!("bound `{shown}` by `Number` or `Float` to use number literals"),
        };
        self.lowerer
            .error(node, &message, "not allowed by the bound", Some(&hint))
    }

    /// Whether values of `ty`, a type parameter, can be ordered with `<`: under a `Number` or
    /// `Float` bound.
    pub(super) fn is_ordered_parameter(&self, ty: Type<'db>) -> bool {
        self.trait_of(ty).is_some_and(Trait::is_numeric)
    }
}
