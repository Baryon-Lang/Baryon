//! Rvalues: the operators, with Python's rounding for `//` and `%`, overflow-reporting
//! arithmetic, and building tuples and structs. Conversions are in the `convert` module, and
//! new arrays and their lengths in the `array` module.

use crate::Unsupported;
use crate::function::Function;
use crate::ty::Number;
use mir::{BinaryOperator, Operand, Place, Rvalue, UnaryOperator};
use types::{Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// Writes the instructions that compute `rvalue`, whose type is `ty`, and returns the value;
    /// `None` when the value is `None`.
    pub(crate) fn rvalue(
        &mut self,
        rvalue: &Rvalue<'db>,
        ty: Type<'db>,
    ) -> Result<Option<String>, Unsupported> {
        let value = match rvalue {
            Rvalue::Use(operand) => return Ok(self.operand(operand)?.map(|(value, _)| value)),
            Rvalue::Unary(operator, operand) => self.unary(*operator, operand)?,
            Rvalue::Binary(operator, left, right) => self.binary(*operator, left, right)?,
            Rvalue::CheckedBinary(operator, left, right) => {
                self.checked_binary(*operator, left, right)?
            }
            Rvalue::Convert(operand, to) => self.convert(operand, *to)?,
            Rvalue::Aggregate(_, operands) => self.aggregate(ty, operands)?,
            Rvalue::NewArray(ty, lengths) => self.new_array(*ty, lengths)?,
            Rvalue::Length(place, dimension) => self.length(place, *dimension)?,
            Rvalue::Elementwise(_) => return Err(Unsupported::new("whole-array operations")),
            Rvalue::IsPresent(place) => self.is_present(place)?,
            Rvalue::ErrorIndex(place) => self.error_index(place)?,
            Rvalue::Format(pieces) => self.format(pieces)?,
        };
        Ok(Some(value))
    }

    /// The scalar kind and LLVM type of `ty`, or what is not supported.
    pub(crate) fn scalar(&self, ty: Type<'db>) -> Result<(Number, String), Unsupported> {
        let number = self
            .types
            .number(ty)
            .ok_or_else(|| self.types.unsupported(ty))?;
        Ok((number, self.types.value(ty)?))
    }

    /// Writes `result = instruction` with a fresh name and returns the name.
    pub(crate) fn emit(&mut self, instruction: &str) -> String {
        let result = self.fresh();
        self.line(&format!("{result} = {instruction}"));
        result
    }

    /// `-x`, `not x` or `~x`.
    fn unary(
        &mut self,
        operator: UnaryOperator,
        operand: &Operand<'db>,
    ) -> Result<String, Unsupported> {
        let (value, ty) = self.value(operand)?;
        let (number, llvm) = self.scalar(ty)?;
        let instruction = match (operator, number) {
            (UnaryOperator::Negate, Number::Float(_)) => format!("fneg {llvm} {value}"),
            (UnaryOperator::Negate, Number::Integer { .. }) => format!("sub {llvm} 0, {value}"),
            (UnaryOperator::Not, Number::Bool) => format!("xor i1 {value}, true"),
            (UnaryOperator::BitNot, Number::Integer { .. }) => format!("xor {llvm} {value}, -1"),
            _ => return Err(self.types.unsupported(ty)),
        };
        Ok(self.emit(&instruction))
    }

    /// A two-operand operation; both operands have the same type.
    fn binary(
        &mut self,
        operator: BinaryOperator,
        left: &Operand<'db>,
        right: &Operand<'db>,
    ) -> Result<String, Unsupported> {
        let (a, ty) = self.value(left)?;
        let (b, _) = self.value(right)?;
        let (number, llvm) = self.scalar(ty)?;
        let simple = |instruction: &str| Some(format!("{instruction} {llvm} {a}, {b}"));
        let instruction = match (number, operator) {
            (Number::Float(_), _) => match operator {
                BinaryOperator::Add => simple("fadd"),
                BinaryOperator::Subtract => simple("fsub"),
                BinaryOperator::Multiply => simple("fmul"),
                BinaryOperator::Divide => simple("fdiv"),
                BinaryOperator::FloorDivide => return Ok(self.float_floor_divide(&llvm, &a, &b)),
                BinaryOperator::Remainder => return Ok(self.float_remainder(&llvm, &a, &b)),
                _ => float_comparison(operator)
                    .map(|condition| format!("fcmp {condition} {llvm} {a}, {b}")),
            },
            (Number::Integer { bits, signed }, _) => match operator {
                BinaryOperator::Add => simple("add"),
                BinaryOperator::Subtract => simple("sub"),
                BinaryOperator::Multiply => simple("mul"),
                BinaryOperator::FloorDivide if signed => {
                    return Ok(self.signed_floor_divide(&llvm, &a, &b));
                }
                BinaryOperator::Remainder if signed => {
                    return Ok(self.signed_remainder(&llvm, &a, &b));
                }
                BinaryOperator::FloorDivide => simple("udiv"),
                BinaryOperator::Remainder => simple("urem"),
                BinaryOperator::ShiftLeft | BinaryOperator::ShiftRight => {
                    return Ok(self.shift(operator, &llvm, bits, signed, &a, &b));
                }
                _ => bitwise(operator)
                    .map(|instruction| format!("{instruction} {llvm} {a}, {b}"))
                    .or_else(|| {
                        integer_comparison(operator, signed)
                            .map(|condition| format!("icmp {condition} {llvm} {a}, {b}"))
                    }),
            },
            (Number::Bool, _) => bitwise(operator)
                .map(|instruction| format!("{instruction} i1 {a}, {b}"))
                .or_else(|| match operator {
                    BinaryOperator::Equal => Some(format!("icmp eq i1 {a}, {b}")),
                    BinaryOperator::NotEqual => Some(format!("icmp ne i1 {a}, {b}")),
                    _ => None,
                }),
        };
        let instruction = instruction.ok_or_else(|| {
            Unsupported::new(format!(
                "`{operator:?}` on values of type `{}`",
                ty.display(self.types.db)
            ))
        })?;
        Ok(self.emit(&instruction))
    }

    /// `a // b` on signed integers, rounded toward negative infinity. Dividing by `-1` is
    /// computed as a negation, which wraps, because `sdiv` of the most negative value by `-1`
    /// is undefined in LLVM.
    fn signed_floor_divide(&mut self, llvm: &str, a: &str, b: &str) -> String {
        let Division {
            minus_one,
            quotient,
            adjust,
            ..
        } = self.signed_division(llvm, a, b);
        let adjust = self.emit(&format!("zext i1 {adjust} to {llvm}"));
        let floored = self.emit(&format!("sub {llvm} {quotient}, {adjust}"));
        let negated = self.emit(&format!("sub {llvm} 0, {a}"));
        self.emit(&format!(
            "select i1 {minus_one}, {llvm} {negated}, {llvm} {floored}"
        ))
    }

    /// `a % b` on signed integers, with the sign of `b`.
    fn signed_remainder(&mut self, llvm: &str, a: &str, b: &str) -> String {
        let Division {
            remainder, adjust, ..
        } = self.signed_division(llvm, a, b);
        let added = self.emit(&format!("add {llvm} {remainder}, {b}"));
        self.emit(&format!(
            "select i1 {adjust}, {llvm} {added}, {llvm} {remainder}"
        ))
    }

    /// The shared part of signed `//` and `%`.
    fn signed_division(&mut self, llvm: &str, a: &str, b: &str) -> Division {
        let minus_one = self.emit(&format!("icmp eq {llvm} {b}, -1"));
        let divisor = self.emit(&format!("select i1 {minus_one}, {llvm} 1, {llvm} {b}"));
        let quotient = self.emit(&format!("sdiv {llvm} {a}, {divisor}"));
        let remainder = self.emit(&format!("srem {llvm} {a}, {divisor}"));
        let nonzero = self.emit(&format!("icmp ne {llvm} {remainder}, 0"));
        let remainder_negative = self.emit(&format!("icmp slt {llvm} {remainder}, 0"));
        let divisor_negative = self.emit(&format!("icmp slt {llvm} {b}, 0"));
        let differ = self.emit(&format!("xor i1 {remainder_negative}, {divisor_negative}"));
        let adjust = self.emit(&format!("and i1 {nonzero}, {differ}"));
        Division {
            minus_one,
            quotient,
            remainder,
            adjust,
        }
    }

    /// `a << b` or `a >> b`. A shift by the width or more, where LLVM's result is undefined,
    /// gives what shifting one bit at a time would: zero, or for `>>` of a negative signed
    /// integer, `-1`. The amount is read as unsigned.
    fn shift(
        &mut self,
        operator: BinaryOperator,
        llvm: &str,
        bits: u32,
        signed: bool,
        a: &str,
        b: &str,
    ) -> String {
        let large = self.emit(&format!("icmp uge {llvm} {b}, {bits}"));
        if operator == BinaryOperator::ShiftRight && signed {
            let amount = self.emit(&format!(
                "select i1 {large}, {llvm} {}, {llvm} {b}",
                bits - 1
            ));
            return self.emit(&format!("ashr {llvm} {a}, {amount}"));
        }
        let instruction = if operator == BinaryOperator::ShiftLeft {
            "shl"
        } else {
            "lshr"
        };
        let amount = self.emit(&format!("select i1 {large}, {llvm} 0, {llvm} {b}"));
        let shifted = self.emit(&format!("{instruction} {llvm} {a}, {amount}"));
        self.emit(&format!("select i1 {large}, {llvm} 0, {llvm} {shifted}"))
    }

    /// `a % b` on floats as Python computes it: `fmod`, moved by `b` when its sign differs
    /// from `b`'s, and a zero result carrying `b`'s sign.
    fn float_remainder(&mut self, llvm: &str, a: &str, b: &str) -> String {
        let (remainder, nonzero, adjust) = self.float_division(llvm, a, b);
        let added = self.emit(&format!("fadd {llvm} {remainder}, {b}"));
        let moved = self.emit(&format!(
            "select i1 {adjust}, {llvm} {added}, {llvm} {remainder}"
        ));
        let zero = self.copysign(llvm, "0.0", b);
        self.emit(&format!(
            "select i1 {nonzero}, {llvm} {moved}, {llvm} {zero}"
        ))
    }

    /// `a // b` on floats as Python computes it: `(a - fmod(a, b)) / b`, less one when the
    /// remainder's sign differs from `b`'s, rounded to the nearest whole number; a zero result
    /// carries the sign of `a / b`.
    fn float_floor_divide(&mut self, llvm: &str, a: &str, b: &str) -> String {
        let (remainder, _, adjust) = self.float_division(llvm, a, b);
        let exact = self.emit(&format!("fsub {llvm} {a}, {remainder}"));
        let divided = self.emit(&format!("fdiv {llvm} {exact}, {b}"));
        let lowered = self.emit(&format!("fsub {llvm} {divided}, 1.0"));
        let quotient = self.emit(&format!(
            "select i1 {adjust}, {llvm} {lowered}, {llvm} {divided}"
        ));
        let suffix = intrinsic_suffix(llvm);
        let floor = self.emit(&format!(
            "call {llvm} @llvm.floor.{suffix}({llvm} {quotient})"
        ));
        self.declare(
            &format!("llvm.floor.{suffix}"),
            &format!("declare {llvm} @llvm.floor.{suffix}({llvm})"),
        );
        let fraction = self.emit(&format!("fsub {llvm} {quotient}, {floor}"));
        let round_up = self.emit(&format!("fcmp ogt {llvm} {fraction}, 0.5"));
        let raised = self.emit(&format!("fadd {llvm} {floor}, 1.0"));
        let rounded = self.emit(&format!(
            "select i1 {round_up}, {llvm} {raised}, {llvm} {floor}"
        ));
        let nonzero = self.emit(&format!("fcmp une {llvm} {quotient}, 0.0"));
        let true_quotient = self.emit(&format!("fdiv {llvm} {a}, {b}"));
        let zero = self.copysign(llvm, "0.0", &true_quotient);
        self.emit(&format!(
            "select i1 {nonzero}, {llvm} {rounded}, {llvm} {zero}"
        ))
    }

    /// The shared part of float `//` and `%`: `fmod(a, b)`, whether it is not zero, and whether
    /// it is not zero with a sign that differs from `b`'s.
    fn float_division(&mut self, llvm: &str, a: &str, b: &str) -> (String, String, String) {
        let remainder = self.emit(&format!("frem {llvm} {a}, {b}"));
        let nonzero = self.emit(&format!("fcmp une {llvm} {remainder}, 0.0"));
        let remainder_negative = self.emit(&format!("fcmp olt {llvm} {remainder}, 0.0"));
        let divisor_negative = self.emit(&format!("fcmp olt {llvm} {b}, 0.0"));
        let differ = self.emit(&format!("xor i1 {remainder_negative}, {divisor_negative}"));
        let adjust = self.emit(&format!("and i1 {nonzero}, {differ}"));
        (remainder, nonzero, adjust)
    }

    /// `magnitude` with the sign of `sign`, through `llvm.copysign`.
    fn copysign(&mut self, llvm: &str, magnitude: &str, sign: &str) -> String {
        let suffix = intrinsic_suffix(llvm);
        self.declare(
            &format!("llvm.copysign.{suffix}"),
            &format!("declare {llvm} @llvm.copysign.{suffix}({llvm}, {llvm})"),
        );
        self.emit(&format!(
            "call {llvm} @llvm.copysign.{suffix}({llvm} {magnitude}, {llvm} {sign})"
        ))
    }

    /// `+`, `-` or `*` on integers that also reports overflow, as LLVM's `with.overflow`
    /// intrinsics do: a `{ iN, i1 }`, the layout of `tuple[T, bool]`.
    fn checked_binary(
        &mut self,
        operator: BinaryOperator,
        left: &Operand<'db>,
        right: &Operand<'db>,
    ) -> Result<String, Unsupported> {
        let (a, ty) = self.value(left)?;
        let (b, _) = self.value(right)?;
        let (Number::Integer { signed, .. }, llvm) = self.scalar(ty)? else {
            return Err(self.types.unsupported(ty));
        };
        let operation = match operator {
            BinaryOperator::Add => "add",
            BinaryOperator::Subtract => "sub",
            BinaryOperator::Multiply => "mul",
            _ => return Err(Unsupported::new(format!("checked `{operator:?}`"))),
        };
        let sign = if signed { 's' } else { 'u' };
        let name = format!("llvm.{sign}{operation}.with.overflow.{llvm}");
        self.declare(
            &name,
            &format!("declare {{ {llvm}, i1 }} @{name}({llvm}, {llvm})"),
        );
        Ok(self.emit(&format!(
            "call {{ {llvm}, i1 }} @{name}({llvm} {a}, {llvm} {b})"
        )))
    }

    /// A tuple or struct of type `ty` built from `operands`, item by item with `insertvalue`.
    fn aggregate(
        &mut self,
        ty: Type<'db>,
        operands: &[Operand<'db>],
    ) -> Result<String, Unsupported> {
        let llvm = self.types.value(ty)?;
        // A `Fixed` array's operands are its elements in row-major order; each goes at the
        // indices that position unravels to. A tuple's or struct's go at their own position.
        let lengths: Vec<u64> = match ty.kind(self.types.db) {
            TypeKind::Fixed { shape, .. } => shape.lengths().to_vec(),
            _ if llvm.starts_with('{') => Vec::new(),
            _ => return Err(self.types.unsupported(ty)),
        };
        let mut built = "poison".to_owned();
        for (position, operand) in operands.iter().enumerate() {
            let (value, item) = self.value(operand)?;
            let item = self.types.value(item)?;
            let indices = if lengths.is_empty() {
                position.to_string()
            } else {
                unravel(position as u64, &lengths)
            };
            built = self.emit(&format!(
                "insertvalue {llvm} {built}, {item} {value}, {indices}"
            ));
        }
        // A present `T | None` is its value with the flag set.
        if matches!(ty.kind(self.types.db), TypeKind::Optional(_)) {
            built = self.emit(&format!("insertvalue {llvm} {built}, i1 true, 1"));
        }
        Ok(built)
    }

    /// Whether the `T | None` at `place` holds a value: its flag.
    fn is_present(&mut self, place: &Place) -> Result<String, Unsupported> {
        let (address, ty) = self.place(place)?;
        let llvm = self.types.value(ty)?;
        let flag = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 1"
        ));
        Ok(self.emit(&format!("load i1, ptr {flag}")))
    }
}

