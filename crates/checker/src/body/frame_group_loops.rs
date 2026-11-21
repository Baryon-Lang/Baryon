//! The loop over a grouping (decision 0045's amendment of 2026-11-19, point 5; roadmap
//! 3.13.25): `for key, group in df.groupby("k"):` runs once for each group, in the order
//! of the keys.
//!
//! Each pass sees a pair. `key` is the group's key: the key column's value, or a tuple of
//! the key columns' values when the keys are written as a list, `df.groupby(["k"])`, a
//! tuple of one included, as in pandas. `group` is a new frame of the group's rows with
//! every column of `df`, the keys included. The loop borrows the pair, as it borrows an
//! element of a list: `group` is read inside the loop, and `group.copy()` keeps it.
//! `df.groupby(...)` is still not a value; this is a form of the `for` statement.

use super::Body;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// The frame `df` when `iterator` is the call `df.groupby(...)` and `df` has been typed
    /// as a frame: the collection a loop over its groups borrows, so the body cannot
    /// replace `df` while the loop reads it.
    pub(super) fn grouping_receiver(&self, iterator: NodeId) -> Option<NodeId> {
        let receiver = self.groupby_receiver(iterator)?;
        let frame = self.typed.expression(receiver)?;
        matches!(frame.kind(self.db), TypeKind::DataFrame { .. }).then_some(receiver)
    }

    /// The value `df` of the call `df.groupby(...)` at `iterator`, whatever its type; `None`
    /// when `iterator` is any other expression.
    fn groupby_receiver(&self, iterator: NodeId) -> Option<NodeId> {
        if self.tree.kind(iterator) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(iterator, Role::Function)?;
        let attribute = self.tree.child(function, Role::Attribute)?;
        let named = self.tree.kind(function) == SyntaxKind::Attribute
            && self.tree.text(attribute, self.text) == "groupby";
        named
            .then(|| self.tree.child(function, Role::Value))
            .flatten()
    }

    /// The element type of a `for` loop over `df.groupby(...)`: the pair of a group's key
    /// and the frame of its rows, which has the type of `df`. `None` when `iterator` is not
    /// such a call on a frame; the unknown type after a mistake in the keys is reported.
    pub(super) fn grouping_iteration(&mut self, iterator: NodeId) -> Option<Type<'db>> {
        let receiver = self.groupby_receiver(iterator)?;
        let base = self.expression(receiver, None);
        if !matches!(base.kind(self.db), TypeKind::DataFrame { .. }) {
            return None;
        }
        let Some((_, keys, true)) = self.grouping_keys(iterator, Some(iterator), base) else {
            return Some(self.lowerer.unknown());
        };
        let arguments = self.arguments(iterator);
        let by = arguments.positional.first().copied().or_else(|| {
            let named = |&&(name, _): &&(NodeId, NodeId)| self.tree.text(name, self.text) == "by";
            arguments.keywords.iter().find(named).map(|&(_, by)| by)
        });
        let listed = by.is_some_and(|by| self.tree.kind(by) == SyntaxKind::List);
        let key = match keys.as_slice() {
            [key] if !listed => key.element,
            keys => {
                let items = keys.iter().map(|key| key.element).collect();
                Type::new(self.db, TypeKind::Tuple(items))
            }
        };
        Some(Type::new(self.db, TypeKind::Tuple(vec![key, base])))
    }
}
