//! Choosing elements of an array with another array (language reference, "Arrays", "Choosing
//! elements"; roadmap 3.13.1), as `NumPy` does:
//!
//! - `a[mask]`, with `mask` an `Array[bool, 1]`, is a new array of the elements of `a` where
//!   `mask` is `True`.
//! - `a[positions]`, with `positions` an `Array[i64, 1]`, is a new array of the elements of `a`
//!   at those positions, in that order.
//!
//! `a` is a rank-1 `Array` in this first version, and the result is one of the same type.

use super::Body;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of `base[indices]` when an index is an array: `base` itself, a new array, when
    /// `base` is a rank-1 `Array` and the one index a rank-1 `Array` of `bool`s or of `i64`s;
    /// an error for any other array index. `None` when no index is an array. The indices have
    /// been typed.
    pub(super) fn selection(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        indices: &[NodeId],
    ) -> Option<Type<'db>> {
        let by = indices
            .iter()
            .filter_map(|&index| self.recorded(index))
            .find(|&ty| self.is_any_array(ty))?;
        let rank_one = |ty: Type<'db>| match ty.kind(self.db) {
            TypeKind::Array { element, rank: 1 } => Some(*element),
            _ => None,
        };
        let chooses = rank_one(by).is_some_and(|element| {
            matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Bool | Primitive::Integer(Integer::I64))
            )
        });
        if indices.len() == 1 && rank_one(base).is_some() && chooses {
            return Some(base);
        }
        let message = format!(
            "`{}` cannot be indexed by `{}`",
            self.show(base),
            self.show(by)
        );
        Some(self.lowerer.error(
            node,
            &message,
            "not a choice of elements",
            Some(
                "a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`",
            ),
        ))
    }

    /// Whether the subscript `node` chooses elements with an array, which gives a new array
    /// rather than naming an element or a view.
    pub(super) fn is_selection(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Index)
                .and_then(|index| self.recorded(index))
                .is_some_and(|ty| self.is_any_array(ty))
    }

    /// The type the checker recorded for the expression `node`, once it has been typed.
    fn recorded(&self, node: NodeId) -> Option<Type<'db>> {
        self.typed.expressions.get(&types::Key(node)).copied()
    }

    /// Whether `ty` is an `Array` or a `Fixed` array.
    fn is_any_array(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Array { .. } | TypeKind::Fixed { .. }
        )
    }
}
