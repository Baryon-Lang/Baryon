//! Statements and terminators: what a basic block does, and where control goes after it.

use crate::{BinaryOperator, BlockId, Local, Operand, Place, Rvalue};
use source::Span;
use types::{Item, Type};

/// One step inside a basic block, with the source it came from.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Statement<'db> {
    /// What the statement does.
    pub kind: StatementKind<'db>,
    /// The source it lowers, for debug information.
    pub span: Span,
}

/// What a statement does.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum StatementKind<'db> {
    /// Computes the rvalue and stores it at the place. The place's old value, if it owns memory,
    /// was dropped or moved out before.
    Assign(Place, Rvalue<'db>),
    /// Ends the value at the place and frees what it owns. Lowering writes one at every exit
    /// from a value's scope. A drop cannot fail, so it needs no error edge.
    ///
    /// In a [`Phase::Built`](crate::Phase) body the drop does nothing when the value was moved
    /// out; drop elaboration makes every drop unconditional, guarded by `bool` flag locals where
    /// a value is moved on some paths only.
    Drop(Place),
    /// Adds `value` into the element at `place`, in an array a `prange` loop lists in
    /// `accumulate=` (decisions 0013 and 0024). `place` is an accumulated parameter of an
    /// outlined body, projected by an index and possibly then into a `Fixed` element, and
    /// `value` has that element's type.
    ///
    /// The statement is not a read-modify-write of shared memory: a backend adds `value` into
    /// the running chunk's private copy of the element, and after the loop the copies combine
    /// into the array in the reference's fixed tree, as a [`Reduction`] does. Lowering puts the
    /// index's bounds check before it, as for any other element.
    Accumulate {
        /// The element added into.
        place: Place,
        /// Whether `value` is added or subtracted.
        operator: AccumulateOperator,
        /// The contribution.
        value: Operand<'db>,
    },
}

/// How an [`Accumulate`](StatementKind::Accumulate) statement combines its value with the
/// element: `x += e` or `x -= e`.
///
/// Subtraction stays a subtraction instead of adding `-e`, so an integer `e` equal to its type's
/// smallest value is never negated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccumulateOperator {
    /// `forces[j] += e`.
    Add,
    /// `forces[j] -= e`.
    Subtract,
}

impl AccumulateOperator {
    /// The operator's symbol in the text form.
    ///
    /// ```
    /// assert_eq!(mir::AccumulateOperator::Subtract.symbol(), "-=");
    /// ```
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+=",
            Self::Subtract => "-=",
        }
    }
}

/// The end of a basic block, with the source it came from.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Terminator<'db> {
    /// Where control goes.
    pub kind: TerminatorKind<'db>,
    /// The source it lowers, for debug information.
    pub span: Span,
}

