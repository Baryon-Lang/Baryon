//! `x.copy()` where the type of `x` is a type parameter (roadmap 1.9.60, decision 0066).
//!
//! A generic body is lowered once, where `x` has type `T`, so lowering cannot choose among the
//! copies each type has: a list's loop of appends, an array's new buffer, a struct's own
//! method. It writes one [`Builtin::Copy`] instead, into a local of its own, which
//! monomorphization sends to a struct's own `copy` method and the backend writes for the
//! other types the checker lets a type parameter be copied as.

use crate::builder::Builder;
use mir::{Argument, Builtin, Callee, Operand, Place, Rvalue, TerminatorKind};
use syntax::NodeId;

impl Builder<'_> {
    /// `value.copy()` into `place`, `value` of a type parameter's type: a [`Builtin::Copy`]
    /// that borrows the value into a new local, which then moves into `place`.
    pub(crate) fn parameter_copy(&mut self, place: Place, value: NodeId, node: NodeId) {
        let ty = self.ty(value);
        let source = self.borrow(value);
        let copied = self.temporary(ty, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Copy),
                arguments: vec![Argument::Borrow(source)],
                destination: copied.into(),
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
        self.assign(place, Rvalue::Use(Operand::Move(copied.into())), node);
    }
}
