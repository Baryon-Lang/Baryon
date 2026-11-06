//! Constant evaluation: computing the value of a `Final` constant, an array's rank or a
//! `Fixed` shape while compiling.
//!
//! A constant is built from number literals, other constants and the arithmetic operators
//! (language reference, "Types", "Constants"). The arithmetic follows the rules the program
//! follows when it runs: `//` rounds toward negative infinity and `%` takes the sign of the
//! divisor, as in Python; `/` on two integers gives a float; an integer and a float give a float;
//! an integer power needs a non-negative exponent. Integers are 64-bit, and a result that does
//! not fit is an error rather than a wrapped number.

use crate::lower::Lowerer;
use crate::outline::{Kind, outline};
use database::Definition;
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Declaration, Item, Value};

/// Why an operation on two constants has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The result does not fit in an `i64`.
    Overflow,
    /// An integer divided by zero.
    DivisionByZero,
    /// An integer raised to a negative power.
    NegativeExponent,
    /// The operator is not arithmetic, such as `&` or `@`.
    NotArithmetic,
}

impl Lowerer<'_> {
    /// The value of a constant expression, or `None` after reporting why it has none. `None`
    /// without a new error when the expression refers to a constant that already failed.
    pub(crate) fn evaluate(&mut self, node: NodeId) -> Option<Value> {
        match self.tree.kind(node) {
            SyntaxKind::Integer => self.integer(node),
            SyntaxKind::Float => self.float(node),
            SyntaxKind::Name | SyntaxKind::Attribute => self.constant(node),
            SyntaxKind::UnaryOperation => {
                let operand = self.tree.child(node, Role::Value)?;
                let value = self.evaluate(operand)?;
                match self.tree.node(node).operator() {
                    Operator::Plus => Some(value),
                    Operator::Negate => {
                        let negated = negate(value);
                        if negated.is_none() {
                            self.overflow(node);
                        }
                        negated
                    }
                    _ => self.not_constant(node),
                }
            }
            SyntaxKind::BinaryOperation => {
                let left = self.tree.child(node, Role::Left)?;
                let right = self.tree.child(node, Role::Right)?;
                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;
                match combine(self.tree.node(node).operator(), left, right) {
                    Ok(value) => Some(value),
                    Err(Failure::Overflow) => self.overflow(node),
                    Err(Failure::DivisionByZero) => {
                        self.error(
                            node,
                            "division by zero in a constant",
                            "divides by zero",
                            None,
                        );
                        None
                    }
                    Err(Failure::NegativeExponent) => {
                        self.error(
                            node,
                            "an integer power needs an exponent of 0 or more",
                            "negative exponent",
                            Some("make the base a float for a fractional result: `2.0 ** -1`"),
                        );
                        None
                    }
                    Err(Failure::NotArithmetic) => self.not_constant(node),
                }
            }
            SyntaxKind::Missing => None,
            _ => self.not_constant(node),
        }
    }

    /// The value of a constant expression, or `None`, without reporting why it has none: for
    /// places where a non-constant is allowed, such as a tuple index.
    pub(crate) fn evaluate_quietly(&mut self, node: NodeId) -> Option<Value> {
        let reported = self.diagnostics.len();
        let value = self.evaluate(node);
        self.diagnostics.truncate(reported);
        value
    }

    /// The value of an integer literal such as `64`, `100_000` or `0xff`.
    fn integer(&mut self, node: NodeId) -> Option<Value> {
        let written = self.tree.text(node, self.text);
        let digits = written.replace('_', "");
        let lowered = digits.to_ascii_lowercase();
        let (radix, digits) = [("0x", 16), ("0o", 8), ("0b", 2)]
            .into_iter()
            .find_map(|(prefix, radix)| lowered.strip_prefix(prefix).map(|rest| (radix, rest)))
            .unwrap_or((10, lowered.as_str()));
        if let Ok(value) = i64::from_str_radix(digits, radix) {
            return Some(Value::Integer(value));
        }
        self.error(
            node,
            "this integer does not fit in `i64`",
            "too large",
            Some("integers are 64-bit, from -2^63 to 2^63 - 1"),
        );
        None
    }

    /// The value of a float literal such as `0.1` or `6.37e6`.
    fn float(&mut self, node: NodeId) -> Option<Value> {
        let written = self.tree.text(node, self.text).replace('_', "");
        match written.parse::<f64>() {
            Ok(value) => Some(Value::Float(value.to_bits())),
            Err(_) => self.not_constant(node),
        }
    }

    /// The value of the constant a name refers to.
    fn constant(&mut self, node: NodeId) -> Option<Value> {
        let written = self.tree.text(node, self.text);
        let (file, name) = match self.definition(node) {
            None | Some(Definition::Standard { .. }) => return None,
            Some(&Definition::Item(file, name)) => (file, name),
            Some(_) => {
                let message = format!("`{written}` is not a constant");
                self.error(node, &message, "used in a constant expression", None);
                return None;
            }
        };
        let is_constant = outline(self.db, file)
            .get(name)
            .is_some_and(|entry| entry.kind == Kind::Constant);
        if !is_constant {
            let message = format!("`{written}` is not a constant");
            self.error(node, &message, "used in a constant expression", None);
            return None;
        }
        let item = Item::new(self.db, self.package, file, name);
        match self.db.declaration(item).declaration {
            Declaration::Constant { value, .. } => value,
            _ => None,
        }
    }

    /// Reports an expression the compiler cannot compute.
    fn not_constant(&mut self, node: NodeId) -> Option<Value> {
        self.error(
            node,
            "the compiler cannot compute this value",
            "not a constant expression",
            Some("a constant is built from number literals, other constants and arithmetic"),
        );
        None
    }

    /// Reports a result too large for an `i64`.
    fn overflow(&mut self, node: NodeId) -> Option<Value> {
        self.error(
            node,
            "this constant does not fit in `i64`",
            "overflows",
            Some("integers are 64-bit; use a float constant for larger numbers"),
        );
        None
    }
}

