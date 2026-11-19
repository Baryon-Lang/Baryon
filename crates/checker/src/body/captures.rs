//! Captures in a `match` that borrow part of their subject (decision 0032): "a capture binds by
//! the subject's mode: a `Copy` field is copied, other fields are read-only borrows when the
//! subject is borrowed". In `match self.mask: case m:` inside a method that borrows `self`,
//! `m` is the array inside `self.mask`, not a copy of it, so it can be read in the case but not
//! moved or changed:
//!
//! ```text
//! error: `m` borrows part of `self.mask`, so it cannot be moved
//! ```
//!
//! Version 0 builds such a capture when the subject is a read-only parameter or a part of one,
//! such as `self.mask`. A capture of a part that is not a copy type from any other subject is
//! reported by lowering as not built yet (roadmap 1.9.50).

use super::Body;
use database::Definition;
use diagnostics::Diagnostic;
use syntax::NodeId;
use types::{Mode, Type};

impl<'db> Body<'db> {
    /// Records the capture `name`, of type `ty`, as a borrow of the `match` subject when `ty`
    /// is not a copy type and the subject is part of a read-only parameter.
    pub(super) fn borrowing_capture(&mut self, name: NodeId, ty: Type<'db>) {
        let Some(subject) = self.subject else {
            return;
        };
        if !self.moves(ty) || !self.borrows_parameter(subject) {
            return;
        }
        if let Some(&Definition::Local(binding)) = self.lowerer.definition(name) {
            self.captures.insert(binding, subject);
        }
    }

    /// Whether the place `node`, such as `self.mask`, is a read-only parameter or a part of
    /// one.
    pub(super) fn borrows_parameter(&self, node: NodeId) -> bool {
        let root = self.place_root(node);
        let binding = root.and_then(|root| match self.lowerer.definition(root) {
            Some(&Definition::Local(binding)) => Some(binding),
            _ => None,
        });
        binding.is_some_and(|binding| self.modes.get(&binding) == Some(&Mode::Borrow))
    }

    /// Reports the place `node` being changed or moved, as `verb` says, when it is a capture
    /// that borrows part of its `match` subject, and returns whether it did.
    pub(super) fn borrowed_by_capture(&mut self, node: NodeId, verb: &str) -> bool {
        let Some(root) = self.place_root(node) else {
            return false;
        };
        let Some(&Definition::Local(binding)) = self.lowerer.definition(root) else {
            return false;
        };
        let Some(&subject) = self.captures.get(&binding) else {
            return false;
        };
        let name = self.tree.text(root, self.text);
        let subject = self.tree.text(subject, self.text);
        let diagnostic = Diagnostic::error(format!(
            "`{name}` borrows part of `{subject}`, so it cannot be {verb}"
        ))
        .with_primary(self.tree.span(node), format!("{verb} here"))
        .with_note(format!(
            "a capture borrows from a borrowed subject (decision 0032); work on a copy: \
             `{name}.copy()`"
        ));
        self.lowerer.diagnostics.push(diagnostic);
        true
    }
}
