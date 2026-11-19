//! Choosing elements of an array with another array (language reference, "Arrays", "Choosing
//! elements"; roadmap 3.13.1), as `NumPy` does:
//!
//! - `a[mask]`, with `mask` an `Array[bool, 1]`, is a new array of the elements of `a` where
//!   `mask` is `True`.
//! - `a[positions]`, with `positions` an `Array[i64, 1]`, is a new array of the elements of `a`
//!   at those positions, in that order.
//! - `a[[0, 2]]`, with a `list` of `bool`s or of `i64`s, chooses as the array of its items
//!   does.
//!
//! `a` is a rank-1 `Array`, an array of `str` or an array with missing values, and the
//! result is one of the same type. A mask may hold `None`, as `a[bids > 1.0]` does when
//! `bids` has missing values: an element whose mask is `None` is not chosen, as in pandas.

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
        let recorded = |index: &NodeId| self.recorded(*index);
        let rank_one = |ty: Type<'db>| match ty.kind(self.db) {
            TypeKind::Array { element, rank: 1 } => Some(*element),
            _ => None,
        };
        // A mask that may hold `None` chooses where it is `True`, as in pandas.
        let gaps = indices
            .iter()
            .filter_map(recorded)
            .find_map(|ty| match ty.kind(self.db) {
                TypeKind::OptionalArray(element) => Some(*element),
                _ => None,
            });
        let chosen_from =
            rank_one(base).is_some() || self.is_text_array(base) || self.is_optional_array(base);
        if let Some(element) = gaps {
            let boolean = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
            if indices.len() == 1 && boolean && chosen_from {
                return Some(base);
            }
            let what = "choosing elements with it, other than of an array of one dimension by \
                        a mask,";
            return Some(self.optional_array_unbuilt(node, what));
        }
        // A list chooses as the array of its items does, as in NumPy: `a[[0, 2]]`.
        let listed = |ty: Type<'db>| match ty.kind(self.db) {
            TypeKind::List(element) => Some(*element),
            _ => None,
        };
        let by = indices
            .iter()
            .filter_map(recorded)
            .find(|&ty| self.is_any_array(ty) || (chosen_from && listed(ty).is_some()))?;
        let chooses = rank_one(by).or(listed(by)).is_some_and(|element| {
            matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Bool | Primitive::Integer(Integer::I64))
            )
        });
        if indices.len() == 1 && chosen_from && chooses {
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
    /// rather than naming an element or a view. A slice of a text array or of an array with
    /// missing values is one too: its elements are copied into a new array.
    pub(super) fn is_selection(&self, node: NodeId) -> bool {
        if self.tree.kind(node) != SyntaxKind::Subscript {
            return false;
        }
        let index = self.tree.child(node, Role::Index);
        let sliced = index.is_some_and(|index| self.tree.kind(index) == SyntaxKind::Slice);
        let copied = self
            .tree
            .child(node, Role::Value)
            .and_then(|value| self.recorded(value))
            .is_some_and(|ty| self.is_text_array(ty) || self.is_optional_array(ty));
        (sliced && copied)
            || index
                .and_then(|index| self.recorded(index))
                .is_some_and(|ty| {
                    let listed = matches!(ty.kind(self.db), TypeKind::List(_));
                    self.is_any_array(ty) || self.is_optional_array(ty) || listed
                })
    }

    /// The type the checker recorded for the expression `node`, once it has been typed.
    fn recorded(&self, node: NodeId) -> Option<Type<'db>> {
        self.typed.expressions.get(&types::Key(node)).copied()
    }

    /// Whether `ty` is an `Array` or a `Fixed` array.
    pub(super) fn is_any_array(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Array { .. } | TypeKind::Fixed { .. }
        )
    }
}
