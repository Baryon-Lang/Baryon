//! Number mixing: the type of `a op b` when both are numbers (language reference, "Types",
//! "Number mixing", and decision 0014).
//!
//! 1. Two operands of the same type give that type, except that `/` on two integers gives an
//!    `f64`.
//! 2. An integer and a float give the float.
//! 3. An integer or a float and a complex number give the complex type, when the float is as
//!    wide as the complex number's parts.
//! 4. Anything else is an error: floats or integers of different widths, signed with unsigned,
//!    and `bool` in arithmetic. The message suggests a conversion, which is a call to the target
//!    type, such as `f64(n)`.

use syntax::Operator;
use types::{Float, Integer, Primitive};

/// The result of an arithmetic operator on two numbers, or `None` when they do not mix.
///
/// `bool` is not a number here: `True + 1` is an error, as a sum of truth values usually is a
/// bug in numeric code.
pub(crate) fn arithmetic(
    operator: Operator,
    left: Primitive,
    right: Primitive,
) -> Option<Primitive> {
    let mixed = common(left, right)?;
    Some(match (operator, mixed) {
        (Operator::Divide, Primitive::Integer(_)) => Primitive::Float(Float::F64),
        _ => mixed,
    })
}

/// The type two numbers convert to before they are combined or compared, or `None` when they do
/// not mix.
pub(crate) fn common(left: Primitive, right: Primitive) -> Option<Primitive> {
    use Primitive::{Bool, Complex as C, Float as F, Integer as I};
    match (left, right) {
        (Bool, _) | (_, Bool) => None,
        _ if left == right => Some(left),
        (I(_), F(float)) | (F(float), I(_)) => Some(F(float)),
        (I(_), C(complex)) | (C(complex), I(_)) => Some(C(complex)),
        (F(float), C(complex)) | (C(complex), F(float)) if complex.part() == float => {
            Some(C(complex))
        }
        _ => None,
    }
}

/// The conversion to suggest when two numbers do not mix: the wider or more general of the two,
/// written as a call, such as `f64`.
pub(crate) fn suggestion(left: Primitive, right: Primitive) -> Option<Primitive> {
    let rank = |primitive: Primitive| match primitive {
        Primitive::Bool => (0, 0),
        Primitive::Integer(integer) => (1, integer.bits()),
        Primitive::Float(float) => (2, float.bits()),
        Primitive::Complex(complex) => (3, complex.part().bits()),
    };
    let wider = if rank(left) >= rank(right) {
        left
    } else {
        right
    };
    match wider {
        Primitive::Bool => None,
        Primitive::Integer(integer) if integer.is_signed() => Some(wider),
        // Signed with unsigned: the signed 64-bit type holds both in most programs.
        Primitive::Integer(_) => Some(Primitive::Integer(Integer::I64)),
        _ => Some(wider),
    }
}

/// Whether `primitive` is a number: an integer, a float or a complex number.
pub(crate) fn is_number(primitive: Primitive) -> bool {
    !matches!(primitive, Primitive::Bool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::Complex;

    const I32: Primitive = Primitive::Integer(Integer::I32);
    const I64: Primitive = Primitive::Integer(Integer::I64);
    const U8: Primitive = Primitive::Integer(Integer::U8);
    const F32: Primitive = Primitive::Float(Float::F32);
    const F64: Primitive = Primitive::Float(Float::F64);
    const C64: Primitive = Primitive::Complex(Complex::C64);
    const C128: Primitive = Primitive::Complex(Complex::C128);

    #[test]
    fn the_four_rules() {
        assert_eq!(arithmetic(Operator::Add, F32, F32), Some(F32));
        assert_eq!(arithmetic(Operator::Divide, I32, I32), Some(F64));
        assert_eq!(arithmetic(Operator::FloorDivide, I32, I32), Some(I32));
        assert_eq!(arithmetic(Operator::Multiply, I64, F32), Some(F32));
        assert_eq!(arithmetic(Operator::Add, F64, C128), Some(C128));
        assert_eq!(arithmetic(Operator::Add, I64, C64), Some(C64));
        assert_eq!(arithmetic(Operator::Add, F64, C64), None);
        assert_eq!(arithmetic(Operator::Add, F32, F64), None);
        assert_eq!(arithmetic(Operator::Add, I32, I64), None);
        assert_eq!(arithmetic(Operator::Add, I64, U8), None);
        assert_eq!(arithmetic(Operator::Add, Primitive::Bool, I64), None);
    }

    #[test]
    fn suggestions_name_the_wider_type() {
        assert_eq!(suggestion(F32, F64), Some(F64));
        assert_eq!(suggestion(I32, I64), Some(I64));
        assert_eq!(suggestion(U8, I32), Some(I32));
        assert_eq!(suggestion(Primitive::Integer(Integer::U64), I32), Some(I64));
        assert!(!is_number(Primitive::Bool));
    }
}