/// Where control goes at the end of a block.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum TerminatorKind<'db> {
    /// To one block.
    Goto(BlockId),
    /// To `then` when the `bool` condition is `True`, else to `otherwise`.
    Branch {
        /// The condition, a `bool`.
        condition: Operand<'db>,
        /// Where to go when it is `True`.
        then: BlockId,
        /// Where to go when it is `False`.
        otherwise: BlockId,
    },
    /// To the block paired with the integer value, or `otherwise` when none matches. Enum values
    /// switch on their variant's position, and errors on their [`ErrorIndex`](Rvalue::ErrorIndex).
    Switch {
        /// The value, an integer or an enum.
        value: Operand<'db>,
        /// Each value and where it goes, each value at most once.
        cases: Vec<(i128, BlockId)>,
        /// Where every other value goes.
        otherwise: BlockId,
    },
    /// Returns the value in [`Local::RETURN`] to the caller.
    Return,
    /// Fails: returns one error value to the caller (decision 0005). Only in a body whose
    /// function declares `Raises`, and the operand's type is one of the errors it lists.
    Raise(Operand<'db>),
    /// Calls a function, stores its result at `destination`, and goes to `target`.
    Call {
        /// The function called.
        callee: Callee<'db>,
        /// The arguments, in parameter order, each in its parameter's mode.
        arguments: Vec<Argument<'db>>,
        /// Where the result goes on success.
        destination: Place,
        /// Where control goes on success.
        target: BlockId,
        /// Where control goes when the callee raises; present exactly when it can.
        error: Option<ErrorEdge>,
    },
    /// Panics with `check`'s message unless the `bool` condition is `True`, then goes to
    /// `target`. Panics are not caught (decision 0005), so a failed check has no successor.
    Assert {
        /// The condition that must hold.
        condition: Operand<'db>,
        /// What failed, for the panic message.
        check: Check<'db>,
        /// Where control goes when it holds.
        target: BlockId,
    },
    /// Control never gets here, as after a `match` that covers every case.
    Unreachable,
    /// Runs a `prange` loop: calls the outlined body `body` once for each iteration
    /// `0..count`, in any order and on any number of threads, then goes to `target`
    /// (decision 0018).
    ///
    /// The outlined body is [`Body::outlined`](crate::Body::outlined)`[body]`. Its parameters
    /// are, in order: the iteration number, an `Owned` `i64` counted from 0; one per argument,
    /// in that argument's mode; one `InOut` per reduction, of the reduced place's type; and one
    /// `InOut` per accumulation, of the accumulated array's type. It returns `None` and cannot
    /// fail: the checker allows no `return`, `break` or `raise` in a `prange` body, so its
    /// `continue` is a return.
    ///
    /// A backend gives each chunk of iterations (runtime, `baryon_rt_parallel_for`) a private
    /// copy of each reduced value, starting at the operator's identity, passes that copy to
    /// the chunk's iterations in order, and after the loop combines the copies into the place
    /// in the fixed tree the language reference defines ("Reproducible reductions", decision
    /// 0021). The chunks and the tree depend only on `count`, so
    /// the result is the same on any number of threads.
    ParallelFor {
        /// The outlined body's position in [`Body::outlined`](crate::Body::outlined).
        body: u32,
        /// How many iterations run, an `i64`; none when it is 0 or less.
        count: Operand<'db>,
        /// The places from outside the loop that the body reads or writes, each in the mode
        /// its parameter declares: `Borrow` for one only read, `InOut` for an array written
        /// only at the iteration's own elements. An `Owned` argument copies a value in.
        arguments: Vec<Argument<'db>>,
        /// The shared scalars the body reduces into, such as `energy += e`.
        reductions: Vec<Reduction>,
        /// The arrays listed in `accumulate=`, which the body only adds into, through
        /// [`Accumulate`](StatementKind::Accumulate) statements on their parameters. A backend
        /// gives each chunk private copies of the parts it touches and combines them into the
        /// array after the loop, in the same fixed tree as a reduction (decision 0024).
        accumulations: Vec<Place>,
        /// Where control goes when every iteration has finished.
        target: BlockId,
    },
}

/// One value a [`ParallelFor`](TerminatorKind::ParallelFor) loop reduces into: each chunk
/// combines its iterations' contributions into a private copy that starts at the operator's
/// identity, and the copies are combined into the place after the loop.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reduction {
    /// The scalar reduced into, outside the loop.
    pub place: Place,
    /// How contributions combine.
    pub operator: ReductionOperator,
}

/// How a [`Reduction`] combines contributions, each with the identity a chunk's private copy
/// starts at (language reference, "What a `prange` body may do", rule 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReductionOperator {
    /// `x += e` and `x -= e`, which may be mixed: addition, from 0.
    Add,
    /// `x *= e`: multiplication, from 1.
    Multiply,
    /// `x = min(x, e)`: the smaller, from the type's largest value (`inf` for floats). As
    /// Python's `min` does, `min(a, b)` is `b` only when `b < a`.
    Min,
    /// `x = max(x, e)`: the larger, from the type's smallest value (`-inf` for floats). As
    /// Python's `max` does, `max(a, b)` is `b` only when `b > a`.
    Max,
}

impl ReductionOperator {
    /// The operator's name in the text form.
    ///
    /// ```
    /// assert_eq!(mir::ReductionOperator::Min.name(), "Min");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Multiply => "Multiply",
            Self::Min => "Min",
            Self::Max => "Max",
        }
    }
}

/// The function a [`Call`](TerminatorKind::Call) calls.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Callee<'db> {
    /// A function or method named in the source, at these type arguments.
    Function {
        /// The function.
        item: Item<'db>,
        /// One type per type parameter of the function, in order; empty when it has none. In a
        /// built generic body these may themselves contain type parameters.
        type_arguments: Vec<Type<'db>>,
    },
    /// A function passed in as a value, of a `Callable` type.
    Value(Operand<'db>),
    /// A function the language provides, which has no item: each backend writes it itself.
    Builtin(Builtin),
}

