//! Loops over collections (language reference, "Values and references", "Loops over
//! collections"). `for v in values:` borrows `values` for the whole loop, so the body cannot
//! change it: no `append`, no assignment to it or its elements. When the elements are not a
//! copy type, `v` borrows each one for one pass, so it cannot be changed or moved either:
//!
//! ```text
//! error: `values` cannot be changed while a loop goes over it
//!  --> stats.npl:3:9
//!   |
//! 3 |         values.append(v * 2.0)
//!   |         ^^^^^^ changed here
//!  ::: stats.npl:2:14
//!   |
//! 2 |     for v in values:
//!   |              ------ the loop goes over `values` here
//!   = note: loop over the indices instead: `for i in range(len(values)):`
//! ```

use super::Body;
use database::Definition;
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};

/// A `for` loop the body being checked is inside.
pub(super) struct Loop {
    /// The collection it goes over, when that is a place such as `atoms.forces`; a loop over
    /// a temporary, such as `range(n)`, borrows nothing.
    collection: Option<NodeId>,
    /// Its loop variable, when that borrows an element that is not a copy type.
    element: Option<NodeId>,
}

impl Body<'_> {
    /// `for target in iterator:` and its `else`: the body runs any number of times, with the
    /// collection and, for elements that are not copy types, the element borrowed. A loop
    /// over `prange` is also checked as a parallel loop (the `parallel` module).
    pub(super) fn for_statement(&mut self, statement: NodeId) {
        let iterator = self.tree.child(statement, Role::Iterator);
        let element = match iterator {
            Some(iterator) => self.iteration(iterator),
            None => self.lowerer.unknown(),
        };
        let collection = iterator.filter(|&iterator| self.place_root(iterator).is_some());
        let parallel = iterator.is_some_and(|iterator| {
            self.is_prange(iterator) && self.enter_parallel(statement, iterator)
        });
        self.repeated(statement, iterator.unwrap_or(statement), |body| {
            let target = body.tree.child(statement, Role::Target);
            if let Some(target) = target {
                body.bind(target, element, None);
            }
            let borrowed = target
                .filter(|&target| body.tree.kind(target) == SyntaxKind::Name && body.moves(element))
                .and_then(|target| match body.lowerer.definition(target) {
                    Some(&Definition::Local(binding)) => Some(binding),
                    _ => None,
                });
            body.loops.push(Loop {
                collection,
                element: borrowed,
            });
            body.block(statement, Role::Body);
            body.loops.pop();
            body.block(statement, Role::Else);
        });
        if parallel {
            self.leave_parallel(statement);
        }
    }

    /// Reports the place `node` being changed or moved, as `verb` says, when an enclosing
    /// loop borrows it, and returns whether it did.
    pub(super) fn borrowed_by_loop(&mut self, node: NodeId, verb: &str) -> bool {
        let Some(root) = self.place_root(node) else {
            return false;
        };
        let binding = match self.lowerer.definition(root) {
            Some(&Definition::Local(binding)) => Some(binding),
            _ => None,
        };
        let found = self.loops.iter().rev().find_map(|found| {
            let collection = found.collection?;
            if binding.is_some() && found.element == binding {
                return Some((collection, true));
            }
            self.overlaps(node, collection)
                .then_some((collection, false))
        });
        let Some((collection, element)) = found else {
            return false;
        };
        let name = self.tree.text(root, self.text);
        let collection_name = self.tree.text(collection, self.text);
        let diagnostic = if element {
            Diagnostic::error(format!(
                "`{name}` borrows an element of `{collection_name}`, so it cannot be {verb}"
            ))
            .with_primary(self.tree.span(node), format!("{verb} here"))
            .with_note(if verb == "moved" {
                format!("to keep the element, keep a copy: `{name}.copy()`")
            } else {
                format!(
                    "work on a copy, `{name}.copy()`, or loop over the indices to change the element: `for i in range(len({collection_name})):`"
                )
            })
        } else {
            Diagnostic::error(format!(
                "`{collection_name}` cannot be {verb} while a loop goes over it"
            ))
            .with_primary(self.tree.span(node), format!("{verb} here"))
            .with_secondary(
                self.tree.span(collection),
                format!("the loop goes over `{collection_name}` here"),
            )
            .with_note(format!(
                "loop over the indices instead: `for i in range(len({collection_name})):`"
            ))
        };
        self.lowerer.diagnostics.push(diagnostic);
        true
    }
}
