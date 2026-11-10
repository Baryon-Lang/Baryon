//! Mathematical functions of floats: the `math` functions the checker types, `x ** y` and
//! `x **= y` on floats and complex numbers, the built-in `round(x)`, and `round` of the elements of a `Fixed` array,
//! each a call of a [`Builtin::Math`] function that a backend writes itself; `floor`, `ceil` and
//! `trunc`, such a call and a conversion to `int`; `isnan`, `isinf` and `isfinite`, comparisons;
//! `degrees`, `radians` and `log(x, base)`, arithmetic; and the built-in `abs(x)`, a comparison
//! and a subtraction.
//! The operands convert to the result's float type first, as number mixing says (decision 0014),
//! so `x ** 3` on an `f64` is `pow(x, 3.0)` and `sqrt(n)` on an integer is `sqrt(f64(n))`.
//!
//! `x ** n` and `x **= n` on integers are repeated squaring in the IR, with a panic on a negative
//! exponent, as the reference's errors chapter says, and each multiplication checked for
//! overflow as decision 0030 requires.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Check, Constant, ConstantValue, MathFunction,
    Operand, Place, Rvalue, TerminatorKind,
};
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

/// The overflow check of a multiplication inside integer `**`.
const POWER: (BinaryOperator, Check<'static>) = (BinaryOperator::Multiply, Check::NoPowerOverflow);

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
    /// `nan` stays `nan`. The absolute value of the most negative integer does not fit, so
    /// the integer `0 - x` is checked as `-x` is: a debug build panics with
    /// ``integer overflow in `-` `` and a release build wraps it around to itself (decision
    /// 0030). A complex number's `abs` is its modulus, a call of `hypot` (the `complex`
    /// module).
    pub(crate) fn absolute(&mut self, place: Place, argument: NodeId, node: NodeId) {
        if self.is_complex(self.ty(argument)) {
            return self.complex_absolute(place, argument, node);
        }
        let ty = self.ty(node);
        let below = if self.is_float(ty) {
            BinaryOperator::LessEqual
        } else if self.is_integer(ty) {
            BinaryOperator::Less
        } else {
            return self.unsupported(node, "`abs` of this type");
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

    /// `place = function(operands…)`, where `place` and every operand have one float type, or
    /// for `**` on complex numbers, one complex type.
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
