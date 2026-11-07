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
        /// The index.
        index: Operand<'db>,
        /// The dimension's length.
        length: Operand<'db>,
    },
    /// An integer operation did not overflow.
    NoOverflow(BinaryOperator),
    /// A divisor is not zero.
    NonZeroDivisor,
    /// A program's `assert` statement holds.
    Assertion,
}
