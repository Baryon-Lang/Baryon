//! Mathematical functions of floats: `x ** y` and `x **= y` on floats, and `round` of the
//! elements of a `Fixed` array, each a call of a [`Builtin::Math`] function that a backend writes
//! itself. The operands convert to the result's float type first, as number mixing says
//! (decision 0014), so `x ** 3` on an `f64` is `pow(x, 3.0)`.
//!
//! `**` on integers is not lowered yet: it needs the panic on a negative exponent that the
//! reference's errors chapter promises.

use crate::builder::Builder;
use mir::{Argument, Builtin, Callee, MathFunction, Operand, Place, TerminatorKind};
use syntax::NodeId;
use types::{Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether `ty` is `f32` or `f64`.
    pub(crate) fn is_float(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Float(_)))
    }

    /// `place = function(operands…)`, where `place` and every operand have one float type.
    pub(crate) fn math_call(
        &mut self,
        place: Place,
        function: MathFunction,
        operands: Vec<Operand<'db>>,
        node: NodeId,
    ) {
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Math(function)),
                arguments: operands.into_iter().map(Argument::Owned).collect(),
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }
}
