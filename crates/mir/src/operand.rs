//! Operands and rvalues: the values a statement computes from.

use crate::Place;
use types::{FormatSpec, Type};

/// A value used by an rvalue, a call or a terminator.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Operand<'db> {
    /// The value at a place, which stays where it is. Only for copy types.
    Copy(Place),
    /// The value at a place, moved out. The place holds nothing until it is assigned again, and
    /// a built body's drop of it does nothing.
    Move(Place),
    /// A value known at compile time.
    Constant(Constant<'db>),
}

/// A value known at compile time, and its type.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Constant<'db> {
    /// The constant's type.
    pub ty: Type<'db>,
    /// Its value.
    pub value: ConstantValue,
}

impl<'db> Constant<'db> {
    /// The float `value` of type `ty`, which must be a float type.
    #[must_use]
    pub fn float(ty: Type<'db>, value: f64) -> Self {
        Self {
            ty,
            value: ConstantValue::Float(value.to_bits()),
        }
    }

    /// The integer `value` of type `ty`, which must be an integer type.
    #[must_use]
    pub fn integer(ty: Type<'db>, value: i128) -> Self {
        Self {
            ty,
            value: ConstantValue::Integer(value),
        }
    }
}

/// The value of a [`Constant`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstantValue {
    /// `True` or `False`.
    Bool(bool),
    /// An integer of any width. An `i128` holds every value of every integer type.
    Integer(i128),
    /// A float, as the IEEE 754 bits of its `f64` value so constants compare exactly. An `f32`
    /// constant holds a value an `f32` represents exactly.
    Float(u64),
    /// A complex number, as the `f64` bits of its real and imaginary parts.
    Complex {
        /// The real part's bits.
        real: u64,
        /// The imaginary part's bits.
        imaginary: u64,
    },
    /// A string literal.
    Str(String),
    /// `None`, of type `None` or of an optional type.
    None,
}

/// What the right-hand side of an assignment computes.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Rvalue<'db> {
    /// The operand itself.
    Use(Operand<'db>),
    /// A one-operand operation on a number or `bool`.
    Unary(UnaryOperator, Operand<'db>),
    /// A two-operand operation; both operands have the same type, and integer arithmetic wraps.
    Binary(BinaryOperator, Operand<'db>, Operand<'db>),
    /// A two-operand integer operation that also reports overflow: a `tuple[T, bool]` whose
    /// second item is `True` when the result wrapped. A [`Check`](crate::Check) on that flag
    /// follows wherever the build checks overflow, which is decision D.11's to settle.
    CheckedBinary(BinaryOperator, Operand<'db>, Operand<'db>),
    /// A number converted to another number type, as `f64(n)` writes it (decision 0014).
    Convert(Operand<'db>, Type<'db>),
    /// A value built from its parts: a struct's fields or a tuple's items in order, a `Fixed`
    /// array's elements in row-major order, or the one value of a present `T | None`.
    Aggregate(Type<'db>, Vec<Operand<'db>>),
    /// A new `Array` of the given type, with one `i64` length per dimension, outermost first,
    /// allocated on the heap. Its elements are not set: the stage that writes this rvalue writes
    /// every element before anything reads one, as array expansion does when it fills a result.
    /// Every length is zero or more; the writer checks that with an [`Assert`](crate::Check)
    /// first when it is not already known, as it does for an index. Running out of memory panics.
    /// The array is owned by the place it is assigned to, which drops it like any other array.
    NewArray(Type<'db>, Vec<Operand<'db>>),
    /// The length of one dimension of the array at the place, as an `i64`; dimension 0 is the
    /// outermost.
    Length(Place, u32),
    /// Whether the `T | None` at the place holds a value, as a `bool`.
    IsPresent(Place),
    /// Which error the error value at the place holds: its position in the callee's `Raises`
    /// list, as an `i64`.
    ErrorIndex(Place),
    /// A whole-array expression, such as `a + b * c` over arrays, kept as one tree so array
    /// fusion can see it (decision 0018). Only in [`Phase::Built`](crate::Phase) bodies.
    Elementwise(Box<Elementwise<'db>>),
    /// A new, owned `str` written from its pieces in order: an f-string such as
    /// `f"step {i}: {energy:.6f}"`, a `copy()` of a `str`, which is one piece with the default
    /// specification, or `a + b` of two. Each value is read where it is, as a borrow, and its
    /// specification has been checked against its type (decision 0022).
    Format(Vec<Piece>),
}

/// One piece of a [`Format`](Rvalue::Format): literal text, or a value written with its format
/// specification.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum Piece {
    /// Text written as it is.
    Text(String),
    /// The number, `bool` or `str` at the place, written as the specification says; the
    /// default specification writes it as `print` does.
    Value(Place, FormatSpec),
}

/// One node of a whole-array expression. Leaves are arrays, all of one shape, or scalars that
/// apply to every element; the result has the shape of the arrays.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Elementwise<'db> {
    /// An array or a scalar.
    Leaf(Operand<'db>),
    /// A one-operand operation on every element.
    Unary(UnaryOperator, Box<Elementwise<'db>>),
    /// A two-operand operation on every pair of elements.
    Binary(BinaryOperator, Box<Elementwise<'db>>, Box<Elementwise<'db>>),
}

/// A one-operand operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOperator {
    /// `-x` on a number.
    Negate,
    /// `not x` on a `bool`.
    Not,
    /// `~x` on an integer.
    BitNot,
}

/// A two-operand operation on two values of one type. The source's `and` and `or` are not here:
/// they skip their right side, so they lower to branches. Neither is `**` on floats, which lowers
/// to a call of [`MathFunction::Power`](crate::MathFunction::Power).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOperator {
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/` on floats and complex numbers. Lowering converts integers to floats first, since
    /// `/` on integers gives a float (decision 0014).
    Divide,
    /// `//`: division rounded toward negative infinity, as Python defines it.
    FloorDivide,
    /// `%`: the remainder that has the sign of the divisor, as Python defines it.
    Remainder,
    /// `&`.
    BitAnd,
    /// `|`.
    BitOr,
    /// `^`.
    BitXor,
    /// `<<`.
    ShiftLeft,
    /// `>>`: arithmetic on signed integers, logical on unsigned ones.
    ShiftRight,
    /// `==`, giving a `bool`.
    Equal,
    /// `!=`, giving a `bool`.
    NotEqual,
    /// `<`, giving a `bool`.
    Less,
    /// `<=`, giving a `bool`.
    LessEqual,
    /// `>`, giving a `bool`.
    Greater,
    /// `>=`, giving a `bool`.
    GreaterEqual,
}