/// A function the language provides that a [`Call`](TerminatorKind::Call) can call by
/// [`Callee::Builtin`]. Built-ins that are only arithmetic, such as `len` or a conversion, are
/// lowered to rvalues instead; the ones here need code a backend or the runtime provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum Builtin {
    /// `print(a, b, …)`: writes each argument to standard output as Python does, separated by a
    /// space, then a line end. Each argument is a number, a `bool` or a `str`, passed `Borrow`
    /// or `Owned`; the destination has type `None` and the call cannot fail.
    Print,
    /// A mathematical function of floats. Every argument and the destination have one float
    /// type, `f32` or `f64`; lowering converts integer operands first. The call cannot fail.
    Math(MathFunction),
    /// `a.sum()` of an `Array` or `Fixed` array of floats: one argument, the array, passed
    /// `Borrow`; the destination has the element type. The elements are added in row-major
    /// order in `NumPy`'s pairwise tree, so the result has the same bits as `numpy.sum` of the
    /// same array and rounding error grows with the logarithm of the length, not the length.
    /// Integer sums need no tree, since integer addition is exact, and lower to a loop.
    Sum,
    /// `xs.append(value)` on a `list`: two arguments, the list passed `InOut` and the value
    /// passed `Owned`, of the list's element type; the destination has type `None`. When the
    /// buffer is full it grows to twice its capacity, at least 4 elements, so a run of appends
    /// costs amortized constant time per element, as Python's and Rust's do. Running out of
    /// memory panics; the call cannot fail otherwise.
    Append,
    /// `random.Philox(seed, stream)`: two arguments, the seed and the stream, each an `i64`
    /// passed `Owned`; the destination has type `Philox`. It makes the generator whose numbers
    /// match `NumPy`'s `Generator(Philox(key=seed).jumped(stream))` (decision 0028). A negative
    /// seed or stream panics; the call cannot fail otherwise.
    Philox,
    /// `generator.uniform()`: one argument, a `Philox` passed `InOut`; the destination has type
    /// `f64`. It draws the generator's next 64-bit output `u` and gives `(u >> 11) * 2**-53`, a
    /// float in `[0, 1)`, as `NumPy`'s `Generator.random()` does. The call cannot fail.
    Uniform,
}

impl Builtin {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::Builtin::Print.name(), "print");
    /// assert_eq!(mir::Builtin::Math(mir::MathFunction::Sqrt).name(), "math.sqrt");
    /// assert_eq!(mir::Builtin::Sum.name(), "sum");
    /// assert_eq!(mir::Builtin::Append.name(), "list.append");
    /// assert_eq!(mir::Builtin::Philox.name(), "random.Philox");
    /// assert_eq!(mir::Builtin::Uniform.name(), "Philox.uniform");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Math(function) => function.name(),
            Self::Sum => "sum",
            Self::Append => "list.append",
            Self::Philox => "random.Philox",
            Self::Uniform => "Philox.uniform",
        }
    }
}

/// A mathematical function of floats, called by [`Builtin::Math`]. Each gives the result IEEE
/// 754 and the C library give for its argument type; none of them fails or panics. NaN and the
/// infinities pass through as C's functions define, so `sqrt(-1.0)` is NaN, as `numpy.sqrt`
/// gives, where Python's `math.sqrt` raises.
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
    /// arguments. Python's `hypot` of more than two arguments is not here.
    Hypot,
    /// `math.copysign(x, y)`: the size of `x` with the sign of `y`; two arguments.
    CopySign,
    /// `math.fmod(x, y)`: the remainder of `x / y` truncated toward zero, with the sign of `x`,
    /// as C's `fmod`; two arguments. It differs from `%`, which floors as Python does.
    TruncatedRemainder,
    /// `math.fma(x, y, z)`: `x * y + z` rounded once; three arguments.
    FusedMultiplyAdd,
    /// `x ** y` on floats, and `math.pow(x, y)`: two arguments, the base then the exponent.
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

/// One argument of a call, in the mode its parameter declares (decision 0006). The modes stay
/// visible here so the reference checker and the backends' `noalias` metadata read them directly.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Argument<'db> {
    /// A read-only borrow of the place, the default mode.
    Borrow(Place),
    /// An `InOut` borrow: the callee may change the place, and nothing else may touch it during
    /// the call.
    InOut(Place),
    /// An `Owned` argument: the value moves, or for a copy type copies, into the callee.
    Owned(Operand<'db>),
}

/// Where a call goes when the callee raises (decision 0005).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ErrorEdge {
    /// The local that receives the error value. It holds one of the callee's errors; read which
    /// with [`ErrorIndex`](Rvalue::ErrorIndex) and the error with
    /// [`Error`](crate::Projection::Error).
    pub error: Local,
    /// Where control goes.
    pub target: BlockId,
}

