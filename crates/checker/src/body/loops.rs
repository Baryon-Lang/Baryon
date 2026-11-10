//! Loops over collections (language reference, "Values and references", "Loops over
//! collections"). `for v in values:` borrows `values` for the whole loop, so the body cannot
//! change it: no `append`, no assignment to it or its elements. When the elements are not a
//! copy type, `v` borrows each one for one pass, so it cannot be changed or moved either:
//!
//! ```text
//! error: `values` cannot be changed while a loop goes over it
//!  --> stats.byn:3:9
//!   |
//! 3 |         values.append(v * 2.0)
//!   |         ^^^^^^ changed here
//!  ::: stats.byn:2:14
//!   |
//! 2 |     for v in values:
//!   |              ------ the loop goes over `values` here
//!   = note: loop over the indices instead: `for i in range(len(values)):`
//! ```

use super::Body;
use database::Definition;
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// A `for` loop the body being checked is inside.
pub(super) struct Loop {
    /// The collection it goes over, when that is a place such as `atoms.forces`; a loop over
    /// a temporary, such as `range(n)`, borrows nothing.
    collection: Option<NodeId>,
    /// The loop variables that borrow an element, or a part of one, that is not a copy type:
    /// `v` in `for v in names:`, `name` in `for name, mass in species:`.
    elements: Vec<NodeId>,
}

impl<'db> Body<'db> {
    /// `for target in iterator:` and its `else`: the body runs any number of times, with the
    /// collection and, for elements that are not copy types, the element borrowed. A loop
    /// over `prange` or `simd` is also checked as a parallel or vector loop (the `parallel`
    /// module).
    pub(super) fn for_statement(&mut self, statement: NodeId) {
        let iterator = self.tree.child(statement, Role::Iterator);
        let element = match iterator {
            Some(iterator) => self.iteration(iterator),
            None => self.lowerer.unknown(),
        };
        let collection = iterator
            .map(|iterator| self.dict_view_receiver(iterator).unwrap_or(iterator))
            .filter(|&collection| self.place_root(collection).is_some());
        let parallel = iterator.is_some_and(|iterator| {
            self.loop_kind(iterator)
                .is_some_and(|kind| self.enter_parallel(statement, iterator, kind))
        });
        self.repeated(statement, iterator.unwrap_or(statement), |body| {
            let target = body.tree.child(statement, Role::Target);
            if let Some(target) = target {
                body.bind(target, element, None);
            }
            let borrowed = body.borrowed_elements(target, element);
            body.loops.push(Loop {
                collection,
                elements: borrowed.iter().map(|&(binding, _)| binding).collect(),
            });
            body.block(statement, Role::Body);
            body.loops.pop();
            // The borrow ends with the loop (decision card, 2026-11-10): the variable holds
            // no element after it, so a read before a new assignment is an error, the `else`
            // block included.
            for (binding, name) in borrowed {
                body.state.moved.insert(binding, name);
                body.loop_ends.insert(name);
            }
            body.block(statement, Role::Else);
        });
        if parallel {
            self.leave_parallel(statement);
        }
    }

