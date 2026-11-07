//! Definite assignment (language reference, "Types", "Scope and definite assignment"): a
//! variable may be read only where every path that reaches the read has assigned it, and a
//! function that returns a value must not reach its end.
//!
//! Variables have function scope, as in Python, so one assigned inside an `if` is visible after
//! it, but only usable there if every branch assigned it. The `flow` module tracks which
//! variables are assigned on every path, and where a path that skips one starts; the error
//! points at both:
//!
//! ```text
//! error: `label` may not be assigned here
//!   |
//! 2 |     if energy < 0.0:
//!   |        ------------ not assigned when this is false
//! ...
//! 4 |     return label
//!   |            ^^^^^ read here
//! ```

use super::Body;
use diagnostics::Diagnostic;
use std::collections::BTreeSet;
use syntax::{NodeId, Role, SyntaxKind};
use types::TypeKind;

impl Body<'_> {
    /// The variables that statements bind: assignment and loop targets, `with` and `except`
    /// names, and `match` captures, each by the `Name` node of its first binding. Only these
    /// are checked for definite assignment; a comprehension's variable is bound by the
    /// comprehension itself, before any read.
    pub(super) fn statement_bindings(&self, node: NodeId, bound: &mut BTreeSet<NodeId>) {
        for child in self.tree.children(node) {
            let kind = self.tree.kind(child);
            match self.tree.node(child).role() {
                Role::Target => self.target_names(child, bound),
                Role::Pattern => self.pattern_names(child, bound),
                Role::Name if self.tree.kind(node) == SyntaxKind::ExceptHandler => {
                    self.target_names(child, bound);
                }
                _ if kind.is_expression() => {}
                _ => self.statement_bindings(child, bound),
            }
        }
    }

    /// The variables an assignment target binds into `bound`: its names, through tuples and
    /// lists, but not the object of `p.x = ...` or `a[i] = ...`.
    fn target_names(&self, target: NodeId, bound: &mut BTreeSet<NodeId>) {
        match self.tree.kind(target) {
            SyntaxKind::Name => {
                if let Some(&database::Definition::Local(binding)) = self.lowerer.definition(target)
                {
                    bound.insert(binding);
                }
            }
            SyntaxKind::Tuple | SyntaxKind::List | SyntaxKind::Starred => {
                for item in self.tree.children(target) {
                    self.target_names(item, bound);
                }
            }
            _ => {}
        }
    }

    /// The variables a `match` pattern captures into `bound`.
    pub(super) fn pattern_names(&self, pattern: NodeId, bound: &mut BTreeSet<NodeId>) {
        for child in self.tree.children(pattern) {
            match self.tree.node(child).role() {
                Role::Name if self.tree.kind(pattern) != SyntaxKind::PatternKeyword => {
                    self.target_names(child, bound);
                }
                Role::Pattern | Role::Item => self.pattern_names(child, bound),
                _ => {}
            }
        }
    }

    /// Reports a read, at `node`, of the variable `binding` on a path that has not assigned
    /// it. Each variable is reported once.
    pub(super) fn read(&mut self, node: NodeId, binding: NodeId) {
        if !self.bound.contains(&binding) || self.state.assigned.contains(&binding) {
            return;
        }
        self.state.assigned.insert(binding);
        let name = self.tree.text(node, self.text);
        let mut diagnostic = match self.state.skipped.get(&binding) {
            Some(skip) => Diagnostic::error(format!("`{name}` may not be assigned here"))
                .with_secondary(self.tree.span(skip.node), skip.label)
                .with_note("give it a value before the branch or loop, or assign it on every path that reaches this read"),
            None => Diagnostic::error(format!("`{name}` is read before it is assigned")),
        };
        diagnostic = diagnostic.with_primary(self.tree.span(node), "read here");
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Reports a function that returns a value, `name`, whose body can reach its end, where it
    /// would return nothing.
    pub(super) fn missing_return(&mut self, name: NodeId) {
        let returns = match self.signature.returns.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => self.signature.returns,
        };
        if matches!(returns.kind(self.db), TypeKind::None | TypeKind::Unknown) {
            return;
        }
        let function = self.tree.text(name, self.text);
        let message = format!("`{function}` can reach its end without returning a value");
        let label = format!("returns `{}`", self.show(returns));
        self.lowerer.error(
            name,
            &message,
            &label,
            Some("add a `return` at the end, or make every branch end in `return` or `raise`"),
        );
    }
}
