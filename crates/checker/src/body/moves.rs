//! Moves (language reference, "Values and references", "Moves" and "Parameters"): a value that
//! is not a copy type moves when it is assigned, returned, passed as `Owned`, or placed in a
//! struct, tuple or list being built. After a move, the old owner holds nothing until it is
//! assigned again, and reading it is an error that points at the move:
//!
//! ```text
//! error: `a` was moved
//!    |
//! 11 |     b = a
//!    |         - `a` moved here
//! 12 |     total = a.sum()
//!    |             ^ read after the move
//! ```
//!
//! Only a local variable or an `Owned` parameter can be moved. A read-only or `InOut` parameter
//! belongs to the caller, so moving it, including returning it, is an error that suggests a
//! copy. Moving a field out moves the whole struct; moving an element out of an array or a list
//! is an error, and so is keeping a slice: a slice is a view of its array, a borrow, which can
//! be passed to a call or assigned to but not stored.
//!
//! A variable moved on some path counts as moved after the paths join, and one moved inside a
//! loop, and not assigned again before the loop's next pass, is reported at the move.

use super::Body;
use super::flow::State;
use crate::traits::{Bound, Trait};
use database::Definition;
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Mode, Type, TypeKind};

impl<'db> Body<'db> {
    /// Whether a value of `ty` moves when it is used as a value, rather than being copied.
    pub(super) fn moves(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Str | TypeKind::List(_) | TypeKind::Array { .. } | TypeKind::Standard(_) => {
                true
            }
            TypeKind::Tuple(items) => items.iter().any(|&item| self.moves(item)),
            TypeKind::Optional(value) => self.moves(*value),
            TypeKind::Struct(_) => !crate::forms::is_copy(self.db, ty),
            // A bound that is not a trait has had its error; its values count as copies.
            TypeKind::Parameter { file, name } => !matches!(
                crate::traits::written_bound(self.db, self.lowerer.package, *file, *name),
                Some(Bound::Other | Bound::Trait(Trait::Copy | Trait::Number | Trait::Float))
            ),
            _ => false,
        }
    }

    /// Moves the value of the expression `node`, of type `ty`, when it is not a copy type:
    /// records its variable as moved, or reports a value that cannot be moved.
    pub(super) fn consume(&mut self, node: NodeId, ty: Type<'db>) {
        if !self.moves(ty) {
            return;
        }
        match self.tree.kind(node) {
            SyntaxKind::Name | SyntaxKind::Attribute if self.borrowed_by_loop(node, "moved") => {}
            SyntaxKind::Name => self.move_variable(node, node),
            SyntaxKind::Attribute => {
                // Moving a field out moves the whole struct: one with a hole cannot be used.
                if let Some(root) = self.place_root(node) {
                    self.move_variable(root, node);
                }
            }
            SyntaxKind::Subscript if self.is_view(node) => {
                // A view borrows the array, and a borrow cannot be stored (reference,
                // "Slices").
                let hint = format!(
                    "a slice can be passed to a call or assigned to, but to keep it, copy it: `{}.copy()`",
                    self.tree.text(node, self.text)
                );
                self.lowerer.error(
                    node,
                    "a slice of an array cannot be kept",
                    "a view of the array's memory",
                    Some(&hint),
                );
            }
            SyntaxKind::Subscript => {
                let hint = format!(
                    "move a copy instead: `{}.copy()`",
                    self.tree.text(node, self.text)
                );
                self.lowerer.error(
                    node,
                    "a value cannot be moved out of an element",
                    "moved out of an element",
                    Some(&hint),
                );
            }
            _ => {}
        }
    }

    /// Whether the subscript `node` is a slice, `a[1:n]` or `grid[:, 0]`: a view of its
    /// array rather than an element.
    fn is_view(&self, node: NodeId) -> bool {
        let Some(index) = self.tree.child(node, Role::Index) else {
            return false;
        };
        let slice = |part: NodeId| self.tree.kind(part) == SyntaxKind::Slice;
        slice(index)
            || (self.tree.kind(index) == SyntaxKind::Tuple
                && self.tree.children_with(index, Role::Item).any(slice))
    }

    /// The variable a place such as `atoms.forces[i]` starts at: its `Name` node, or `None`
    /// when it starts at something else, such as a call.
    pub(super) fn place_root(&self, node: NodeId) -> Option<NodeId> {
        match self.tree.kind(node) {
            SyntaxKind::Name => Some(node),
            SyntaxKind::Attribute | SyntaxKind::Subscript => {
                self.place_root(self.tree.child(node, Role::Value)?)
            }
            _ => None,
        }
    }

    /// Moves the variable named at `name`, at the move site `site`: all of it, or the field of
    /// it that `site` names.
    fn move_variable(&mut self, name: NodeId, site: NodeId) {
        let Some(&Definition::Local(binding)) = self.lowerer.definition(name) else {
            return;
        };
        let text = self.tree.text(name, self.text);
        match self.modes.get(&binding) {
            Some(Mode::Borrow) => {
                let message =
                    format!("`{text}` is borrowed from the caller, so it cannot be moved");
                let hint = format!(
                    "move a copy instead: `{}.copy()`, or take the parameter as `Owned[...]`",
                    self.tree.text(site, self.text)
                );
                self.lowerer
                    .error(site, &message, "moved here", Some(&hint));
            }
            Some(Mode::InOut) => {
                let message = format!(
                    "`{text}` is borrowed from the caller as `InOut`, so it cannot be moved"
                );
                let hint = format!(
                    "move a copy instead: `{}.copy()`, or assign it a new value in place",
                    self.tree.text(site, self.text)
                );
                self.lowerer
                    .error(site, &message, "moved here", Some(&hint));
            }
            Some(Mode::Owned) | None => {
                self.state.moved.insert(binding, site);
            }
        }
    }

    /// Reports a read, at `node`, of the variable `binding` after it was moved, and forgets the
    /// move so that each move is reported once.
    pub(super) fn read_moved(&mut self, node: NodeId, binding: NodeId) {
        let Some(site) = self.state.moved.remove(&binding) else {
            return;
        };
        let name = self.tree.text(node, self.text);
        // A generator has no copy: two copies would draw the same numbers (decision 0028).
        let generator = self
            .locals
            .get(&binding)
            .is_some_and(|ty| matches!(ty.kind(self.db), TypeKind::Standard(_)));
        let advice = if generator {
            "a generator cannot be copied, so no two draw the same numbers; make another with its \
             own `stream`"
                .to_owned()
        } else {
            format!(
                "to keep both, move a copy: `{}.copy()`",
                self.tree.text(site, self.text)
            )
        };
        let diagnostic = Diagnostic::error(format!("`{name}` was moved"))
            .with_primary(self.tree.span(node), "read after the move")
            .with_secondary(self.tree.span(site), format!("`{name}` moved here"))
            .with_note(advice);
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Reports each variable the loop `statement` moved and did not assign anywhere in it,
    /// given what was known before the loop, `entry`, and at the end of its body, `end`: the
    /// next pass would read a moved value.
    pub(super) fn moved_in_loop(
        &mut self,
        statement: NodeId,
        entry: &State<'db>,
        end: &State<'db>,
    ) {
        for (&binding, &site) in &end.moved {
            if entry.moved.contains_key(&binding)
                || !entry.assigned.contains(&binding)
                || self.assigns(statement, binding)
            {
                continue;
            }
            let name = self.tree.text(binding, self.text);
            let message = format!("`{name}` is moved in one pass of the loop and read in the next");
            self.lowerer.error(
                site,
                &message,
                "moved here, inside a loop",
                Some(
                    "move a copy, `.copy()`, or assign the variable a new value later in the loop",
                ),
            );
        }
    }

    /// Whether any statement inside `node` assigns the variable `binding`.
    fn assigns(&self, node: NodeId, binding: NodeId) -> bool {
        let mut bound = std::collections::BTreeSet::new();
        self.statement_bindings(node, &mut bound);
        bound.contains(&binding)
    }
}
