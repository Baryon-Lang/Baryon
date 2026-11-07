//! Errors (language reference, "Errors and panics"; decisions 0005 and 0018): a call that can
//! fail gets an error edge, `raise` fails, and `try` with its `except` clauses catches.
//!
//! An error never unwinds. Each place an error can start, a fallible call's error edge or a
//! `raise`, goes to one of two places, decided here while lowering:
//!
//! - the `except` clause of the innermost enclosing `try` that names the error's type: the
//!   error is stored where the clause keeps it, the temporaries made since the `try` began are
//!   dropped, and control jumps to the clause's body;
//! - otherwise the caller: everything the function owns is dropped, and a
//!   [`Raise`](mir::TerminatorKind::Raise) returns the error.
//!
//! At an error edge the error local holds one of the callee's errors. A callee with one error
//! type needs no test; with several, a [`Switch`](mir::TerminatorKind::Switch) on its
//! [`ErrorIndex`](mir::Rvalue::ErrorIndex) picks the block for each, and each block reads the
//! error with the [`Error`](mir::Projection::Error) projection.

use crate::builder::{Builder, Clause, Handler};
use database::Definition;
use mir::{Constant, ErrorEdge, Local, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Nominal, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The error edge for a call `node` that can fail with `errors`: a new local to receive the
    /// error and a new block to go to. `None` for a call that cannot fail.
    pub(crate) fn error_edge(&mut self, errors: &[Type<'db>], node: NodeId) -> Option<ErrorEdge> {
        if errors.is_empty() {
            return None;
        }
        let nothing = Type::new(self.db, TypeKind::None);
        let ty = Type::new(
            self.db,
            TypeKind::Fallible {
                value: nothing,
                errors: errors.to_vec(),
            },
        );
        Some(ErrorEdge {
            error: self.temporary(ty, node),
            target: self.new_block(),
        })
    }

    /// The code at a call's error edge, in the edge's block: each error the call can fail with
    /// goes where [`fail`](Self::fail) sends it.
    pub(crate) fn dispatch(&mut self, edge: &ErrorEdge, errors: &[Type<'db>], node: NodeId) {
        self.start(edge.target);
        let place = Place::from(edge.error);
        let error = |position: usize| {
            let position = u32::try_from(position).unwrap_or(u32::MAX);
            Operand::Move(place.clone().project(Projection::Error(position)))
        };
        if let [only] = errors {
            return self.fail(error(0), *only, node);
        }
        let which = self.temporary(self.int_type(), node);
        self.assign(Place::from(which), Rvalue::ErrorIndex(place.clone()), node);
        let blocks: Vec<BlockAndType<'db>> =
            errors.iter().map(|&ty| (self.new_block(), ty)).collect();
        self.switch(which, &blocks, node);
        for (position, &(block, ty)) in blocks.iter().enumerate() {
            self.start(block);
            self.fail(error(position), ty, node);
        }
    }

    /// Ends the current block with a switch on the `i64` in `which`: value `n` goes to the
    /// `n`th block, and any other value cannot happen.
    fn switch(&mut self, which: Local, blocks: &[BlockAndType<'db>], node: NodeId) {
        let otherwise = self.new_block();
        let cases = (0..)
            .zip(blocks)
            .map(|(position, &(block, _))| (position, block))
            .collect();
        self.terminate(
            TerminatorKind::Switch {
                value: Operand::Copy(Place::from(which)),
                cases,
                otherwise,
            },
            node,
        );
    }

    /// Fails at `node` with `error`, of the error type `ty`: to the innermost `except` that
    /// catches `ty`, or else to the caller.
    pub(crate) fn fail(&mut self, error: Operand<'db>, ty: Type<'db>, node: NodeId) {
        let found = self.handlers.iter().rev().find_map(|handler| {
            handler.clauses.iter().find_map(|clause| {
                let position = clause.errors.iter().position(|&caught| caught == ty)?;
                Some((handler.depth, clause.clone(), position))
            })
        });
        let Some((depth, clause, position)) = found else {
            // The error may live in a variable or temporary that is about to be dropped, so it
            // moves out first.
            let error = match error {
                Operand::Move(place) if self.dropped_on_exit(place.local) => {
                    let held = self.temporary(ty, node);
                    self.assign(Place::from(held), Rvalue::Use(Operand::Move(place)), node);
                    Operand::Move(Place::from(held))
                }
                error => error,
            };
            self.drop_all(node);
            return self.terminate(TerminatorKind::Raise(error), node);
        };
        let local = Place::from(clause.locals[position]);
        if self.needs_drop(ty) {
            // The variable may still hold an error caught on an earlier pass.
            self.push(mir::StatementKind::Drop(local.clone()), node);
        }
        // Stored before the temporaries are dropped, since it may be one of them.
        self.assign(local, Rvalue::Use(error), node);
        self.drop_temporaries(depth, node);
        if let Some(which) = clause.which {
            let position = i128::try_from(position).unwrap_or(i128::MAX);
            let constant = Constant::integer(self.int_type(), position);
            self.assign(
                Place::from(which),
                Rvalue::Use(Operand::Constant(constant)),
                node,
            );
        }
        self.terminate(TerminatorKind::Goto(clause.block), node);
    }

    /// A bare `raise` inside an `except` clause: fails again with the error the clause caught.
    pub(crate) fn raise_again(&mut self, statement: NodeId) {
        let Some(clause) = self.caught.last().cloned() else {
            return;
        };
        let error = |position: usize| Operand::Move(Place::from(clause.locals[position]));
        let Some(which) = clause.which else {
            return self.fail(error(0), clause.errors[0], statement);
        };
        let blocks: Vec<BlockAndType<'db>> = clause
            .errors
            .iter()
            .map(|&ty| (self.new_block(), ty))
            .collect();
        self.switch(which, &blocks, statement);
        for (position, &(block, ty)) in blocks.iter().enumerate() {
            self.start(block);
            self.fail(error(position), ty, statement);
        }
    }

    /// `try:` with its `except` clauses. The body runs with the clauses as the innermost
    /// handler; each clause's body starts in its own block, and every path that finishes joins
    /// after the statement.
    pub(crate) fn try_statement(&mut self, statement: NodeId) {
        let nodes: Vec<NodeId> = self.tree.children_with(statement, Role::Handler).collect();
        let mut clauses = Vec::with_capacity(nodes.len());
        // Whether each clause keeps its errors in temporaries, which its body then owns.
        let mut hidden = Vec::with_capacity(nodes.len());
        for node in nodes {
            let Some(errors) = self.except_types(node) else {
                return self.unsupported(node, "this `except`");
            };
            let bound = self
                .tree
                .child(node, Role::Name)
                .and_then(|name| self.target_variable(name));
            let locals = match (errors.as_slice(), bound) {
                ([_], Some(variable)) => vec![variable],
                _ => errors.iter().map(|&ty| self.temporary(ty, node)).collect(),
            };
            let which = (errors.len() > 1).then(|| self.temporary(self.int_type(), node));
            hidden.push(bound.is_none() || which.is_some());
            clauses.push(Clause {
                node,
                errors,
                locals,
                which,
                block: self.new_block(),
            });
        }
        let depth = self.depth();
        self.handlers.push(Handler {
            clauses: clauses.clone(),
            depth,
        });
        self.block(statement, Role::Body);
        self.handlers.pop();
        let mut end = None;
        self.jump(&mut end, statement);
        for (clause, hidden) in clauses.into_iter().zip(hidden) {
            self.start(clause.block);
            self.enter();
            if hidden {
                for &local in &clause.locals {
                    self.drop_later(local);
                }
            }
            self.caught.push(clause.clone());
            self.block(clause.node, Role::Body);
            self.caught.pop();
            self.leave(clause.node);
            self.jump(&mut end, clause.node);
        }
        self.join(end);
    }

    /// The error types an `except` names: one class, or a parenthesized tuple of them. `None`
    /// when one is written some other way, such as through a module.
    fn except_types(&self, handler: NodeId) -> Option<Vec<Type<'db>>> {
        let written = self.tree.child(handler, Role::Type)?;
        let nodes: Vec<NodeId> = if self.tree.kind(written) == SyntaxKind::Tuple {
            self.tree.children_with(written, Role::Item).collect()
        } else {
            vec![written]
        };
        nodes
            .into_iter()
            .map(|node| match self.definition(node)? {
                &Definition::Item(file, name) => Some(Type::new(
                    self.db,
                    TypeKind::Struct(Nominal {
                        file,
                        name,
                        arguments: Vec::new(),
                    }),
                )),
                _ => None,
            })
            .collect()
    }
}

/// A block and the error type it handles.
type BlockAndType<'db> = (mir::BlockId, Type<'db>);
