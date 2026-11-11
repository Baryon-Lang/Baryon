//! Reductions along one axis (roadmap 3.9): `a.sum(axis=k)`, `a.min(axis=k)` and
//! `a.max(axis=k)` on an `Array` or a `Fixed` array, with `NumPy`'s names and results.
//!
//! The axis is a constant whole number, written as a literal, a negated literal or the name of a
//! `Final` constant, since the result's rank depends on it. As in `NumPy`, `-1` is the last axis
//! and an axis outside the array's rank is an error. The result drops that axis: an
//! `Array[T, R - 1]`, a `Fixed` array without that length, or one `T` when the array has rank 1,
//! the same as `a.sum()`.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Operator, SyntaxKind};
use types::{Integer, Primitive, Shape, Type, TypeKind, Value};

/// What a reduction along an axis takes, for its messages.
const AXIS: &str = "the axis is a constant whole number, such as `a.sum(axis=0)` or \
                    `a.max(axis=-1)` for the last";

impl<'db> Body<'db> {
    /// The axis argument of `a.sum(...)`, `a.min(...)` or `a.max(...)`: one positional
    /// argument or one `axis=` keyword. `None` when there is none, or when the arguments are
    /// anything else, which [`axis_reduction`](Self::axis_reduction) then reports.
    pub(super) fn axis_argument(&self, arguments: &Arguments) -> Option<NodeId> {
        match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([axis], []) => Some(*axis),
            ([], [(name, axis)]) if self.tree.text(*name, self.text) == "axis" => Some(*axis),
            _ => None,
        }
    }

    /// `a.sum(axis=k)`, `a.min(axis=k)` or `a.max(axis=k)` on `base`, an array with `element`
    /// type, called `name`: the array of reductions along axis `k`. `axis` is the argument
    /// [`axis_argument`](Self::axis_argument) found, or `None` for other arguments.
    pub(super) fn axis_reduction(
        &mut self,
        node: NodeId,
        (base, element): (Type<'db>, Type<'db>),
        name: &str,
        axis: Option<NodeId>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let Some(axis) = axis else {
            self.untyped(arguments);
            let message = format!("`{name}` takes no arguments or one axis");
            return self
                .lowerer
                .error(node, &message, "wrong arguments", Some(AXIS));
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let found = self.expression(axis, Some(int));
        self.expect(axis, int, found, "an axis is");
        let written = matches!(self.tree.kind(axis), SyntaxKind::Integer | SyntaxKind::Name)
            || (self.tree.kind(axis) == SyntaxKind::UnaryOperation
                && self.tree.node(axis).operator() == Operator::Negate);
        if !written {
            return self.lowerer.error(
                axis,
                "the axis must be written as a constant",
                "not a constant",
                Some(AXIS),
            );
        }
        let Some(value) = self.lowerer.evaluate(axis) else {
            return self.lowerer.unknown();
        };
        let lengths = match base.kind(self.db) {
            TypeKind::Fixed { shape, .. } => Some(shape.lengths().to_vec()),
            _ => None,
        };
        let rank = match base.kind(self.db) {
            TypeKind::Array { rank, .. } => i64::from(*rank),
            TypeKind::Fixed { shape, .. } => i64::try_from(shape.rank()).unwrap_or(i64::MAX),
            _ => return self.lowerer.unknown(),
        };
        let Value::Integer(given) = value else {
            return self.lowerer.unknown();
        };
        let Some(position) = Some(if given < 0 { given + rank } else { given })
            .filter(|position| (0..rank).contains(position))
            .and_then(|position| usize::try_from(position).ok())
        else {
            let message = format!(
                "axis {given} is out of range for `{}`, which has {rank} {}",
                self.show(base),
                if rank == 1 { "axis" } else { "axes" }
            );
            let hint = format!(
                "an array of rank {rank} has axes 0 to {}, or -{rank} to -1 counted from the last",
                rank - 1
            );
            return self
                .lowerer
                .error(axis, &message, "no such axis", Some(&hint));
        };
        if rank == 1 {
            return element;
        }
        let kind = match lengths {
            Some(mut lengths) => {
                lengths.remove(position);
                match Shape::new(lengths) {
                    Some(shape) => TypeKind::Fixed { element, shape },
                    None => return self.lowerer.unknown(),
                }
            }
            None => TypeKind::Array {
                element,
                rank: u32::try_from(rank - 1).unwrap_or(1),
            },
        };
        Type::new(self.db, kind)
    }
}