/// `-value`, or `None` if it overflows, as `-i64::MIN` does.
fn negate(value: Value) -> Option<Value> {
    match value {
        Value::Integer(value) => value.checked_neg().map(Value::Integer),
        Value::Float(bits) => Some(float(-f64::from_bits(bits))),
    }
}

/// `left operator right`, with the language's arithmetic rules.
pub(crate) fn combine(operator: Operator, left: Value, right: Value) -> Result<Value, Failure> {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => integers(operator, left, right),
        _ => floats(operator, as_float(left), as_float(right)),
    }
}

/// An operation on two integers.
fn integers(operator: Operator, left: i64, right: i64) -> Result<Value, Failure> {
    let integer = |value: Option<i64>| value.map(Value::Integer).ok_or(Failure::Overflow);
    match operator {
        Operator::Add => integer(left.checked_add(right)),
        Operator::Subtract => integer(left.checked_sub(right)),
        Operator::Multiply => integer(left.checked_mul(right)),
        Operator::Divide | Operator::FloorDivide | Operator::Modulo if right == 0 => {
            Err(Failure::DivisionByZero)
        }
        Operator::Divide => Ok(float(to_float(left) / to_float(right))),
        Operator::FloorDivide => {
            let quotient = left.checked_div(right).ok_or(Failure::Overflow)?;
            let inexact = left % right != 0;
            integer(Some(
                quotient - i64::from(inexact && (left < 0) != (right < 0)),
            ))
        }
        Operator::Modulo => {
            let remainder = left.checked_rem(right).ok_or(Failure::Overflow)?;
            let adjust = remainder != 0 && (remainder < 0) != (right < 0);
            integer(Some(if adjust { remainder + right } else { remainder }))
        }
        Operator::Power if right < 0 => Err(Failure::NegativeExponent),
        Operator::Power => {
            let exponent = u32::try_from(right).map_err(|_| Failure::Overflow)?;
            integer(left.checked_pow(exponent))
        }
        _ => Err(Failure::NotArithmetic),
    }
}

/// An operation on two floats, IEEE 754: dividing by zero gives an infinity or NaN.
fn floats(operator: Operator, left: f64, right: f64) -> Result<Value, Failure> {
    Ok(float(match operator {
        Operator::Add => left + right,
        Operator::Subtract => left - right,
        Operator::Multiply => left * right,
        Operator::Divide => left / right,
        Operator::FloorDivide => (left / right).floor(),
        Operator::Modulo => {
            let remainder = left % right;
            if remainder != 0.0 && (remainder < 0.0) != (right < 0.0) {
                remainder + right
            } else {
                remainder
            }
        }
        Operator::Power => left.powf(right),
        _ => return Err(Failure::NotArithmetic),
    }))
}

/// A float value.
fn float(value: f64) -> Value {
    Value::Float(value.to_bits())
}

/// A value as a float, converting an integer.
fn as_float(value: Value) -> f64 {
    match value {
        Value::Integer(value) => to_float(value),
        Value::Float(bits) => f64::from_bits(bits),
    }
}

/// The float nearest an integer, as Python's `float(n)` and the language's `f64(n)` give.
#[expect(
    clippy::cast_precision_loss,
    reason = "integers beyond 2^53 round to the nearest float, as the language specifies"
)]
pub(crate) fn to_float(value: i64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integer(operator: Operator, left: i64, right: i64) -> Result<Value, Failure> {
        combine(operator, Value::Integer(left), Value::Integer(right))
    }

    #[test]
    fn floor_division_and_modulo_follow_python() {
        assert_eq!(
            integer(Operator::FloorDivide, -7, 2),
            Ok(Value::Integer(-4))
        );
        assert_eq!(integer(Operator::Modulo, -7, 2), Ok(Value::Integer(1)));
        assert_eq!(integer(Operator::Modulo, 7, -2), Ok(Value::Integer(-1)));
        assert_eq!(integer(Operator::FloorDivide, 7, 2), Ok(Value::Integer(3)));
        assert_eq!(
            combine(Operator::Modulo, float(-7.0), Value::Integer(2)),
            Ok(float(1.0))
        );
    }

    #[test]
    fn integer_division_gives_a_float() {
        assert_eq!(integer(Operator::Divide, 8, 3), Ok(float(8.0 / 3.0)));
    }

    #[test]
    fn overflow_and_bad_operands_are_failures() {
        assert_eq!(
            integer(Operator::Multiply, i64::MAX, 2),
            Err(Failure::Overflow)
        );
        assert_eq!(
            integer(Operator::FloorDivide, i64::MIN, -1),
            Err(Failure::Overflow)
        );
        assert_eq!(
            integer(Operator::Modulo, 1, 0),
            Err(Failure::DivisionByZero)
        );
        assert_eq!(
            integer(Operator::Power, 2, -1),
            Err(Failure::NegativeExponent)
        );
        assert_eq!(integer(Operator::Power, 2, 10), Ok(Value::Integer(1024)));
        assert_eq!(integer(Operator::BitAnd, 2, 1), Err(Failure::NotArithmetic));
        assert_eq!(negate(Value::Integer(i64::MIN)), None);
    }

    #[test]
    fn a_float_divided_by_zero_is_infinite() {
        assert_eq!(
            combine(Operator::Divide, float(1.0), Value::Integer(0)),
            Ok(float(f64::INFINITY))
        );
    }
}
