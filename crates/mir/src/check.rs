//! What an [`Assert`](crate::TerminatorKind::Assert) checks: each [`Check`] names the
//! condition and carries the operands its panic message needs.

use crate::{BinaryOperator, Operand};

/// What an [`Assert`](crate::TerminatorKind::Assert) checks, for the panic message.
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
    /// An array reduced by `a.min()` or `a.max()` has at least one element, since there is no
    /// smallest or largest of none. The panic message quotes the call from the source.
    NotEmpty,
    /// A key looked up with `d[k]` or `d.pop(k)` is in the `dict` (decision 0035). A missing key
    /// panics rather than raising `KeyError`, as an index out of range does.
    KeyPresent {
        /// The key, written as Python's `repr` writes it, such as `'Ar'` or `(1, 2)`: a `str`
        /// the panic message names. The stage that writes the check builds it only on the
        /// failing path, so a lookup that succeeds formats nothing.
        key: Operand<'db>,
    },
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
            Self::KeyPresent { key } => vec![key],
            Self::NoOverflow(_)
            | Self::NoPowerOverflow
            | Self::NonZeroDivisor
            | Self::NonZeroStep { .. }
            | Self::Assertion
            | Self::NotEmpty => Vec::new(),
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
            Self::KeyPresent { key } => vec![key],
            Self::NoOverflow(_)
            | Self::NoPowerOverflow
            | Self::NonZeroDivisor
            | Self::NonZeroStep { .. }
            | Self::Assertion
            | Self::NotEmpty => Vec::new(),
        }
    }
}