/// What an [`Assert`](TerminatorKind::Assert) checks, for the panic message.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Check<'db> {
    /// An array index is below its dimension's length and not negative (decision 0010).
    IndexInBounds {
        /// The dimension indexed, 0 for the outermost, so the panic message can name it.
        dimension: u32,
        /// The index.
        index: Operand<'db>,
        /// The dimension's length.
        length: Operand<'db>,
    },
    /// A slice's range lies within its dimension (language reference, "Arrays", "Slicing"):
    /// `start` and `stop` are each from 0 to the length, and when the range takes any element
    /// its first, at `start`, is below the length. Python shortens such a slice; here it is
    /// the bug it usually is.
    SliceInBounds {
        /// The dimension sliced, 0 for the outermost, so the panic message can name it.
        dimension: u32,
        /// Where the range starts, as the program wrote it or its default.
        start: Operand<'db>,
        /// Where the range stops, as the program wrote it or its default.
        stop: Operand<'db>,
        /// The dimension's length.
        length: Operand<'db>,
    },
    /// An integer operation did not overflow.
    NoOverflow(BinaryOperator),
    /// An integer `**` did not overflow. Lowering writes integer `x ** n` as repeated squaring,
    /// whose multiplications each carry this check, so the panic names the `**` the program
    /// wrote rather than a `*` it did not.
    NoPowerOverflow,
    /// A divisor is not zero.
    NonZeroDivisor,
    /// The step of a `range` or a slice is not zero, which Python reports as a `ValueError`:
    /// `range() arg 3 must not be zero`, or `slice step cannot be zero`. Lowering writes it only
    /// for a step that is not a constant; a constant zero is reported when building.
    NonZeroStep {
        /// Whether the step is a slice's, `a[::k]`, rather than a `range`'s, which picks the
        /// panic message.
        slice: bool,
    },
    /// The exponent of an integer `**` is zero or more: a negative exponent would need a
    /// fraction, which an integer cannot hold (language reference, "Errors and panics").
    NonNegativeExponent {
        /// The exponent, which the panic message names.
        exponent: Operand<'db>,
    },
    /// Two arrays combined element by element have the same length in one dimension, as
    /// whole-array arithmetic requires until broadcasting (roadmap 3.9).
    SameLength {
        /// The dimension, 0 for the outermost.
        dimension: u32,
        /// The length the first array has.
        expected: Operand<'db>,
        /// The length this array has.
        found: Operand<'db>,
    },
    /// A program's `assert` statement holds.
    Assertion,
}

impl<'db> Check<'db> {
    /// The operands the check's panic message reports, in order; empty for checks that report
    /// none. Passes that visit every operand of a body use it so a new check cannot be missed.
    #[must_use]
    pub fn operands(&self) -> Vec<&Operand<'db>> {
        match self {
            Self::IndexInBounds { index, length, .. } => vec![index, length],
            Self::SliceInBounds {
                start,
                stop,
                length,
                ..
            } => vec![start, stop, length],
            Self::SameLength {
                expected, found, ..
            } => vec![expected, found],
            Self::NonNegativeExponent { exponent } => vec![exponent],
            Self::NoOverflow(_)
            | Self::NoPowerOverflow
            | Self::NonZeroDivisor
            | Self::NonZeroStep { .. }
            | Self::Assertion => Vec::new(),
        }
    }

    /// Whether the check is an integer overflow check, which a release build leaves out so the
    /// result wraps (decision 0030).
    ///
    /// # Example
    ///
    /// ```
    /// use mir::{BinaryOperator, Check};
    ///
    /// assert!(Check::NoOverflow(BinaryOperator::Add).is_overflow());
    /// assert!(Check::NoPowerOverflow.is_overflow());
    /// assert!(!Check::NonZeroStep { slice: false }.is_overflow());
    /// ```
    #[must_use]
    pub fn is_overflow(&self) -> bool {
        matches!(self, Self::NoOverflow(_) | Self::NoPowerOverflow)
    }

    /// The operands the check reports, to change in place, in the order of
    /// [`operands`](Self::operands).
    pub fn operands_mut(&mut self) -> Vec<&mut Operand<'db>> {
        match self {
            Self::IndexInBounds { index, length, .. } => vec![index, length],
            Self::SliceInBounds {
                start,
                stop,
                length,
                ..
            } => vec![start, stop, length],
            Self::SameLength {
                expected, found, ..
            } => vec![expected, found],
            Self::NonNegativeExponent { exponent } => vec![exponent],
            Self::NoOverflow(_)
            | Self::NoPowerOverflow
            | Self::NonZeroDivisor
            | Self::NonZeroStep { .. }
            | Self::Assertion => Vec::new(),
        }
    }
}
