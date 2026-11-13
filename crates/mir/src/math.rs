//! The [`MathFunction`]s a [`Builtin::Math`](crate::Builtin::Math) call computes: the float
//! functions of Python's `math` module, which the backends build inline or call the C library
//! for. Split from the `call` module, which holds the rest of what a call calls.

/// A mathematical function of floats, called by [`Builtin::Math`](crate::Builtin::Math). Each
/// gives the result IEEE 754 and the C library give for its argument type; none of them fails or
/// panics. NaN and the infinities pass through as C's functions define, so `sqrt(-1.0)` is NaN,
/// as `numpy.sqrt` gives, where Python's `math.sqrt` raises.
///
/// These are the functions of Python's `math` module that take and return floats (roadmap
/// 1.13.1). `math.floor`, `math.ceil` and `math.trunc` return an integer in Python, so each is a
/// call of [`Floor`](Self::Floor), [`Ceil`](Self::Ceil) or [`Truncate`](Self::Truncate) and then
/// a conversion. `isnan`, `isinf`, `isfinite`, `degrees` and `radians` need no function: lowering
/// writes them as comparisons and arithmetic. Functions of integers alone, such as `gcd`, are not
/// here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum MathFunction {
    /// `math.sqrt(x)`: the square root, correctly rounded.
    Sqrt,
    /// `math.cbrt(x)`: the cube root, negative for a negative `x`.
    Cbrt,
    /// `math.exp(x)`: e to the power `x`.
    Exp,
    /// `math.exp2(x)`: 2 to the power `x`.
    Exp2,
    /// `math.expm1(x)`: `exp(x) - 1`, accurate when `x` is close to zero.
    ExpMinusOne,
    /// `math.log(x)`: the natural logarithm.
    Log,
    /// `math.log2(x)`: the base-2 logarithm.
    Log2,
    /// `math.log10(x)`: the base-10 logarithm.
    Log10,
    /// `math.log1p(x)`: `log(1 + x)`, accurate when `x` is close to zero.
    LogOnePlus,
    /// `math.sin(x)`, with `x` in radians.
    Sin,
    /// `math.cos(x)`, with `x` in radians.
    Cos,
    /// `math.tan(x)`, with `x` in radians.
    Tan,
    /// `math.asin(x)`: the arc sine in radians, in `[-pi/2, pi/2]`.
    Asin,
    /// `math.acos(x)`: the arc cosine in radians, in `[0, pi]`.
    Acos,
    /// `math.atan(x)`: the arc tangent in radians, in `[-pi/2, pi/2]`.
    Atan,
    /// `math.atan2(y, x)`: the angle of the point `(x, y)` in radians, in `[-pi, pi]`; two
    /// arguments, `y` first, as in Python and C.
    Atan2,
    /// `math.sinh(x)`: the hyperbolic sine.
    Sinh,
    /// `math.cosh(x)`: the hyperbolic cosine.
    Cosh,
    /// `math.tanh(x)`: the hyperbolic tangent.
    Tanh,
    /// `math.asinh(x)`: the inverse hyperbolic sine.
    Asinh,
    /// `math.acosh(x)`: the inverse hyperbolic cosine.
    Acosh,
    /// `math.atanh(x)`: the inverse hyperbolic tangent.
    Atanh,
    /// `math.erf(x)`: the error function.
    Erf,
    /// `math.erfc(x)`: `1 - erf(x)`, accurate when `erf(x)` is close to 1.
    Erfc,
    /// `math.gamma(x)`: the gamma function, C's `tgamma`.
    Gamma,
    /// `math.lgamma(x)`: the natural logarithm of the absolute value of the gamma function. It
    /// must not write C's global `signgam`, which a `prange` loop would race on.
    LogGamma,
    /// `math.fabs(x)`: the absolute value, always a float.
    Fabs,
    /// `math.hypot(x, y)`: `sqrt(x * x + y * y)` without overflow or underflow in between; two
    /// arguments. Python's `hypot` of more than two arguments is not here. `abs` of a complex
    /// number is `hypot` of its parts, as Python computes it.
    Hypot,
    /// `math.copysign(x, y)`: the size of `x` with the sign of `y`; two arguments.
    CopySign,
    /// `math.fmod(x, y)`: the remainder of `x / y` truncated toward zero, with the sign of `x`,
    /// as C's `fmod`; two arguments. It differs from `%`, which floors as Python does.
    TruncatedRemainder,
    /// `math.fma(x, y, z)`: `x * y + z` rounded once; three arguments.
    FusedMultiplyAdd,
    /// `x ** y` on floats, and `math.pow(x, y)`: two arguments, the base then the exponent.
    ///
    /// Also `x ** y` on two complex numbers of one type, `complex64` or `complex128`, computed
    /// as Python's `complex.__pow__` computes it: by repeated squaring for a whole exponent of
    /// at most 100 in size, through polar form otherwise. Where Python raises
    /// `ZeroDivisionError`, zero to a negative or complex power, the result is `nan+nanj`, as
    /// `NumPy` gives; where it raises `OverflowError`, the parts are infinite.
    Power,
    /// `math.floor(x)` before its conversion to an integer: the largest whole number not above
    /// `x`, still a float.
    Floor,
    /// `math.ceil(x)` before its conversion to an integer: the smallest whole number not below
    /// `x`, still a float.
    Ceil,
    /// `math.trunc(x)` before its conversion to an integer: `x` with its fraction dropped,
    /// rounding toward zero, still a float.
    Truncate,
    /// `round` of a float to the nearest whole number, ties to even, as Python's `round` and
    /// `numpy.round` do; the result is still a float, so `2.5` gives `2.0`.
    RoundEven,
}

