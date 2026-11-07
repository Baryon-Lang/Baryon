//! Parallel loops (language reference, "Parallel and vector loops", "What a `prange` body may
//! do"). The iterations of `for i in prange(n):` may run in any order on any number of
//! threads, so the checker proves that the result cannot depend on that:
//!
//! - A variable assigned before the loop is *shared*; one first assigned inside it is private
//!   to each iteration, and free to use.
//! - An element of a shared array is written only where every access to that array, read or
//!   write, indexes one fixed dimension with exactly the loop variable.
//! - A shared scalar is written only as a *reduction*: `x += e` and `x -= e`, `x *= e`,
//!   `x = min(x, e)` or `x = max(x, e)`, one kind per variable, and read nowhere else in the
//!   body.
//! - Passing a shared place as `InOut`, or appending to a shared list, counts as writing it.
//! - The body never leaves the loop early, and holds no other `prange`.
//!
//! Arrays named in `accumulate=` follow their own rule, in the `accumulate` module.
//!
//! Each use of a shared variable is found by walking the body from the top, so a place such
//! as `atoms.forces[i]` is seen whole, and its writes are those the `exclusive` module's
//! `change` recorded while the body was checked.

use super::Body;
use database::Definition;
use diagnostics::Diagnostic;
use std::collections::BTreeSet;
use syntax::{NodeId, Operator, Role, SyntaxKind};

/// A `prange` loop whose body is being checked.
pub(super) struct Parallel {
    /// The `prange(...)` call.
    pub(super) call: NodeId,
    /// The loop variable, by its binding.
    pub(super) variable: Option<NodeId>,
    /// The variables assigned before the loop, which every iteration shares.
    shared: BTreeSet<NodeId>,
    /// The places the body changes, by node, recorded as the body is checked.
    pub(super) writes: BTreeSet<NodeId>,
    /// The arrays listed in `accumulate=` (the `accumulate` module).
    pub(super) accumulate: Vec<NodeId>,
}

/// A place's steps after its variable: a field's name, or `None` for any `[...]`.
pub(super) type Path = Vec<Option<String>>;

/// One use of a shared variable in a parallel loop's body.
pub(super) struct Access {
    /// The place, such as `forces[j]`.
    pub(super) node: NodeId,
    /// Its variable.
    pub(super) binding: NodeId,
    /// Its steps.
    pub(super) path: Path,
    /// What the use does.
    pub(super) usage: Usage,
}

/// What one use of a shared place does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Usage {
    /// Reads it.
    Read,
    /// Writes it as a reduction of this kind.
    Reduce(Reduction),
    /// Writes it any other way.
    Write,
}

/// The kinds of reduction a shared scalar may take part in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Reduction {
    /// `x += e` and `x -= e`.
    Sum,
    /// `x *= e`.
    Product,
    /// `x = min(x, e)`.
    Min,
    /// `x = max(x, e)`.
    Max,
}

