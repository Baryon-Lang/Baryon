//! The built-in type forms: `Array[T, rank]`, `Fixed[T, shape]`, `Callable[[A], R]`,
//! `Raises[T, E]` and unions `A | B` (decision 0032), and the copy types an array may hold.

use crate::lower::Lowerer;
use crate::outline::{Base, Kind, outline};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Shape, Type, TypeDb, TypeKind, Value};

impl<'db> Lowerer<'db> {
    /// `Array[element, rank]`.
    pub(crate) fn array(&mut self, element: NodeId, rank_node: NodeId) -> Type<'db> {
        if matches!(self.lower(element).kind(self.db), TypeKind::Str) {
            return self.text_array(rank_node);
        }
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

    /// `Array[str, rank]`: a text array (decision 0045), the one array whose elements are not
    /// a copy type, which has one dimension only.
    fn text_array(&mut self, rank_node: NodeId) -> Type<'db> {
        match self.evaluate(rank_node) {
            Some(Value::Integer(1)) => Type::new(self.db, TypeKind::TextArray),
            None => self.unknown(),
            Some(_) => self.error(
                rank_node,
                "an array of `str` has one dimension",
                "not 1",
                Some("write `Array[str, 1]`; for a grid of text use a `list` of them"),
            ),
        }
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
            // An alias of several error types, `IOErrors = ReadError | WriteError`, lists them.
            if let TypeKind::Union(members) = error.kind(self.db)
                && members.iter().all(|&member| self.is_error_type(member))
            {
                lowered.extend_from_slice(members);
                continue;
            }
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

    /// `A | B | C`: `T | None`, or a union of structs, enums, error types and `None`
    /// (decision 0032). Nested unions and aliases of unions are flattened, order and repeats
    /// do not matter, and [`types::union`] builds the one canonical type.
    ///
    /// Any type may be the `T` of `T | None`. A union with two or more members besides `None`
    /// takes only structs, error types and enums: a number, string, array or other type, or a
    /// type parameter, is an error at the first member that brings one in.
    pub(crate) fn union(&mut self, node: NodeId) -> Type<'db> {
        let mut nodes = Vec::new();
        self.union_members(node, &mut nodes);
        let written: Vec<(NodeId, Type<'db>)> = nodes
            .into_iter()
            .map(|member| (member, self.lower(member)))
            .collect();
        let union = types::union(self.db, written.iter().map(|&(_, ty)| ty));
        if !matches!(union.kind(self.db), TypeKind::Union(_)) {
            return union;
        }
        // The first member that cannot be in a union is reported; one mistake, one message.
        for (member, ty) in written {
            let parts = match ty.kind(self.db) {
                TypeKind::Union(members) => members.clone(),
                TypeKind::Optional(value) => vec![*value],
                _ => vec![ty],
            };
            if let Some(&bad) = parts.iter().find(|&&part| !self.is_union_member(part)) {
                let message = format!("`{}` cannot be a member of a union", bad.display(self.db));
                self.error(
                    member,
                    &message,
                    "not a struct, enum or error type",
                    Some(
                        "a union's members are structs, enums, error types and `None`; numbers, \
                         strings, arrays and type parameters are not supported in version 0 \
                         (decision 0032)",
                    ),
                );
                return self.unknown();
            }
        }
        union
    }

    /// Whether `ty` may be one of several members of a union: a struct or error type whose
    /// type arguments name no type parameter, an enum, or `None`. A type with an error already
    /// reported counts as one, so the error is not repeated.
    fn is_union_member(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::None | TypeKind::Unknown => true,
            TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => !nominal
                .arguments
                .iter()
                .any(|&argument| crate::body::relations::is_open(self.db, argument)),
            _ => false,
        }
    }

    /// The members of `A | B | C`, in order, into `members`.
    pub(crate) fn union_members(&self, node: NodeId, members: &mut Vec<NodeId>) {
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
/// enum, a struct declared `Copy`, or a union whose members are all of these or `None`
/// (decision 0032). An unknown type or a type parameter counts as one, so an
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
        TypeKind::Union(members) => members
            .iter()
            .all(|&member| matches!(member.kind(db), TypeKind::None) || is_copy(db, member)),
        _ => false,
    }
}