    /// The list comprehension `node`, `[value for target in iterator if condition ...]`, and
    /// the type of its elements. Each `for` clause is checked as a `for` statement over its
    /// collection: the collection and a borrowed element stay borrowed for the clauses and the
    /// value inside it, and the passes repeat. The value is moved into the new list, so an
    /// element that is borrowed, `[w for w in words]` over a `list[str]`, is an error with the
    /// same fix as the loop's, `w.copy()`; otherwise two lists would own one string.
    pub(super) fn list_comprehension(&mut self, node: NodeId) -> Type<'db> {
        let clauses: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        self.comprehension_clauses(&clauses, &mut |body| {
            let Some(value) = body.tree.child(node, Role::Value) else {
                return body.lowerer.unknown();
            };
            let ty = body.expression(value, None);
            body.consume(value, ty);
            ty
        })
    }

    /// The `for` clauses `clauses` of a comprehension, the first outermost, then `inner`, which
    /// checks what the comprehension builds from each pass: a list's value, or a dict's key and
    /// value. Returns what `inner` returns.
    pub(super) fn comprehension_clauses(
        &mut self,
        clauses: &[NodeId],
        inner: &mut dyn FnMut(&mut Self) -> Type<'db>,
    ) -> Type<'db> {
        let Some((&clause, rest)) = clauses.split_first() else {
            return inner(self);
        };
        let iterator = self.tree.child(clause, Role::Iterator);
        let element = match iterator {
            Some(iterator) => self.iteration(iterator),
            None => self.lowerer.unknown(),
        };
        if let Some(iterator) = iterator.filter(|&iterator| self.is_prange(iterator)) {
            self.prange_in_comprehension(iterator);
        }
        let collection = iterator
            .map(|iterator| self.dict_view_receiver(iterator).unwrap_or(iterator))
            .filter(|&collection| self.place_root(collection).is_some());
        let mut item = self.lowerer.unknown();
        self.repeated(clause, iterator.unwrap_or(clause), |body| {
            let target = body.tree.child(clause, Role::Target);
            if let Some(target) = target {
                body.bind(target, element, None);
            }
            body.loops.push(Loop {
                collection,
                elements: body
                    .borrowed_elements(target, element)
                    .into_iter()
                    .map(|(binding, _)| binding)
                    .collect(),
            });
            let conditions: Vec<NodeId> =
                body.tree.children_with(clause, Role::Condition).collect();
            for condition in conditions {
                body.condition(Some(condition));
            }
            item = body.comprehension_clauses(rest, inner);
            body.loops.pop();
        });
        item
    }

    /// Reports `prange` as the iterator of a comprehension clause (Collins, 2026-11-10): a
    /// parallel loop is a `for` statement, where the checker's sharing rules apply, so the error
    /// shows that form. A parallel map that fills the list in order could be added later without
    /// breaking code (roadmap idea I.41).
    fn prange_in_comprehension(&mut self, iterator: NodeId) {
        self.lowerer.error(
            iterator,
            "`prange` cannot be used in a comprehension",
            "a parallel loop must be a `for` statement",
            Some(
                "make the result first, then fill it in parallel: `for i in prange(n): out[i] = f(i)`",
            ),
        );
    }

    /// The variables `target` binds that borrow their part of an element of type `element`,
    /// rather than copy it, as lowering binds them: a name whose type is not a copy type, and,
    /// in a tuple target such as `name, mass`, each part the same way. Each comes with the
    /// `Name` node in the target that binds it.
    fn borrowed_elements(
        &self,
        target: Option<NodeId>,
        element: Type<'db>,
    ) -> Vec<(NodeId, NodeId)> {
        let mut borrowed = Vec::new();
        if let Some(target) = target {
            self.borrowed_parts(target, element, &mut borrowed);
        }
        borrowed
    }

    /// Appends to `borrowed` the variables the target `target` binds that borrow their part, of
    /// type `ty`, each with its `Name` node in the target.
    fn borrowed_parts(&self, target: NodeId, ty: Type<'db>, borrowed: &mut Vec<(NodeId, NodeId)>) {
        if !self.moves(ty) {
            return;
        }
        match (self.tree.kind(target), ty.kind(self.db)) {
            (SyntaxKind::Name, _) => {
                if let Some(&Definition::Local(binding)) = self.lowerer.definition(target) {
                    borrowed.push((binding, target));
                }
            }
            (SyntaxKind::Tuple, TypeKind::Tuple(items)) => {
                let parts: Vec<NodeId> = self.tree.children_with(target, Role::Item).collect();
                for (part, &item) in parts.into_iter().zip(items) {
                    self.borrowed_parts(part, item, borrowed);
                }
            }
            _ => {}
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
            if binding.is_some_and(|binding| found.elements.contains(&binding)) {
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
            .with_note(if self.is_dict(collection) {
                format!("loop over a copy instead: `for k in {collection_name}.copy():`")
            } else {
                format!("loop over the indices instead: `for i in range(len({collection_name})):`")
            })
        };
        self.lowerer.diagnostics.push(diagnostic);
        true
    }

    /// Whether the expression `node` was typed as a `dict`, whose loops have no indices.
    fn is_dict(&self, node: NodeId) -> bool {
        matches!(
            self.typed.expression(node).map(|ty| ty.kind(self.db)),
            Some(TypeKind::Dict { .. })
        )
    }
}
