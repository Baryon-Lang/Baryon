//! Errors (language reference, "Errors and panics", and decision 0005): which error types each
//! call and `raise` can fail with, and whether a `try` or the function's `Raises` covers each.
//!
//! A call to a function that returns `Raises[T, E]` has the type `T`; its error `E` passes on
//! to the caller unless a `try` around the call names `E` in an `except`. Every error type that
//! is not caught must be listed in the enclosing function's `Raises`; the error names the type
//! that is not, and how to cover it:
//!
//! ```text
//! error: `solve` can fail with `ConvergenceError`, which is not handled here
//! ```

use super::Body;
use super::flow::{End, Skip};
use crate::outline::{Base, Kind, outline};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of a call's value: the `T` of a call that returns `Raises[T, E]`, after
    /// checking that each `E` is handled; any other type unchanged.
    pub(super) fn fallible(&mut self, call: NodeId, ty: Type<'db>) -> Type<'db> {
        let TypeKind::Fallible { value, errors } = ty.kind(self.db) else {
            return ty;
        };
        let (value, errors) = (*value, errors.clone());
        let callee = self
            .tree
            .child(call, Role::Function)
            .map_or("this call", |function| self.tree.text(function, self.text))
            .to_owned();
        self.propagate(call, &format!("`{callee}` can fail with"), &errors);
        value
    }

    /// `raise E(...)`, or a bare `raise` inside an `except`, which raises the error it caught.
    pub(super) fn raise(&mut self, statement: NodeId) {
        if let Some(cause) = self.tree.child(statement, Role::Cause) {
            self.expression(cause, None);
        }
        let Some(value) = self.tree.child(statement, Role::Value) else {
            match self.caught.last().cloned() {
                Some(errors) => self.propagate(statement, "this `raise` passes on", &errors),
                None => {
                    self.lowerer.error(
                        statement,
                        "a bare `raise` must be inside an `except` block",
                        "nothing to raise again",
                        Some("raise an error value: `raise SolverError(...)`"),
                    );
                }
            }
            return;
        };
        let ty = self.expression(value, None);
        if matches!(ty.kind(self.db), TypeKind::Unknown) {
            return;
        }
        if self.is_error(ty) {
            self.propagate(value, "this `raise` fails with", &[ty]);
        } else {
            let message = format!("`raise` needs an error, found `{}`", self.show(ty));
            self.lowerer.error(
                value,
                &message,
                "not an error type",
                Some("an error type is a class with the base `Exception`"),
            );
        }
    }

    /// `try:` with its `except` clauses. The errors the clauses name are handled in the `try`
    /// body; each clause's body runs with its name bound to the error it caught. Returns
    /// whether the statement can fall through: when the body or any clause can.
    pub(super) fn try_statement(&mut self, statement: NodeId) -> bool {
        let handlers: Vec<NodeId> = self.tree.children_with(statement, Role::Handler).collect();
        let caught: Vec<Vec<Type<'db>>> = handlers
            .iter()
            .map(|&handler| self.except_types(handler))
            .collect();
        // A handler may run after the body stopped part way, so what the body assigns is
        // unknown in it.
        self.forget_assigned(statement);
        let entry = self.state.clone();
        let mut ends = Vec::new();
        self.handled.push(caught.concat());
        if self.block(statement, Role::Body) {
            let first = self.tree.children_with(statement, Role::Body).next();
            let label = "not assigned when the `try` body succeeds";
            ends.push(End {
                state: self.state.clone(),
                skip: Skip {
                    node: first.unwrap_or(statement),
                    label,
                },
            });
        }
        self.handled.pop();
        for (handler, errors) in handlers.into_iter().zip(caught) {
            self.state.clone_from(&entry);
            if let Some(name) = self.tree.child(handler, Role::Name) {
                let ty = match errors.as_slice() {
                    [only] => *only,
                    _ => self.lowerer.unknown(),
                };
                self.bind(name, ty, None);
            }
            self.caught.push(errors);
            if self.block(handler, Role::Body) {
                let label = "not assigned when this handler runs";
                ends.push(End {
                    state: self.state.clone(),
                    skip: Skip {
                        node: self.tree.child(handler, Role::Type).unwrap_or(handler),
                        label,
                    },
                });
            }
            self.caught.pop();
        }
        self.block(statement, Role::Else);
        self.block(statement, Role::Finally);
        self.merge(entry, &ends)
    }

    /// The error types an `except` names, one or a parenthesized tuple of them, reporting any
    /// that is not an error type.
    fn except_types(&mut self, handler: NodeId) -> Vec<Type<'db>> {
        let Some(written) = self.tree.child(handler, Role::Type) else {
            return Vec::new();
        };
        let nodes: Vec<NodeId> = if self.tree.kind(written) == SyntaxKind::Tuple {
            self.tree.children_with(written, Role::Item).collect()
        } else {
            vec![written]
        };
        let mut errors = Vec::new();
        for node in nodes {
            let ty = self.lowerer.lower(node);
            if self.is_error(ty) {
                errors.push(ty);
            } else {
                let message = format!("`{}` is not an error type", self.show(ty));
                self.lowerer.error(
                    node,
                    &message,
                    "expected an error type",
                    Some("an error type is a class with the base `Exception`"),
                );
            }
        }
        errors
    }

    /// Checks that each of `errors`, which `node` can fail with, is caught by an enclosing
    /// `try` or listed in the function's `Raises`. `what` starts the message, such as
    /// "`solve` can fail with".
    fn propagate(&mut self, node: NodeId, what: &str, errors: &[Type<'db>]) {
        let (value, allowed) = match self.signature.returns.kind(self.db) {
            TypeKind::Fallible { value, errors } => (*value, errors.clone()),
            _ => (self.signature.returns, Vec::new()),
        };
        // Inside a `prange` body only a `try` inside the body counts: an error cannot leave a
        // parallel loop (the `parallel` module).
        let parallel = self.parallel.last().map(|parallel| parallel.handled);
        for &error in errors {
            let frames = self
                .handled
                .get(parallel.unwrap_or(0)..)
                .unwrap_or_default();
            let covered = matches!(error.kind(self.db), TypeKind::Unknown)
                || (parallel.is_none() && allowed.contains(&error))
                || frames.iter().any(|frame| frame.contains(&error));
            if covered {
                continue;
            }
            let shown = self.show(error);
            if parallel.is_some() {
                let message =
                    format!("{what} `{shown}`, which is not handled inside this parallel loop");
                let hint = format!(
                    "catch it with `try:` and `except {shown}:` inside the loop body: an error cannot leave a parallel loop"
                );
                let label = format!("can fail with `{shown}`");
                self.lowerer.error(node, &message, &label, Some(&hint));
                continue;
            }
            let listed: Vec<String> = allowed
                .iter()
                .chain([&error])
                .map(|&ty| self.show(ty))
                .collect();
            let message = format!("{what} `{shown}`, which is not handled here");
            let hint = format!(
                "catch it with `try:` and `except {shown}:`, or let it pass on by declaring `-> Raises[{}, {}]`",
                self.show(value),
                listed.join(" | ")
            );
            let label = format!("can fail with `{shown}`");
            self.lowerer.error(node, &message, &label, Some(&hint));
        }
    }

    /// Whether `ty` is an error type: a class with the base `Exception`.
    fn is_error(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Struct(nominal) => matches!(
                outline(self.db, nominal.file).get(nominal.name).map(|entry| &entry.kind),
                Some(Kind::Class(class)) if class.base == Base::Error
            ),
            _ => false,
        }
    }
}
