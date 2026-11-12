//! Panics: the runtime function a failed check calls, and the constants its message and source
//! location live in.
//!
//! A failed check jumps to a block of its own that calls one of the runtime's panic functions
//! (decision 0021) and never returns:
//!
//! | Check | Runtime function | What it prints |
//! | --- | --- | --- |
//! | An index in bounds | `baryon_rt_panic_index` | The index, the dimension and its length |
//! | A slice in bounds | `baryon_rt_panic_slice` | The start and stop, the dimension and its length |
//! | Two arrays of the same length | `baryon_rt_panic_length_mismatch` | The dimension and both lengths |
//! | A non-negative exponent of integer `**` | `baryon_rt_panic_exponent` | The exponent |
//! | A key in a `dict` | `baryon_rt_panic_key` | The key, as Python's `repr` writes it |
//! | Everything else | `baryon_rt_panic` | A message the backend writes as a constant |
//!
//! Each call passes a pointer to a `Location` constant, the runtime's `#[repr(C)]` structure
//! `{ ptr file, i64 file_length, i32 line, i32 column }`. The `i64` is the runtime's `usize`, so
//! the module targets 64-bit machines only, which every target in the design plan is.
//!
//! Constants are shared across the module: one per distinct message, file name and location.
//! The `constants` module writes them.

use crate::Unsupported;
use crate::constants::Constants;
use crate::function::Function;
use mir::Check;
use std::fmt::Write as _;

/// Why a check fails, with the values its message reports, each an LLVM operand already written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// A message written in full, such as "integer overflow in `+`".
    Message(String),
    /// An index out of range: the index, the dimension, 0 for the outermost, and its length.
    Index {
        /// The index, an `i64` operand.
        index: String,
        /// The dimension indexed.
        dimension: u32,
        /// The dimension's length, an `i64` operand.
        length: String,
    },
    /// A slice whose range does not lie within its dimension.
    Slice {
        /// Where the range starts, an `i64` operand.
        start: String,
        /// Where the range stops, an `i64` operand.
        stop: String,
        /// The dimension sliced.
        dimension: u32,
        /// The dimension's length, an `i64` operand.
        length: String,
    },
    /// Arrays combined element by element whose lengths differ in `dimension`.
    Length {
        /// The dimension, 0 for the outermost.
        dimension: u32,
        /// The first array's length, an `i64` operand.
        expected: String,
        /// The other array's length, an `i64` operand.
        found: String,
    },
    /// A key that is not in a `dict`: the key as Python's `repr` writes it, a `str`'s byte
    /// pointer and byte count.
    Key {
        /// The text's bytes, a `ptr` operand.
        bytes: String,
        /// The number of bytes, an `i64` operand.
        length: String,
    },
    /// Integer `**` with a negative exponent.
    Exponent {
        /// The exponent, an `i64` operand.
        exponent: String,
    },
}

impl<'db> Function<'_, '_, 'db> {
    /// Branches to a failure block for `failure` unless `condition` holds, and continues in a
    /// new block.
    pub(crate) fn check(&mut self, condition: &str, failure: &Failure) {
        let next = self.fresh_block();
        let failed = self.failure_block(failure);
        self.line(&format!(
            "br i1 {condition}, label %{next}, label %{failed}"
        ));
        self.line_label(&next);
    }

    /// Writes a block that panics with `failure` at the current source location, and returns
    /// its label. The block goes after the body's own blocks, out of the hot path.
    pub(crate) fn failure_block(&mut self, failure: &Failure) -> String {
        let label = format!("p{}", self.next_failure);
        self.next_failure += 1;
        let location = self.constants.location(self.span);
        let (instruction, symbol, declaration) = call(self.constants, failure, &location);
        self.declare(symbol, declaration);
        let located = self.located(&instruction);
        let _ = write!(self.failures, "{label}:\n  {located}\n  unreachable\n");
        label
    }

