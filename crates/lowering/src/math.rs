//! Mathematical functions of floats: the `math` functions the checker types (`sqrt`, `exp`,
//! `log`, `sin`, `cos` and `pow`), `x ** y` and `x **= y` on floats, the built-in `round(x)`,
//! and `round` of the elements of a `Fixed` array, each a call of a [`Builtin::Math`] function
//! that a backend writes itself, and the built-in `abs(x)`, a comparison and a subtraction.
//! The operands convert to the result's float type first, as number mixing says (decision 0014),
//! so `x ** 3` on an `f64` is `pow(x, 3.0)` and `sqrt(n)` on an integer is `sqrt(f64(n))`.
//!
//! `**` on integers is not lowered yet: it needs the panic on a negative exponent that the
//! reference's errors chapter promises.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, MathFunction, Operand, Place, Rvalue, TerminatorKind,
};
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

    /// `abs(x)` of an integer or a float, into `place`: `0 - x` when `x` is below zero, else
    /// `x`. A float takes `0.0 - x` at zero too, so `abs(-0.0)` is `0.0` as in Python, and a
    /// `nan` stays `nan`. The absolute value of the most negative integer does not fit, so
    /// the integer `0 - x` is checked as `-x` is: a debug build panics with
    /// ``integer overflow in `-` `` and a release build wraps it around to itself (decision
    /// 0030). A complex number, whose `abs` is its modulus, is reported as not lowered yet.
    pub(crate) fn absolute(&mut self, place: Place, argument: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let below = if self.is_float(ty) {
            BinaryOperator::LessEqual
        } else if self.is_integer(ty) {
            BinaryOperator::Less
        } else {
            return self.unsupported(node, "`abs` of a complex number");
        };
        let value = self.temporary(ty, argument);
        let operand = self.operand_as(argument, ty);
        self.assign(Place::from(value), Rvalue::Use(operand), argument);
        let zero = Operand::Constant(self.number_constant(ty, 0));
        let negative = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(negative),
            Rvalue::Binary(below, Operand::Copy(Place::from(value)), zero.clone()),
            node,
        );
        let (negate, keep, join) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(negative)),
                then: negate,
                otherwise: keep,
            },
            node,
        );
        self.start(negate);
        let negated = (zero, Operand::Copy(Place::from(value)));
        self.arithmetic(place.clone(), BinaryOperator::Subtract, negated, ty, node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(keep);
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(value))), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
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
