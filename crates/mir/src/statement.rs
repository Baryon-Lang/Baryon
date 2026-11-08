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
}

impl Builtin {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::Builtin::Print.name(), "print");
    /// assert_eq!(mir::Builtin::Math(mir::MathFunction::Sqrt).name(), "math.sqrt");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Math(function) => function.name(),
        }
    }
}

/// A mathematical function of floats, called by [`Builtin::Math`]. Each gives the result IEEE
/// 754 and the C library give for its argument type; none of them fails or panics. NaN and the
/// infinities pass through as C's functions define, so `sqrt(-1.0)` is NaN, as `numpy.sqrt`
/// gives, where Python's `math.sqrt` raises.
///
/// These are the functions the example programs and the most common physics formulas need; the
/// standard library (roadmap 1.13) adds the rest of Python's `math` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum MathFunction {
    /// `math.sqrt(x)`: the square root, correctly rounded.
    Sqrt,
    /// `math.exp(x)`: e to the power `x`.
    Exp,
    /// `math.log(x)`: the natural logarithm.
    Log,
    /// `math.sin(x)`, with `x` in radians.
    Sin,
    /// `math.cos(x)`, with `x` in radians.
    Cos,
    /// `x ** y` on floats, and `math.pow(x, y)`: two arguments, the base then the exponent.
    Power,
    /// `round` of a float to the nearest whole number, ties to even, as Python's `round` and
    /// `numpy.round` do; the result is still a float, so `2.5` gives `2.0`.
    RoundEven,
}

impl MathFunction {
    /// The name the source calls it by. `**` is written `pow`, its name in `math`.
    ///
    /// ```
    /// assert_eq!(mir::MathFunction::Power.name(), "math.pow");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sqrt => "math.sqrt",
            Self::Exp => "math.exp",
            Self::Log => "math.log",
            Self::Sin => "math.sin",
            Self::Cos => "math.cos",
            Self::Power => "math.pow",
            Self::RoundEven => "round",
        }
    }

    /// How many arguments it takes: 2 for [`Power`](Self::Power), 1 for the rest.
    ///
    /// ```
    /// assert_eq!(mir::MathFunction::Sqrt.arity(), 1);
    /// assert_eq!(mir::MathFunction::Power.arity(), 2);
    /// ```
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            Self::Power => 2,
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
    /// An integer operation did not overflow.
    NoOverflow(BinaryOperator),
    /// A divisor is not zero.
    NonZeroDivisor,
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
            Self::SameLength {
                expected, found, ..
            } => vec![expected, found],
            Self::NoOverflow(_) | Self::NonZeroDivisor | Self::Assertion => Vec::new(),
        }
    }

    /// The operands the check reports, to change in place, in the order of
    /// [`operands`](Self::operands).
    pub fn operands_mut(&mut self) -> Vec<&mut Operand<'db>> {
        match self {
            Self::IndexInBounds { index, length, .. } => vec![index, length],
            Self::SameLength {
                expected, found, ..
            } => vec![expected, found],
            Self::NoOverflow(_) | Self::NonZeroDivisor | Self::Assertion => Vec::new(),
        }
    }
}