    /// The failure an [`Assert`](mir::TerminatorKind::Assert) reports when `check` does not
    /// hold, with the values it names loaded.
    pub(crate) fn failure(&mut self, check: &Check<'db>) -> Result<Failure, Unsupported> {
        Ok(match check {
            Check::NoOverflow(operator) => {
                Failure::Message(format!("integer overflow in `{}`", symbol(*operator)))
            }
            Check::NonZeroDivisor => Failure::Message("integer division by zero".to_owned()),
            Check::NonZeroStep { slice: true } => {
                Failure::Message("slice step cannot be zero".to_owned())
            }
            Check::SliceInBounds {
                dimension,
                start,
                stop,
                length,
            } => Failure::Slice {
                start: self.value(start)?.0,
                stop: self.value(stop)?.0,
                dimension: *dimension,
                length: self.value(length)?.0,
            },
            Check::NoPowerOverflow => Failure::Message("integer overflow in `**`".to_owned()),
            Check::NonZeroStep { slice: false } => {
                Failure::Message("`range` step is zero".to_owned())
            }
            Check::NonNegativeExponent { exponent } => {
                let (exponent, ty) = self.value(exponent)?;
                Failure::Exponent {
                    exponent: self.widen(&exponent, ty)?,
                }
            }
            Check::Assertion => {
                let source = self.constants.source(self.span);
                let condition = source.strip_prefix("assert ").unwrap_or(&source);
                Failure::Message(format!("assertion failed: {condition}"))
            }
            Check::KeyPresent { key } => {
                let (text, _) = self.value(key)?;
                let (bytes, length) = self.text_parts(&text);
                Failure::Key { bytes, length }
            }
            Check::NotEmpty { list } => {
                let call = self.constants.source(self.span);
                let sequence = if *list { "list" } else { "array" };
                Failure::Message(format!("`{call}` of an empty {sequence}"))
            }
            Check::SameLength {
                dimension,
                expected,
                found,
            } => Failure::Length {
                dimension: *dimension,
                expected: self.value(expected)?.0,
                found: self.value(found)?.0,
            },
            Check::IndexInBounds {
                dimension,
                index,
                length,
            } => {
                let (index, ty) = self.value(index)?;
                Failure::Index {
                    index: self.widen(&index, ty)?,
                    dimension: *dimension,
                    length: self.value(length)?.0,
                }
            }
        })
    }
}

/// The call a failed check makes, for `failure` at the location constant `location`: the
/// instruction and the runtime function's symbol and declaration.
pub(crate) fn call(
    constants: &mut Constants<'_>,
    failure: &Failure,
    location: &str,
) -> (String, &'static str, &'static str) {
    match failure {
        Failure::Message(message) => {
            let text = constants.bytes(message.as_bytes());
            (
                format!(
                    "call void @baryon_rt_panic(ptr {text}, i64 {}, ptr {location})",
                    message.len()
                ),
                "baryon_rt_panic",
                "declare void @baryon_rt_panic(ptr, i64, ptr) cold noreturn nounwind",
            )
        }
        Failure::Index {
            index,
            dimension,
            length,
        } => (
            format!(
                "call void @baryon_rt_panic_index(i64 {index}, i32 {dimension}, i64 {length}, \
                 ptr {location})"
            ),
            "baryon_rt_panic_index",
            "declare void @baryon_rt_panic_index(i64, i32, i64, ptr) cold noreturn nounwind",
        ),
        Failure::Slice {
            start,
            stop,
            dimension,
            length,
        } => (
            format!(
                "call void @baryon_rt_panic_slice(i64 {start}, i64 {stop}, i32 {dimension}, \
                 i64 {length}, ptr {location})"
            ),
            "baryon_rt_panic_slice",
            "declare void @baryon_rt_panic_slice(i64, i64, i32, i64, ptr) cold noreturn nounwind",
        ),
        Failure::Length {
            dimension,
            expected,
            found,
        } => (
            format!(
                "call void @baryon_rt_panic_length_mismatch(i32 {dimension}, i64 {expected}, \
                 i64 {found}, ptr {location})"
            ),
            "baryon_rt_panic_length_mismatch",
            "declare void @baryon_rt_panic_length_mismatch(i32, i64, i64, ptr) cold noreturn nounwind",
        ),
        Failure::Key { bytes, length } => (
            format!("call void @baryon_rt_panic_key(ptr {bytes}, i64 {length}, ptr {location})"),
            "baryon_rt_panic_key",
            "declare void @baryon_rt_panic_key(ptr, i64, ptr) cold noreturn nounwind",
        ),
        Failure::Exponent { exponent } => (
            format!("call void @baryon_rt_panic_exponent(i64 {exponent}, ptr {location})"),
            "baryon_rt_panic_exponent",
            "declare void @baryon_rt_panic_exponent(i64, ptr) cold noreturn nounwind",
        ),
    }
}

/// How an operator is written in source, for a message such as "integer overflow in `+`".
pub(crate) fn symbol(operator: mir::BinaryOperator) -> &'static str {
    use mir::BinaryOperator as Operator;
    match operator {
        Operator::Add => "+",
        Operator::Subtract => "-",
        Operator::Multiply => "*",
        Operator::Divide => "/",
        Operator::FloorDivide => "//",
        Operator::Remainder => "%",
        Operator::BitAnd => "&",
        Operator::BitOr => "|",
        Operator::BitXor => "^",
        Operator::ShiftLeft => "<<",
        Operator::ShiftRight => ">>",
        Operator::Equal => "==",
        Operator::NotEqual => "!=",
        Operator::Less => "<",
        Operator::LessEqual => "<=",
        Operator::Greater => ">",
        Operator::GreaterEqual => ">=",
    }
}
