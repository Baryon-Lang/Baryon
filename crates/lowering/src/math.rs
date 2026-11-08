//! Mathematical functions of floats: the `math` functions the checker types (`sqrt`, `exp`,
//! `log`, `sin`, `cos` and `pow`), `x ** y` and `x **= y` on floats, the built-in `round(x)`,
//! and `round` of the elements of a `Fixed` array, each a call of a [`Builtin::Math`] function that a backend writes itself.
//! The operands convert to the result's float type first, as number mixing says (decision 0014),
//! so `x ** 3` on an `f64` is `pow(x, 3.0)` and `sqrt(n)` on an integer is `sqrt(f64(n))`.
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

    /// `math.<member>(arguments…)` into `place`, with each argument converted to the result's
    /// float type. A member the checker typed but this slice has no function for is reported
    /// as not lowered yet.
    pub(crate) fn math_function(
        &mut self,
        place: Place,
        member: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let function = match member {
            "sqrt" => MathFunction::Sqrt,
            "exp" => MathFunction::Exp,
            "log" => MathFunction::Log,
            "sin" => MathFunction::Sin,
            "cos" => MathFunction::Cos,
            "pow" => MathFunction::Power,
            _ => return self.unsupported(node, &format!("`math.{member}`")),
        };
        let ty = self.ty(node);
        if !self.is_float(ty) || arguments.len() != function.arity() {
            return self.unsupported(node, &format!("this call of `math.{member}`"));
        }
        let operands = arguments
            .iter()
            .map(|&argument| self.operand_as(argument, ty))
            .collect();
        self.math_call(place, function, operands, node);
    }

    /// `round(x)` with one argument, into `place`: the nearest integer, ties to even, as
    /// Python's `round` gives, so `round(2.5)` is 2. A float rounds with [`MathFunction::RoundEven`]
    /// and then converts, which panics when the result does not fit in the integer, as every
    /// float-to-integer conversion does. An integer is already whole.
    pub(crate) fn round(&mut self, place: Place, argument: NodeId, node: NodeId) {
        let (from, to) = (self.ty(argument), self.ty(node));
        let value = self.operand(argument);
        if !self.is_float(from) {
            let rvalue = self.coerce(value, from, to);
            return self.assign(place, rvalue, node);
        }
        let rounded = self.temporary(from, node);
        self.math_call(
            Place::from(rounded),
            MathFunction::RoundEven,
            vec![value],
            node,
        );
        let rvalue = self.coerce(Operand::Copy(Place::from(rounded)), from, to);
        self.assign(place, rvalue, node);
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
