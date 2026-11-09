//! Mathematical functions of floats: the `math` functions the checker types (`sqrt`, `exp`,
//! `log`, `sin`, `cos` and `pow`), `x ** y` and `x **= y` on floats, the built-in `round(x)`,
//! and `round` of the elements of a `Fixed` array, each a call of a [`Builtin::Math`] function
//! that a backend writes itself, and the built-in `abs(x)`, a comparison and a subtraction.
//! The operands convert to the result's float type first, as number mixing says (decision 0014),
//! so `x ** 3` on an `f64` is `pow(x, 3.0)` and `sqrt(n)` on an integer is `sqrt(f64(n))`.
//!
//! `x ** n` and `x **= n` on integers are repeated squaring in the IR, with a panic on a negative
//! exponent, as the reference's errors chapter says, and each multiplication checked for
//! overflow as decision 0030 requires.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Check, Constant, MathFunction, Operand, Place,
    Rvalue, TerminatorKind,
};
use syntax::NodeId;
use types::{Primitive, Type, TypeKind};

/// The overflow check of a multiplication inside integer `**`.
const POWER: (BinaryOperator, Check<'static>) = (BinaryOperator::Multiply, Check::NoPowerOverflow);

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
    /// `nan` stays `nan`. The absolute value of the most negative integer does not fit; it
    /// wraps around to itself, as integer overflow does today. A complex number, whose `abs`
    /// is its modulus, is reported as not lowered yet.
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
        let negated = Rvalue::Binary(
            BinaryOperator::Subtract,
            zero,
            Operand::Copy(Place::from(value)),
        );
        self.assign(place.clone(), negated, node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(keep);
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(value))), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// `place = base ** exponent` on the integer type `ty`, by repeated squaring: `result`
    /// starts at 1, and for each bit of the exponent, lowest first, `result *= base` when the bit
    /// is set, then `base *= base` while higher bits remain. That takes about `2 * log2(n)`
    /// multiplications, and `0 ** 0` is 1, as in Python.
    ///
    /// A signed exponent below zero panics, naming it, since the result would be a fraction.
    /// Each multiplication is checked for overflow. `base` is squared only while a higher bit
    /// will multiply the square into the result, so a squaring overflows only when the result
    /// would; a release build drops the checks and gets the result wrapped, since wrapping
    /// multiplication is exact modulo 2 to the bit width.
    pub(crate) fn integer_power(
        &mut self,
        place: Place,
        (base, exponent): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
        node: NodeId,
    ) {
        let constant = |value| Operand::Constant(Constant::integer(ty, value));
        let copy = |local| Operand::Copy(Place::from(local));
        let factor = self.temporary(ty, node);
        self.assign(Place::from(factor), Rvalue::Use(base), node);
        let remaining = self.temporary(ty, node);
        self.assign(Place::from(remaining), Rvalue::Use(exponent), node);
        let boolean = self.boolean_type();
        let signed = matches!(ty.kind(self.db),
            TypeKind::Primitive(Primitive::Integer(integer)) if integer.is_signed());
        if signed {
            let valid = self.temporary(boolean, node);
            let test = Rvalue::Binary(BinaryOperator::GreaterEqual, copy(remaining), constant(0));
            self.assign(Place::from(valid), test, node);
            self.then(node, |target| TerminatorKind::Assert {
                condition: copy(valid),
                check: Check::NonNegativeExponent {
                    exponent: copy(remaining),
                },
                target,
            });
        }
        let result = self.temporary(ty, node);
        self.assign(Place::from(result), Rvalue::Use(constant(1)), node);
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), node);

        // header: multiply in the factor when the lowest bit is set.
        self.start(header);
        let bit = self.temporary(ty, node);
        let lowest = Rvalue::Binary(BinaryOperator::BitAnd, copy(remaining), constant(1));
        self.assign(Place::from(bit), lowest, node);
        let set = self.temporary(boolean, node);
        let test = Rvalue::Binary(BinaryOperator::NotEqual, copy(bit), constant(0));
        self.assign(Place::from(set), test, node);
        let (multiply, shift) = (self.new_block(), self.new_block());
        self.branch(copy(set), (multiply, shift), node);
        self.start(multiply);
        let operands = (copy(result), copy(factor));
        self.checked_as(Place::from(result), POWER, operands, ty, node);
        self.terminate(TerminatorKind::Goto(shift), node);

        // shift: drop the bit; stop when none remain, else square the factor and go again.
        self.start(shift);
        let shifted = Rvalue::Binary(BinaryOperator::ShiftRight, copy(remaining), constant(1));
        self.assign(Place::from(remaining), shifted, node);
        let done = self.temporary(boolean, node);
        let test = Rvalue::Binary(BinaryOperator::Equal, copy(remaining), constant(0));
        self.assign(Place::from(done), test, node);
        let (square, end) = (self.new_block(), self.new_block());
        self.branch(copy(done), (end, square), node);
        self.start(square);
        let operands = (copy(factor), copy(factor));
        self.checked_as(Place::from(factor), POWER, operands, ty, node);
        self.terminate(TerminatorKind::Goto(header), node);

        self.start(end);
        self.assign(place, Rvalue::Use(copy(result)), node);
    }

    /// Ends the current block by going to `then` when `condition` holds, else to `otherwise`.
    fn branch(
        &mut self,
        condition: Operand<'db>,
        (then, otherwise): (mir::BlockId, mir::BlockId),
        node: NodeId,
    ) {
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            },
            node,
        );
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