impl Body<'_> {
    /// Whether `iterator` is a call to `prange`.
    pub(super) fn is_prange(&self, iterator: NodeId) -> bool {
        self.tree.kind(iterator) == SyntaxKind::Call
            && self
                .tree
                .child(iterator, Role::Function)
                .is_some_and(|function| {
                    matches!(
                        self.lowerer.definition(function),
                        Some(Definition::Builtin("prange"))
                    )
                })
    }

    /// Starts checking the `prange` loop `statement` over `call`, before its body: returns
    /// whether it did, or reports it nested in another parallel loop.
    pub(super) fn enter_parallel(&mut self, statement: NodeId, call: NodeId) -> bool {
        if let Some(outer) = self.parallel.last() {
            let diagnostic = Diagnostic::error("a parallel loop cannot run inside another")
                .with_primary(self.tree.span(call), "inside a parallel loop")
                .with_secondary(self.tree.span(outer.call), "the outer parallel loop")
                .with_note(
                    "use `range` for the inner loop: nested parallel loops would oversubscribe the cores",
                );
            self.lowerer.diagnostics.push(diagnostic);
            self.accumulate.clear();
            return false;
        }
        let variable = self.tree.child(statement, Role::Target).and_then(|target| {
            match self.lowerer.definition(target) {
                Some(&Definition::Local(binding)) => Some(binding),
                _ => None,
            }
        });
        let mut shared = self.state.assigned.clone();
        if let Some(variable) = variable {
            shared.remove(&variable);
        }
        self.parallel.push(Parallel {
            call,
            variable,
            shared,
            writes: BTreeSet::new(),
            accumulate: std::mem::take(&mut self.accumulate),
        });
        true
    }

    /// Finishes checking the `prange` loop `statement`, after its body.
    pub(super) fn leave_parallel(&mut self, statement: NodeId) {
        let Some(parallel) = self.parallel.pop() else {
            return;
        };
        self.early_exits(statement, &parallel, false, false);
        let mut accesses = Vec::new();
        for child in self.tree.children_with(statement, Role::Body) {
            self.collect(child, &parallel, &mut accesses);
        }
        let accesses = self.accumulated(&parallel, accesses);
        let elements = self.elements(&parallel, &accesses);
        self.reductions(&parallel, &accesses, &elements);
    }

    /// Reports each `return`, `raise` and `break` inside `node` that would leave the loop.
    /// `nested` says a loop inside the parallel one holds `node`, and `guarded` that the body
    /// of a `try` inside it does.
    fn early_exits(&mut self, node: NodeId, parallel: &Parallel, nested: bool, guarded: bool) {
        let in_try = self.tree.kind(node) == SyntaxKind::Try;
        let children: Vec<NodeId> = self.tree.children(node).collect();
        for child in children {
            let kind = self.tree.kind(child);
            if kind.is_expression() {
                continue;
            }
            let guarded = guarded || (in_try && self.tree.node(child).role() == Role::Body);
            let label = match kind {
                SyntaxKind::Return => Some("returns from inside the loop"),
                SyntaxKind::Break if !nested => Some("leaves the loop"),
                SyntaxKind::Raise if !guarded => Some("raises out of the loop"),
                _ => None,
            };
            if let Some(label) = label {
                let diagnostic = Diagnostic::error("a parallel loop cannot end early")
                    .with_primary(self.tree.span(child), label)
                    .with_secondary(self.tree.span(parallel.call), "iterations run in parallel")
                    .with_note(
                        "every iteration runs; skip the rest of one with `continue`, or use `range` for a loop that stops early",
                    );
                self.lowerer.diagnostics.push(diagnostic);
            }
            let nested = nested || matches!(kind, SyntaxKind::For | SyntaxKind::While);
            self.early_exits(child, parallel, nested, guarded);
        }
    }

    /// Records into `out` each use, inside `node`, of a variable `parallel` shares.
    fn collect(&self, node: NodeId, parallel: &Parallel, out: &mut Vec<Access>) {
        match self.tree.kind(node) {
            SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript => {
                let usage = if parallel.writes.contains(&node) {
                    Usage::Write
                } else {
                    Usage::Read
                };
                self.record(node, usage, parallel, out);
                self.collect_indices(node, parallel, out);
            }
            SyntaxKind::AugmentedAssign => {
                let usage = match self.tree.node(node).operator() {
                    Operator::Add | Operator::Subtract => Usage::Reduce(Reduction::Sum),
                    Operator::Multiply => Usage::Reduce(Reduction::Product),
                    _ => Usage::Write,
                };
                if let Some(target) = self.tree.child(node, Role::Target) {
                    self.record(target, usage, parallel, out);
                    self.collect_indices(target, parallel, out);
                }
                if let Some(value) = self.tree.child(node, Role::Value) {
                    self.collect(value, parallel, out);
                }
            }
            SyntaxKind::Call => {
                for child in self.tree.children(node).collect::<Vec<_>>() {
                    if self.tree.node(child).role() != Role::Function {
                        self.collect(child, parallel, out);
                    } else if self.tree.kind(child) == SyntaxKind::Attribute
                        && let Some(receiver) = self.tree.child(child, Role::Value)
                    {
                        // `values.append(x)`: the place is the receiver, `values`.
                        self.collect(receiver, parallel, out);
                    }
                }
            }
            kind => {
                if kind == SyntaxKind::Assign
                    && let Some((target, reduction, rest)) = self.min_max(node)
                {
                    // `x = min(x, e)`: the `x` inside the call is the reduction's own read.
                    self.record(target, Usage::Reduce(reduction), parallel, out);
                    self.collect_indices(target, parallel, out);
                    for argument in rest {
                        self.collect(argument, parallel, out);
                    }
                    return;
                }
                for child in self.tree.children(node).collect::<Vec<_>>() {
                    self.collect(child, parallel, out);
                }
            }
        }
    }

    /// The parts of `x = min(x, e)` or `x = max(x, e)`: the target, the reduction, and the
    /// arguments after the first. `None` for any other assignment.
    fn min_max(&self, assign: NodeId) -> Option<(NodeId, Reduction, Vec<NodeId>)> {
        let targets: Vec<NodeId> = self.tree.children_with(assign, Role::Target).collect();
        let ([target], Some(value)) = (targets.as_slice(), self.tree.child(assign, Role::Value))
        else {
            return None;
        };
        if self.tree.kind(value) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(value, Role::Function)?;
        let reduction = match self.lowerer.definition(function) {
            Some(Definition::Builtin("min")) => Reduction::Min,
            Some(Definition::Builtin("max")) => Reduction::Max,
            _ => return None,
        };
        let arguments = self.arguments(value);
        let (&first, rest) = arguments.positional.split_first()?;
        if self.tree.text(first, self.text) != self.tree.text(*target, self.text) {
            return None;
        }
        let mut rest = rest.to_vec();
        rest.extend(arguments.keywords.iter().map(|&(_, value)| value));
        Some((*target, reduction, rest))
    }

    /// Collects the uses inside the indices of the place `node`, and inside whatever it
    /// starts at when that is not a variable, such as the call in `f(x).y`.
    fn collect_indices(&self, node: NodeId, parallel: &Parallel, out: &mut Vec<Access>) {
        let mut current = node;
        loop {
            match self.tree.kind(current) {
                SyntaxKind::Subscript => {
                    if let Some(index) = self.tree.child(current, Role::Index) {
                        self.collect(index, parallel, out);
                    }
                }
                SyntaxKind::Attribute => {}
                SyntaxKind::Name => return,
                _ => {
                    self.collect(current, parallel, out);
                    return;
                }
            }
            match self.tree.child(current, Role::Value) {
                Some(value) => current = value,
                None => return,
            }
        }
    }

    /// Adds the place `node` to `out` when it starts at a shared variable.
    fn record(&self, node: NodeId, usage: Usage, parallel: &Parallel, out: &mut Vec<Access>) {
        if let Some((binding, path)) = self.path(node)
            && parallel.shared.contains(&binding)
        {
            out.push(Access {
                node,
                binding,
                path,
                usage,
            });
        }
    }

    /// The variable the place `node` starts at, and its steps.
    pub(super) fn path(&self, node: NodeId) -> Option<(NodeId, Path)> {
        let mut path = Vec::new();
        let mut current = node;
        loop {
            match self.tree.kind(current) {
                SyntaxKind::Name => break,
                SyntaxKind::Attribute => {
                    let field = self.tree.child(current, Role::Attribute)?;
                    path.push(Some(self.tree.text(field, self.text).to_owned()));
                }
                SyntaxKind::Subscript => path.push(None),
                _ => return None,
            }
            current = self.tree.child(current, Role::Value)?;
        }
        path.reverse();
        match self.lowerer.definition(current) {
            Some(&Definition::Local(binding)) => Some((binding, path)),
            _ => None,
        }
    }
}

/// Whether the paths `first` and `second` overlap: one is a prefix of the other, where any
/// `[...]` matches any other.
pub(super) fn overlap(first: &Path, second: &Path) -> bool {
    first.iter().zip(second).all(|(a, b)| a == b)
}