impl MathFunction {
    /// The name the source calls it by. `**` is written `pow`, its name in `math`, and
    /// [`Floor`](Self::Floor), [`Ceil`](Self::Ceil) and [`Truncate`](Self::Truncate) by the
    /// `math` function they are the first step of.
    ///
    /// ```
    /// assert_eq!(mir::MathFunction::Power.name(), "math.pow");
    /// assert_eq!(mir::MathFunction::LogOnePlus.name(), "math.log1p");
    /// assert_eq!(mir::MathFunction::RoundEven.name(), "round");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sqrt => "math.sqrt",
            Self::Cbrt => "math.cbrt",
            Self::Exp => "math.exp",
            Self::Exp2 => "math.exp2",
            Self::ExpMinusOne => "math.expm1",
            Self::Log => "math.log",
            Self::Log2 => "math.log2",
            Self::Log10 => "math.log10",
            Self::LogOnePlus => "math.log1p",
            Self::Sin => "math.sin",
            Self::Cos => "math.cos",
            Self::Tan => "math.tan",
            Self::Asin => "math.asin",
            Self::Acos => "math.acos",
            Self::Atan => "math.atan",
            Self::Atan2 => "math.atan2",
            Self::Sinh => "math.sinh",
            Self::Cosh => "math.cosh",
            Self::Tanh => "math.tanh",
            Self::Asinh => "math.asinh",
            Self::Acosh => "math.acosh",
            Self::Atanh => "math.atanh",
            Self::Erf => "math.erf",
            Self::Erfc => "math.erfc",
            Self::Gamma => "math.gamma",
            Self::LogGamma => "math.lgamma",
            Self::Fabs => "math.fabs",
            Self::Hypot => "math.hypot",
            Self::CopySign => "math.copysign",
            Self::TruncatedRemainder => "math.fmod",
            Self::FusedMultiplyAdd => "math.fma",
            Self::Power => "math.pow",
            Self::Floor => "math.floor",
            Self::Ceil => "math.ceil",
            Self::Truncate => "math.trunc",
            Self::RoundEven => "round",
        }
    }

    /// How many arguments it takes: 3 for [`FusedMultiplyAdd`](Self::FusedMultiplyAdd); 2 for
    /// [`Power`](Self::Power), [`Atan2`](Self::Atan2), [`Hypot`](Self::Hypot),
    /// [`CopySign`](Self::CopySign) and [`TruncatedRemainder`](Self::TruncatedRemainder); 1 for
    /// the rest.
    ///
    /// ```
    /// assert_eq!(mir::MathFunction::Sqrt.arity(), 1);
    /// assert_eq!(mir::MathFunction::Power.arity(), 2);
    /// assert_eq!(mir::MathFunction::FusedMultiplyAdd.arity(), 3);
    /// ```
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            Self::FusedMultiplyAdd => 3,
            Self::Power | Self::Atan2 | Self::Hypot | Self::CopySign | Self::TruncatedRemainder => {
                2
            }
            _ => 1,
        }
    }
}
