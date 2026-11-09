//! Mathematical functions of floats: the `math` functions the checker types, `x ** y` and
//! `x **= y` on floats, the built-in `round(x)`, and `round` of the elements of a `Fixed` array,
//! each a call of a [`Builtin::Math`] function that a backend writes itself; `floor`, `ceil` and
//! `trunc`, such a call and a conversion to `int`; `isnan`, `isinf` and `isfinite`, comparisons;
//! `degrees`, `radians` and `log(x, base)`, arithmetic; and the built-in `abs(x)`, a comparison
//! and a subtraction.
//! The operands convert to the result's float type first, as number mixing says (decision 0014),
//! so `x ** 3` on an `f64` is `pow(x, 3.0)` and `sqrt(n)` on an integer is `sqrt(f64(n))`.
//!
//! `**` on integers is not lowered yet: it needs the panic on a negative exponent that the
//! reference's errors chapter promises.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, ConstantValue, MathFunction, Operand,
    Place, Rvalue, TerminatorKind,
};
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether `ty` is `f32` or `f64`.
    pub(crate) fn is_float(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Float(_)))
    }

    /// `math.<member>(arguments…)` into `place`, with each argument converted to the float type
    /// the function computes in. Most are one [`MathFunction`]; the rest are written here:
    /// `floor`, `ceil` and `trunc` round and then convert to `int`, `isnan`, `isinf` and
    /// `isfinite` compare, `degrees` and `radians` multiply by Python's constant, and
    /// `log(x, base)` divides two logarithms as Python does. A member the checker typed but
    /// this function does not know is reported as not lowered yet.
    pub(crate) fn math_function(
        &mut self,
        place: Place,
        member: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        match (member, arguments) {
            ("floor" | "ceil" | "trunc", &[argument]) => {
                let function = match member {
                    "floor" => MathFunction::Floor,
                    "ceil" => MathFunction::Ceil,
                    _ => MathFunction::Truncate,
                };
                return self.whole(place, function, argument, node);
            }
            ("isnan" | "isinf" | "isfinite", &[argument]) => {
                return self.classify(place, member, argument, node);
            }
            ("degrees" | "radians", &[argument]) => {
                let factor = if member == "degrees" {
                    180.0 / std::f64::consts::PI
                } else {
                    std::f64::consts::PI / 180.0
                };
                let ty = self.ty(node);
                let operand = self.operand_as(argument, ty);
                let factor = Operand::Constant(float_constant(ty, factor));
                let rvalue = Rvalue::Binary(BinaryOperator::Multiply, operand, factor);
                return self.assign(place, rvalue, node);
            }
            ("log", &[value, base]) => return self.logarithm(place, value, base, node),
            _ => {}
        }
        let Some(function) = function(member) else {
            return self.unsupported(node, &format!("`math.{member}`"));
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

    /// `math.floor`, `math.ceil`, `math.trunc` or `round` of `argument` into the `int` at
    /// `place`: the
    /// float rounded by `function`, then converted, which panics when the result does not fit,
    /// as every float-to-integer conversion does. An integer is already whole.
    fn whole(&mut self, place: Place, function: MathFunction, argument: NodeId, node: NodeId) {
        let (from, to) = (self.ty(argument), self.ty(node));
        let value = self.operand(argument);
        if !self.is_float(from) {
            let rvalue = self.coerce(value, from, to);
            return self.assign(place, rvalue, node);
        }
        let rounded = self.temporary(from, node);
        self.math_call(Place::from(rounded), function, vec![value], node);
        let rvalue = self.coerce(Operand::Copy(Place::from(rounded)), from, to);
        self.assign(place, rvalue, node);
    }

    /// `math.isnan`, `math.isinf` or `math.isfinite` of `argument` into the `bool` at `place`,
    /// with an integer converted to `f64` first, as Python does. `isnan(x)` is `x != x`, true
    /// only for NaN; `isinf(x)` is `fabs(x) == inf`; `isfinite(x)` is `x - x == 0.0`, since
    /// `x - x` is NaN for an infinity or NaN and zero for every other float.
    fn classify(&mut self, place: Place, member: &str, argument: NodeId, node: NodeId) {
        let from = self.ty(argument);
        let ty = if self.is_float(from) {
            from
        } else {
            self.float_type()
        };
        let value = self.temporary(ty, argument);
        let operand = self.operand_as(argument, ty);
        self.assign(Place::from(value), Rvalue::Use(operand), argument);
        let value = || Operand::Copy(Place::from(value));
        let rvalue = match member {
            "isnan" => Rvalue::Binary(BinaryOperator::NotEqual, value(), value()),
            "isinf" => {
                let size = self.temporary(ty, node);
                self.math_call(Place::from(size), MathFunction::Fabs, vec![value()], node);
                let infinity = Operand::Constant(float_constant(ty, f64::INFINITY));
                let size = Operand::Copy(Place::from(size));
                Rvalue::Binary(BinaryOperator::Equal, size, infinity)
            }
            _ => {
                let difference = self.temporary(ty, node);
                let subtract = Rvalue::Binary(BinaryOperator::Subtract, value(), value());
                self.assign(Place::from(difference), subtract, node);
                let zero = Operand::Constant(float_constant(ty, 0.0));
                let difference = Operand::Copy(Place::from(difference));
                Rvalue::Binary(BinaryOperator::Equal, difference, zero)
            }
        };
        self.assign(place, rvalue, node);
    }

    /// `log(argument)` at the float type `ty`, in a new temporary.
    fn natural_logarithm(&mut self, argument: NodeId, ty: Type<'db>, node: NodeId) -> Operand<'db> {
        let logarithm = self.temporary(ty, argument);
        let operand = self.operand_as(argument, ty);
        self.math_call(
            Place::from(logarithm),
            MathFunction::Log,
            vec![operand],
            node,
        );
        Operand::Copy(Place::from(logarithm))
    }

    /// `f64`, the type an integer converts to before a `math` function.
    fn float_type(&self) -> Type<'db> {
        Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)))
    }

    /// `math.log(value, base)` into `place`: `log(value) / log(base)`, as Python computes it.
    fn logarithm(&mut self, place: Place, value: NodeId, base: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let numerator = self.natural_logarithm(value, ty, node);
        let denominator = self.natural_logarithm(base, ty, node);
        let rvalue = Rvalue::Binary(BinaryOperator::Divide, numerator, denominator);
        self.assign(place, rvalue, node);
    }

    /// `round(x)` with one argument, into `place`: the nearest integer, ties to even, as
    /// Python's `round` gives, so `round(2.5)` is 2. A float rounds with [`MathFunction::RoundEven`]
    /// and then converts, which panics when the result does not fit in the integer, as every
    /// float-to-integer conversion does. An integer is already whole.
    pub(crate) fn round(&mut self, place: Place, argument: NodeId, node: NodeId) {
        self.whole(place, MathFunction::RoundEven, argument, node);
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

/// The [`MathFunction`] that computes `member` of `math`, for the members that are one call.
fn function(member: &str) -> Option<MathFunction> {
    Some(match member {
        "sqrt" => MathFunction::Sqrt,
        "cbrt" => MathFunction::Cbrt,
        "exp" => MathFunction::Exp,
        "exp2" => MathFunction::Exp2,
        "expm1" => MathFunction::ExpMinusOne,
        "log" => MathFunction::Log,
        "log2" => MathFunction::Log2,
        "log10" => MathFunction::Log10,
        "log1p" => MathFunction::LogOnePlus,
        "sin" => MathFunction::Sin,
        "cos" => MathFunction::Cos,
        "tan" => MathFunction::Tan,
        "asin" => MathFunction::Asin,
        "acos" => MathFunction::Acos,
        "atan" => MathFunction::Atan,
        "atan2" => MathFunction::Atan2,
        "sinh" => MathFunction::Sinh,
        "cosh" => MathFunction::Cosh,
        "tanh" => MathFunction::Tanh,
        "asinh" => MathFunction::Asinh,
        "acosh" => MathFunction::Acosh,
        "atanh" => MathFunction::Atanh,
        "erf" => MathFunction::Erf,
        "erfc" => MathFunction::Erfc,
        "gamma" => MathFunction::Gamma,
        "lgamma" => MathFunction::LogGamma,
        "fabs" => MathFunction::Fabs,
        "hypot" => MathFunction::Hypot,
        "copysign" => MathFunction::CopySign,
        "fmod" => MathFunction::TruncatedRemainder,
        "fma" => MathFunction::FusedMultiplyAdd,
        "pow" => MathFunction::Power,
        _ => return None,
    })
}

/// The float `value` as a constant of the float type `ty`; an `f32` rounds it.
fn float_constant(ty: Type<'_>, value: f64) -> Constant<'_> {
    Constant {
        ty,
        value: ConstantValue::Float(value.to_bits()),
    }
}