/// The row-major indices of the element at flat `position` in an array of `lengths`, as an
/// LLVM index list: position 3 of a 2 by 2 array is `1, 1`.
fn unravel(mut position: u64, lengths: &[u64]) -> String {
    let mut indices = vec![0; lengths.len()];
    for (index, &length) in indices.iter_mut().zip(lengths).rev() {
        *index = position % length.max(1);
        position /= length.max(1);
    }
    indices
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The values signed `//` and `%` share, each the name of an LLVM value.
struct Division {
    /// Whether `b` is `-1`, where `a` is divided by 1 instead to avoid `sdiv`'s overflow.
    minus_one: String,
    /// `a` divided by `b`, rounded toward zero.
    quotient: String,
    /// What is left after that division, with the sign of `a`.
    remainder: String,
    /// Whether rounding toward negative infinity moves the result: the remainder is not zero
    /// and its sign differs from `b`'s.
    adjust: String,
}

/// The suffix LLVM's float intrinsics take for `llvm`: `f32` or `f64`.
fn intrinsic_suffix(llvm: &str) -> &'static str {
    if llvm == "float" { "f32" } else { "f64" }
}

/// The LLVM instruction for a bitwise operator on integers or `bool`.
fn bitwise(operator: BinaryOperator) -> Option<&'static str> {
    match operator {
        BinaryOperator::BitAnd => Some("and"),
        BinaryOperator::BitOr => Some("or"),
        BinaryOperator::BitXor => Some("xor"),
        _ => None,
    }
}

