//! The standard `random` module's `Philox` generator (decision 0028): `Philox(seed, stream)`
//! and `generator.uniform()` each become a call of a [`Builtin`], which the backend hands to the
//! runtime.

use crate::builder::Builder;
use mir::{Argument, Builtin, Callee, Constant, Operand, Place, TerminatorKind};
use syntax::NodeId;

impl<'db> Builder<'db> {
    /// `Philox(seed, stream)` into `place`: a call of [`Builtin::Philox`] with both as `i64`s,
    /// the stream 0 when the call leaves it out.
    pub(crate) fn philox(
        &mut self,
        place: Place,
        seed: NodeId,
        stream: Option<NodeId>,
        node: NodeId,
    ) {
        let int = self.int_type();
        let seed = self.operand_as(seed, int);
        let stream = match stream {
            Some(stream) => self.operand_as(stream, int),
            None => Operand::Constant(Constant::integer(int, 0)),
        };
        let arguments = vec![Argument::Owned(seed), Argument::Owned(stream)];
        self.builtin_call(Builtin::Philox, arguments, place, node);
    }

    /// `generator.uniform()` into `place`: a call of [`Builtin::Uniform`] with the generator
    /// passed `InOut`, since drawing changes it.
    pub(crate) fn uniform(&mut self, place: Place, generator: NodeId, node: NodeId) {
        let Some(generator) = self.changed_place(generator) else {
            return;
        };
        self.builtin_call(
            Builtin::Uniform,
            vec![Argument::InOut(generator)],
            place,
            node,
        );
    }

    /// A call of `builtin` with `arguments`, its result written into `place`; it cannot fail.
    pub(crate) fn builtin_call(
        &mut self,
        builtin: Builtin,
        arguments: Vec<Argument<'db>>,
        place: Place,
        node: NodeId,
    ) {
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(builtin),
                arguments,
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }
}
