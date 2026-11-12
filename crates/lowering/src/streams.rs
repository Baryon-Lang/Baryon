//! The standard streams (decision 0036, part 6): `sys.stdin`, `sys.stdout` and `sys.stderr`,
//! `print(..., file=, flush=)` and `input(prompt)`.
//!
//! - A read of `sys.stdout` is a call of [`Builtin::StandardStream`], which gives a new `File`
//!   on the stream; the statement that made it drops it, which leaves the stream open.
//! - `print` picks its call by where `file=` points ([`PrintTarget`]): [`Builtin::Print`] for
//!   standard output, [`Builtin::PrintError`] for standard error, and [`FileMethod::Print`],
//!   which can fail with `OSError`, for any other file.
//! - `flush=` is a branch on its value to [`Builtin::FlushOutput`] or [`FileMethod::Flush`];
//!   standard error holds nothing back, so there it is only computed.
//! - `input(prompt)` is a call of [`Builtin::Input`], whose error edge carries the `EOFError`
//!   the checker recorded.

use crate::builder::Builder;
use database::Definition;
use mir::{
    Argument, Builtin, Callee, Constant, ConstantValue, FileMethod, Operand, Place, TerminatorKind,
};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// Where a `print` writes.
pub(crate) enum PrintTarget {
    /// Standard output: no `file=`, or `file=sys.stdout`.
    Output,
    /// Standard error: `file=sys.stderr`.
    Error,
    /// The file at this place: any other `file=`.
    File(Place),
}

impl<'db> Builder<'db> {
    /// The number of the standard stream `node` names where it stands, 0 for `sys.stdin`, 1 for
    /// `sys.stdout` and 2 for `sys.stderr`, also after `from sys import stderr`. `None` for
    /// anything else.
    pub(crate) fn standard_stream(&self, node: NodeId) -> Option<i128> {
        let name = match self.tree.kind(node) {
            SyntaxKind::Name => Some(node),
            SyntaxKind::Attribute => self.tree.child(node, Role::Attribute),
            _ => None,
        }?;
        match self.definition(name)? {
            Definition::Standard {
                module: "sys",
                member: Some(member),
            } => ["stdin", "stdout", "stderr"]
                .iter()
                .zip(0..)
                .find_map(|(stream, number)| (member == stream).then_some(number)),
            _ => None,
        }
    }

    /// `sys.stdin`, `sys.stdout` or `sys.stderr`, the stream numbered `stream`, into `place`: a
    /// new `File` on it. Cost: one small allocation, freed when the handle is dropped.
    pub(crate) fn stream(&mut self, place: Place, stream: i128, node: NodeId) {
        let number = Operand::Constant(Constant::integer(self.int_type(), stream));
        let arguments = vec![Argument::Owned(number)];
        self.call_builtin(Builtin::StandardStream, arguments, place, node);
    }

    /// Where the `file=` argument `value` of a `print` points: a standard stream named there,
    /// or the file `value` gives, borrowed.
    pub(crate) fn print_target(&mut self, value: NodeId) -> PrintTarget {
        match self.standard_stream(value) {
            Some(2) => PrintTarget::Error,
            Some(_) => PrintTarget::Output,
            None => PrintTarget::File(self.borrow(value)),
        }
    }

    /// Writes the `print` call `node`, whose values, separators and line end are `passed`, to
    /// `target`, its `None` result into `place`, then writes the target out when `flush`, the
    /// value of `flush=`, is true. A print to a file takes the error edge the checker recorded
    /// at `node`, and so does its flush.
    pub(crate) fn print_to(
        &mut self,
        target: &PrintTarget,
        flush: Option<NodeId>,
        passed: Vec<Argument<'db>>,
        place: Place,
        node: NodeId,
    ) {
        match target {
            PrintTarget::Output => self.call_builtin(Builtin::Print, passed, place, node),
            PrintTarget::Error => self.call_builtin(Builtin::PrintError, passed, place, node),
            PrintTarget::File(file) => {
                let mut arguments = vec![Argument::Borrow(file.clone())];
                arguments.extend(passed);
                self.file_call(FileMethod::Print, arguments, place, node);
            }
        }
        let Some(flush) = flush else {
            return;
        };
        let wanted = self.operand(flush);
        if matches!(target, PrintTarget::Error) {
            return;
        }
        let (flushing, next) = (self.new_block(), self.new_block());
        self.branch(wanted, (flushing, next), node);
        self.start(flushing);
        let nothing = self.temporary(Type::new(self.db, TypeKind::None), node);
        match target {
            PrintTarget::File(file) => {
                let arguments = vec![Argument::Borrow(file.clone())];
                self.file_call(FileMethod::Flush, arguments, Place::from(nothing), node);
            }
            _ => self.call_builtin(Builtin::FlushOutput, Vec::new(), Place::from(nothing), node),
        }
        self.terminate(TerminatorKind::Goto(next), node);
        self.start(next);
    }

    /// `input()` or `input(prompt)` into `place`, a new `str`: a call that fails with the
    /// `EOFError` the checker recorded for `node`, which goes where any error of a call goes.
    /// A missing prompt is the empty string.
    pub(crate) fn input(&mut self, place: Place, prompt: Option<NodeId>, node: NodeId) {
        let prompt = match prompt {
            Some(prompt) if !self.is_literal(prompt) => Argument::Borrow(self.borrow(prompt)),
            Some(prompt) => Argument::Owned(self.operand(prompt)),
            None => Argument::Owned(Operand::Constant(Constant {
                ty: Type::new(self.db, TypeKind::Str),
                value: ConstantValue::Str(String::new()),
            })),
        };
        let errors = self.typed.errors(node);
        let edge = self.error_edge(errors, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Input),
                arguments: vec![prompt],
                destination: place,
                target: next,
                error: edge.clone(),
            },
            node,
        );
        if let Some(edge) = edge {
            self.dispatch(&edge, errors, node);
        }
        self.start(next);
    }
}