/// The `icmp` condition for a comparison of integers.
fn integer_comparison(operator: BinaryOperator, signed: bool) -> Option<&'static str> {
    Some(match (operator, signed) {
        (BinaryOperator::Equal, _) => "eq",
        (BinaryOperator::NotEqual, _) => "ne",
        (BinaryOperator::Less, true) => "slt",
        (BinaryOperator::Less, false) => "ult",
        (BinaryOperator::LessEqual, true) => "sle",
        (BinaryOperator::LessEqual, false) => "ule",
        (BinaryOperator::Greater, true) => "sgt",
        (BinaryOperator::Greater, false) => "ugt",
        (BinaryOperator::GreaterEqual, true) => "sge",
        (BinaryOperator::GreaterEqual, false) => "uge",
        _ => return None,
    })
}

/// The `fcmp` condition for a comparison of floats. Every comparison with NaN is false except
/// `!=`, as in Python, so `!=` is the unordered `une` and the rest are ordered.
fn float_comparison(operator: BinaryOperator) -> Option<&'static str> {
    Some(match operator {
        BinaryOperator::Equal => "oeq",
        BinaryOperator::NotEqual => "une",
        BinaryOperator::Less => "olt",
        BinaryOperator::LessEqual => "ole",
        BinaryOperator::Greater => "ogt",
        BinaryOperator::GreaterEqual => "oge",
        _ => return None,
    })
}
