//! Lending through a conditional expression (language reference, "Values and references",
//! "Moves"). A conditional of a type that moves, such as a `str`, is computed
//! into a temporary that the statement frees. When the value is kept, by an assignment, a
//! return or an `Owned` argument, a branch that names a variable moves out of it, and the
//! `moves` module checks that move. When the value is only lent, to a read-only parameter, a
//! method, an operator or `print`, a branch that names a variable would be freed with the
//! temporary while the variable still holds it. Until lowering can lend the branch that runs
//! in place, that is an error (roadmap 1.9.22):
//!
//! ```text
//! error: a conditional expression cannot lend `name` yet
//!   --> main.byn:10:40
//!    |
//! 10 |     return size(name[0:1] if long else name)
//!    |                                        ^^^^ freed with the conditional's value
//!    = note: lend a copy, `name.copy()`, or choose with an `if` statement and pass `name` itself
//! ```
//!
//! A branch that builds a new value, `name[0:1]`, `name + "s"` or a literal, lends nothing
//! and is never reported.

use super::Body;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Key, TypeKind};

impl Body<'_> {
    /// Reports each conditional expression of a type that moves that was only lent, not
    /// kept, and has a branch naming a variable. Runs once the body is checked, when every
    /// keeping use has been seen. A conditional inside a branch of another is judged with it.
    pub(super) fn lent_conditionals(&mut self) {
        let tree = self.tree;
        let nested: Vec<NodeId> = self
            .conditionals
            .iter()
            .flat_map(|&node| {
                [Role::Value, Role::Else]
                    .into_iter()
                    .filter_map(move |role| tree.child(node, role))
            })
            .collect();
        let lent: Vec<NodeId> = self
            .conditionals
            .iter()
            .copied()
            .filter(|node| !self.kept.contains(node) && !nested.contains(node))
            .collect();
        for node in lent {
            let Some(branch) = self.lending_branch(node) else {
                continue;
            };
            let text = self.tree.text(branch, self.text);
            let message = format!("a conditional expression cannot lend `{text}` yet");
            let hint = format!(
                "lend a copy, `{text}.copy()`, or choose with an `if` statement and pass `{text}` itself"
            );
            self.lowerer.error(
                branch,
                &message,
                "freed with the conditional's value",
                Some(&hint),
            );
        }
    }

    /// The first branch of the conditional `node`, nested conditionals included, that names a
    /// variable's value rather than building a new one.
    fn lending_branch(&self, node: NodeId) -> Option<NodeId> {
        [Role::Value, Role::Else]
            .into_iter()
            .filter_map(|role| self.tree.child(node, role))
            .find_map(|branch| match self.tree.kind(branch) {
                SyntaxKind::Conditional => self.lending_branch(branch),
                _ if self.names_variable(branch) => Some(branch),
                _ => None,
            })
    }

    /// Whether `node` is a place that starts at a local variable or parameter, `name`,
    /// `atoms.label` or `names[0]`, and reading it gives that place's value rather than a new
    /// one. A slice of a `str` or list, a mask or positions, a frame's row, a part of a `Path`
    /// and `os.environ[...]` are new values, as the `moves` module counts them.
    fn names_variable(&self, node: NodeId) -> bool {
        let Some(root) = self.place_root(node) else {
            return false;
        };
        if !matches!(self.lowerer.definition(root), Some(Definition::Local(_))) {
            return false;
        }
        match self.tree.kind(node) {
            SyntaxKind::Name => true,
            SyntaxKind::Attribute => !self.is_path_part(node),
            SyntaxKind::Subscript => {
                let new_slice = self.is_view(node)
                    && self.typed.expressions.get(&Key(node)).is_some_and(|ty| {
                        matches!(ty.kind(self.db), TypeKind::List(_) | TypeKind::Str)
                    });
                !(new_slice
                    || self.is_selection(node)
                    || self.is_frame_row(node)
                    || self.typed.calls.contains_key(&Key(node)))
            }
            _ => false,
        }
    }
}
