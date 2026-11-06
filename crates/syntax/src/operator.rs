//! The operator a node applies: `+` in `a + b`, `not` in `not done`, `<` in `a < b`.

use std::fmt;

/// The operator stored on an operator node.
///
/// Only these kinds carry an operator; every other node holds [`Operator::None`]:
///
/// | Kind | Operators |
/// | --- | --- |
/// | [`BinaryOperation`](crate::SyntaxKind::BinaryOperation), [`AugmentedAssign`](crate::SyntaxKind::AugmentedAssign) | arithmetic and bitwise, `Add` to `BitAnd` |
/// | [`BooleanOperation`](crate::SyntaxKind::BooleanOperation) | `And`, `Or` |
/// | [`UnaryOperation`](crate::SyntaxKind::UnaryOperation) | `Not`, `Negate`, `Plus`, `Invert` |
/// | [`Comparator`](crate::SyntaxKind::Comparator) | `Equal` to `NotIn` |
///
/// ```
/// use syntax::Operator;
///
/// assert_eq!(Operator::FloorDivide.symbol(), "//");
/// assert_eq!(Operator::NotIn.to_string(), "not in");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Operator {
    /// The node applies no operator.
    None,
    /// `+` between two operands.
    Add,
    /// `-` between two operands.
    Subtract,
    /// `*`.
    Multiply,
    /// `@`, matrix multiplication.
    MatrixMultiply,
    /// `/`, which always gives a float (decision 0009).
    Divide,
    /// `//`, division rounded toward negative infinity.
    FloorDivide,
    /// `%`.
    Modulo,
    /// `**`.
    Power,
    /// `<<`.
    LeftShift,
    /// `>>`.
    RightShift,
    /// `|`.
    BitOr,
    /// `^`.
    BitXor,
    /// `&`.
    BitAnd,
    /// `and`.
    And,
    /// `or`.
    Or,
    /// `not`.
    Not,
    /// `-` before one operand.
    Negate,
    /// `+` before one operand.
    Plus,
    /// `~`, bitwise inversion.
    Invert,
    /// `==`.
    Equal,
    /// `!=`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,
    /// `is`.
    Is,
    /// `is not`.
    IsNot,
    /// `in`.
    In,
    /// `not in`.
    NotIn,
}

impl Operator {
    /// The operator as it is written in source. Empty for [`Operator::None`].
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Add | Self::Plus => "+",
            Self::Subtract | Self::Negate => "-",
            Self::Multiply => "*",
            Self::MatrixMultiply => "@",
            Self::Divide => "/",
            Self::FloorDivide => "//",
            Self::Modulo => "%",
            Self::Power => "**",
            Self::LeftShift => "<<",
            Self::RightShift => ">>",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::BitAnd => "&",
            Self::And => "and",
            Self::Or => "or",
            Self::Not => "not",
            Self::Invert => "~",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::In => "in",
            Self::NotIn => "not in",
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.symbol())
    }
}
