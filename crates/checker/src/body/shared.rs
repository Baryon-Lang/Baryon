//! The rules for shared variables written in a `prange` or `simd` body (language reference,
//! "Parallel and vector loops", rules 2 to 4), checked over the uses the `parallel` module
//! collected:
//!
//! - An element of a shared array is written only where every access to that array indexes one
//!   fixed dimension with exactly the loop variable, so each element belongs to one iteration.
//! - A shared scalar is written only as one kind of reduction, and read nowhere else in the
//!   body, so each thread can keep a private partial result.

use super::Body;
use super::parallel::{Access, Parallel, Path, Reduction, Usage, overlap};
use database::Definition;
use diagnostics::Diagnostic;
use std::collections::BTreeSet;
use syntax::{NodeId, Role, SyntaxKind};

impl Body<'_> {
    /// Checks every shared array written at an index: each access to it must index the same
    /// dimension with the loop variable. Returns the arrays, as variable and path, so the
    /// reduction check leaves them alone.
    pub(super) fn elements(
        &mut self,
        parallel: &Parallel,
        accesses: &[Access],
    ) -> Vec<(NodeId, Path)> {
        let mut arrays: Vec<(NodeId, Path)> = Vec::new();
        for access in accesses {
            if access.usage == Usage::Read {
                continue;
            }
            let Some(first) = access.path.iter().position(Option::is_none) else {
                continue;
            };
            let array = (access.binding, access.path[..first].to_vec());
            if !arrays.contains(&array) {
                arrays.push(array);
            }
        }
        for (binding, array) in &arrays {
            let members: Vec<&Access> = accesses
                .iter()
                .filter(|access| access.binding == *binding && overlap(&access.path, array))
                .collect();
            let writes = members.iter().filter(|access| access.usage != Usage::Read);
            let adds = writes
                .clone()
                .all(|access| access.usage == Usage::Reduce(Reduction::Sum));
            let name = writes
                .clone()
                .find_map(|access| self.first_subscript(access.node))
                .and_then(|subscript| self.tree.child(subscript, Role::Value))
                .map(|value| self.tree.text(value, self.text).to_owned())
                .unwrap_or_default();
            let mut dimensions: Option<BTreeSet<usize>> = None;
            for access in members {
                let indexed = access.path.get(array.len()).is_some_and(Option::is_none);
                let subscript = self.first_subscript(access.node).filter(|_| indexed);
                let Some(subscript) = subscript else {
                    self.used_whole(parallel, access, &name);
                    continue;
                };
                let found = self.loop_dimensions(subscript, parallel);
                let next: BTreeSet<usize> = match &dimensions {
                    Some(known) => known.intersection(&found).copied().collect(),
                    None => found,
                };
                if next.is_empty() {
                    self.wrong_index(parallel, access, subscript, &name, adds);
                } else {
                    dimensions = Some(next);
                }
            }
        }
        arrays
    }

    /// The first `[...]` of the place `node`: the innermost subscript in the tree.
    fn first_subscript(&self, node: NodeId) -> Option<NodeId> {
        let mut found = None;
        let mut current = node;
        while matches!(
            self.tree.kind(current),
            SyntaxKind::Attribute | SyntaxKind::Subscript
        ) {
            if self.tree.kind(current) == SyntaxKind::Subscript {
                found = Some(current);
            }
            current = self.tree.child(current, Role::Value)?;
        }
        found
    }

    /// The dimensions that `subscript` indexes with exactly the loop variable.
    fn loop_dimensions(&self, subscript: NodeId, parallel: &Parallel) -> BTreeSet<usize> {
        let Some(index) = self.tree.child(subscript, Role::Index) else {
            return BTreeSet::new();
        };
        let items: Vec<NodeId> = if self.tree.kind(index) == SyntaxKind::Tuple {
            self.tree.children_with(index, Role::Item).collect()
        } else {
            vec![index]
        };
        items
            .into_iter()
            .enumerate()
            .filter(|&(_, item)| {
                self.tree.kind(item) == SyntaxKind::Name
                    && matches!(
                        self.lowerer.definition(item),
                        Some(&Definition::Local(binding)) if Some(binding) == parallel.variable
                    )
            })
            .map(|(dimension, _)| dimension)
            .collect()
    }

    /// Reports `access`, at `subscript`, to the array `name` at an index other than the loop
    /// variable. `adds` says every write to the array adds to it, so `accumulate=` would do.
    fn wrong_index(
        &mut self,
        parallel: &Parallel,
        access: &Access,
        subscript: NodeId,
        name: &str,
        adds: bool,
    ) {
        let index = self
            .tree
            .child(subscript, Role::Index)
            .map_or("", |index| self.tree.text(index, self.text));
        let label = match parallel.variable {
            Some(variable) => format!(
                "indexed by `{index}`, not by the loop variable `{}`",
                self.tree.text(variable, self.text)
            ),
            None => format!("indexed by `{index}`, not by a single loop variable"),
        };
        let noun = parallel.kind.noun();
        let mut diagnostic = if access.usage == Usage::Read {
            Diagnostic::error(format!(
                "an iteration of this {noun} may read an element of `{name}` that another iteration writes"
            ))
        } else {
            Diagnostic::error(format!(
                "iterations of this {noun} may write the same element of `{name}`"
            ))
        }
        .with_primary(self.tree.span(access.node), label)
        .with_secondary(self.tree.span(parallel.call), parallel.kind.together());
        if access.usage == Usage::Read {
            diagnostic = diagnostic.with_note(
                "an array the loop writes may be read only at the loop variable's index; read a copy made before the loop instead",
            );
        } else {
            diagnostic = diagnostic.with_note(
                "each element of a shared array may be written only at the loop variable's index",
            );
            if adds {
                let call = self.tree.text(parallel.call, self.text);
                let opened = call.strip_suffix(')').unwrap_or(call);
                diagnostic = diagnostic.with_note(format!(
                    "if every write to `{name}` in this loop adds to it, declare it add-only: `{opened}, accumulate={name})`"
                ));
            }
            diagnostic = diagnostic.with_note(
                "or compute each element's total in its own iteration, or use `range` for this loop",
            );
        }
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Reports `access` using the array `name`, which the loop writes element by element,
    /// without indexing it.
    fn used_whole(&mut self, parallel: &Parallel, access: &Access, name: &str) {
        let noun = parallel.kind.noun();
        let diagnostic = Diagnostic::error(format!(
            "`{name}` is written element by element in this {noun}, so the body cannot use it whole"
        ))
        .with_primary(self.tree.span(access.node), "used whole here")
        .with_secondary(self.tree.span(parallel.call), parallel.kind.together())
        .with_note(
            "use single elements at the loop variable's index, or use `range` for this loop",
        );
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Checks every shared variable written without an index, other than the `arrays` the
    /// element check covered: each must be one kind of reduction, read nowhere else.
    pub(super) fn reductions(
        &mut self,
        parallel: &Parallel,
        accesses: &[Access],
        arrays: &[(NodeId, Path)],
    ) {
        let noun = parallel.kind.noun();
        let mut scalars: Vec<(NodeId, Path)> = Vec::new();
        for access in accesses {
            let covered = arrays
                .iter()
                .any(|(binding, path)| *binding == access.binding && overlap(path, &access.path));
            let scalar = (access.binding, access.path.clone());
            if access.usage != Usage::Read && !covered && !scalars.contains(&scalar) {
                scalars.push(scalar);
            }
        }
        for (binding, path) in &scalars {
            let members: Vec<&Access> = accesses
                .iter()
                .filter(|access| access.binding == *binding && overlap(&access.path, path))
                .collect();
            let mut first: Option<(NodeId, Reduction)> = None;
            for access in &members {
                match access.usage {
                    Usage::Read => {}
                    Usage::Reduce(reduction) if access.path == *path => match first {
                        None => first = Some((access.node, reduction)),
                        Some((_, known)) if known == reduction => {}
                        Some((site, _)) => self.mixed_reduction(access, site, noun),
                    },
                    Usage::Reduce(_) | Usage::Write => self.not_a_reduction(parallel, access),
                }
            }
            let Some((site, _)) = first else {
                continue;
            };
            for access in members {
                if access.usage == Usage::Read {
                    let name = self.tree.text(site, self.text);
                    let diagnostic = Diagnostic::error(format!(
                        "`{name}` is a reduction in this {noun}, so the body cannot read it"
                    ))
                    .with_primary(self.tree.span(access.node), "read here")
                    .with_secondary(self.tree.span(site), "reduced here")
                    .with_note("its total is known only after the loop");
                    self.lowerer.diagnostics.push(diagnostic);
                }
            }
        }
    }

    /// Reports `access`, a write to a shared variable that is not a reduction.
    fn not_a_reduction(&mut self, parallel: &Parallel, access: &Access) {
        let noun = parallel.kind.noun();
        let name = self.tree.text(access.node, self.text);
        let diagnostic = Diagnostic::error(format!(
            "iterations of this {noun} all write `{name}`"
        ))
        .with_primary(self.tree.span(access.node), "not a reduction")
        .with_secondary(self.tree.span(parallel.call), parallel.kind.together())
        .with_note(format!(
            "a shared variable is written in a {noun} only as a reduction: `x += e`, `x -= e`, `x *= e`, `x = min(x, e)` or `x = max(x, e)`",
        ))
        .with_note("to keep a value for one iteration, assign a new variable inside the loop");
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Reports `access`, a reduction of another kind than the first one, at `site`, in a
    /// loop named `noun`.
    fn mixed_reduction(&mut self, access: &Access, site: NodeId, noun: &str) {
        let name = self.tree.text(access.node, self.text);
        let diagnostic = Diagnostic::error(format!(
            "`{name}` is reduced two ways in this {noun}"
        ))
        .with_primary(self.tree.span(access.node), "a different reduction")
        .with_secondary(self.tree.span(site), "first reduced here")
        .with_note(
            "a shared variable reduces one way: `+=` and `-=` together, or `*=`, `min` or `max` alone",
        );
        self.lowerer.diagnostics.push(diagnostic);
    }
}
