//! The built-in type forms that take arguments: `Array[T, rank]`, `Fixed[T, shape]`,
//! `Callable[[A], R]` and `Raises[T, E]`, and the copy types an array may hold.

use crate::lower::Lowerer;
use crate::outline::{Base, Kind, outline};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Shape, Type, TypeDb, TypeKind, Value};

impl<'db> Lowerer<'db> {
    /// `Array[element, rank]`.
    pub(crate) fn array(&mut self, element: NodeId, rank_node: NodeId) -> Type<'db> {
        let element = self.element(element);
        let rank = match self.evaluate(rank_node) {
            Some(Value::Integer(rank)) if rank >= 1 => u32::try_from(rank).ok(),
            None => return self.unknown(),
            _ => None,
        };
        let Some(rank) = rank else {
            return self.error(
                rank_node,
                "an array's rank is a whole number of dimensions, at least 1",
                "not a valid rank",
                Some("for example `Array[f64, 1]` for a vector or `Array[f64, 2]` for a grid"),
            );
        };
        Type::new(self.db, TypeKind::Array { element, rank })
    }

    /// `Fixed[element, shape]`, where the shape is a tuple of constant lengths.
    pub(crate) fn fixed(&mut self, element: NodeId, shape: NodeId) -> Type<'db> {
        let element = self.element(element);
        let note = "a shape is a tuple of lengths, such as `(3,)` or `(3, 3)`";
        if self.tree.kind(shape) != SyntaxKind::Tuple {
            return self.error(shape, "expected a shape", "not a tuple", Some(note));
        }
        let mut lengths = Vec::new();
        for length in self.tree.children_with(shape, Role::Item) {
            match self.evaluate(length) {
                Some(Value::Integer(value)) if value >= 0 => lengths.push(value.cast_unsigned()),
                None => return self.unknown(),
                Some(_) => {
                    return self.error(
                        length,
                        "a length is a whole number, 0 or more",
                        "not a valid length",
                        None,
                    );
                }
            }
        }
        match Shape::new(lengths) {
            Some(shape) => Type::new(self.db, TypeKind::Fixed { element, shape }),
            None => self.error(
                shape,
                "this shape is empty or has more elements than fit in memory",
                "not a valid shape",
                Some(note),
            ),
        }
    }

    /// The element type of an array, which must be a copy type (language reference, "Arrays",
    /// "Element types"): a number, `bool`, a `Fixed` array, an enum or a struct declared
    /// `Copy`.
    fn element(&mut self, node: NodeId) -> Type<'db> {
        let element = self.lower(node);
        if is_copy(self.db, element) {
            return element;
        }
        let message = format!(
            "an array's elements must be a copy type, found `{}`",
            element.display(self.db)
        );
        self.error(
            node,
            &message,
            "not a copy type",
            Some("a copy type is a number, `bool`, a `Fixed` array, an enum, or a struct declared `Copy`; use a `list` for other values"),
        )
    }

    /// `Callable[[parameters], returns]`.
    pub(crate) fn callable(&mut self, parameters: NodeId, returns: NodeId) -> Type<'db> {
        if self.tree.kind(parameters) != SyntaxKind::List {
            return self.error(
                parameters,
                "a `Callable`'s parameter types go in a list",
                "not a list",
                Some("for example `Callable[[f64, Vec3], Vec3]`"),
            );
        }
        let items: Vec<NodeId> = self.tree.children_with(parameters, Role::Item).collect();
        let parameters = items.into_iter().map(|item| self.lower(item)).collect();
        let returns = self.lower(returns);
        Type::new(
            self.db,
            TypeKind::Callable {
                parameters,
                returns,
            },
        )
    }

    /// `Raises[value, errors]`, where the errors are one error type or several joined by `|`.
    pub(crate) fn raises(&mut self, value: NodeId, errors: NodeId) -> Type<'db> {
        let value = self.lower(value);
        let mut nodes = Vec::new();
        self.union_members(errors, &mut nodes);
        let mut lowered = Vec::new();
        for node in nodes {
            let error = self.lower(node);
            if !self.is_error_type(error) {
                let message = format!("`{}` is not an error type", error.display(self.db));
                self.error(
                    node,
                    &message,
                    "expected an error type",
                    Some("an error type is a class with the base `Exception`"),
                );
            }
            lowered.push(error);
        }
        Type::new(
            self.db,
            TypeKind::Fallible {
                value,
                errors: lowered,
            },
        )
    }

    /// The members of `A | B | C`, in order, into `members`.
    fn union_members(&self, node: NodeId, members: &mut Vec<NodeId>) {
        if self.tree.kind(node) == SyntaxKind::BinaryOperation
            && self.tree.node(node).operator() == Operator::BitOr
        {
            for side in [Role::Left, Role::Right] {
                if let Some(side) = self.tree.child(node, side) {
                    self.union_members(side, members);
                }
            }
        } else {
            members.push(node);
        }
    }

    /// Whether `ty` is an error type, or unknown because of an error already reported.
    fn is_error_type(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Unknown => true,
            TypeKind::Struct(nominal) => matches!(
                outline(self.db, nominal.file).get(nominal.name).map(|entry| &entry.kind),
                Some(Kind::Class(class)) if class.base == Base::Error
            ),
            _ => false,
        }
    }
}

/// Whether `ty` is a copy type, one an array may hold: a number, `bool`, a `Fixed` array, an
/// enum or a struct declared `Copy`. An unknown type or a type parameter counts as one, so an
/// error is reported once, where it was made.
pub(crate) fn is_copy<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    match ty.kind(db) {
        TypeKind::Primitive(_)
        | TypeKind::Fixed { .. }
        | TypeKind::Enum(_)
        | TypeKind::Parameter { .. }
        | TypeKind::Unknown => true,
        TypeKind::Struct(nominal) => matches!(
            outline(db, nominal.file).get(nominal.name).map(|entry| &entry.kind),
            Some(Kind::Class(class)) if class.copy
        ),
        _ => false,
    }
}
