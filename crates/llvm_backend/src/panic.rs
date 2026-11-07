//! Panics: the runtime function a failed check calls, and the constants its message and source
//! location live in.
//!
//! A failed check jumps to a block of its own that calls one of the runtime's panic functions
//! (decision 0021) and never returns:
//!
//! | Check | Runtime function | What it prints |
//! | --- | --- | --- |
//! | An index in bounds | `npl_rt_panic_index` | The index, the dimension and its length |
//! | Two arrays of the same length | `npl_rt_panic_length_mismatch` | The dimension and both lengths |
//! | Everything else | `npl_rt_panic` | A message the backend writes as a constant |
//!
//! Each call passes a pointer to a `Location` constant, the runtime's `#[repr(C)]` structure
//! `{ ptr file, i64 file_length, i32 line, i32 column }`. The `i64` is the runtime's `usize`, so
//! the module targets 64-bit machines only, which every target in the design plan is.
//!
//! Constants are shared across the module: one per distinct message, file name and location.

use crate::Unsupported;
use crate::function::Function;
use mir::Check;
use source::{FileId, Sources, Span};
use std::collections::BTreeMap;
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
    /// Arrays combined element by element whose lengths differ in `dimension`.
    Length {
        /// The dimension, 0 for the outermost.
        dimension: u32,
        /// The first array's length, an `i64` operand.
        expected: String,
        /// The other array's length, an `i64` operand.
        found: String,
    },
}

/// What a constant holds, which is also how a repeated one is found again.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// The bytes of a message or file name.
    Bytes(Vec<u8>),
    /// The location of the byte at `offset` in `file`.
    Location {
        /// The file.
        file: FileId,
        /// The byte offset in it.
        offset: u32,
    },
}

/// The module's message, file name and location constants, written once each.
pub(crate) struct Constants<'a> {
    /// The source files, for file names, positions and the text of an `assert`.
    sources: &'a dyn Sources,
    /// Each constant's definition, in the order they were made; the constant's number is its
    /// position here.
    definitions: Vec<(Key, String)>,
    /// The number of each constant, by what it holds.
    numbers: BTreeMap<Key, usize>,
}

impl<'a> Constants<'a> {
    /// No constants yet; file names, positions and source text are read from `sources`.
    pub(crate) fn new(sources: &'a dyn Sources) -> Self {
        Self {
            sources,
            definitions: Vec::new(),
            numbers: BTreeMap::new(),
        }
    }

    /// How many constants there are, to [`roll_back`](Self::roll_back) to.
    pub(crate) fn mark(&self) -> usize {
        self.definitions.len()
    }

    /// Forgets every constant made after `mark`, when the function that made them failed.
    pub(crate) fn roll_back(&mut self, mark: usize) {
        for (key, _) in self.definitions.drain(mark..) {
            self.numbers.remove(&key);
        }
    }

    /// Every constant's definition, one per line, in the order they were made.
    pub(crate) fn finish(self) -> String {
        self.definitions
            .into_iter()
            .map(|(_, definition)| definition + "\n")
            .collect()
    }

    /// The constant holding `bytes`, as `@npl.str.N`, made the first time it is asked for.
    pub(crate) fn bytes(&mut self, bytes: &[u8]) -> String {
        let key = Key::Bytes(bytes.to_vec());
        if let Some(&number) = self.numbers.get(&key) {
            return format!("@npl.str.{number}");
        }
        let name = format!("@npl.str.{}", self.definitions.len());
        let mut escaped = String::new();
        for &byte in bytes {
            if byte == b'"' || byte == b'\\' || !(0x20..0x7f).contains(&byte) {
                let _ = write!(escaped, "\\{byte:02X}");
            } else {
                escaped.push(char::from(byte));
            }
        }
        let definition = format!(
            "{name} = private unnamed_addr constant [{} x i8] c\"{escaped}\"",
            bytes.len()
        );
        self.push(key, definition);
        name
    }

    /// The `Location` constant of where `span` starts, as `@npl.loc.N`.
    pub(crate) fn location(&mut self, span: Span) -> String {
        let key = Key::Location {
            file: span.file(),
            offset: span.start(),
        };
        if let Some(&number) = self.numbers.get(&key) {
            return format!("@npl.loc.{number}");
        }
        let file = self.sources.file(span.file());
        let position = file.position(span.start());
        let name_bytes = file.name().as_bytes().to_vec();
        let file_name = self.bytes(&name_bytes);
        let name = format!("@npl.loc.{}", self.definitions.len());
        let definition = format!(
            "{name} = private unnamed_addr constant {{ ptr, i64, i32, i32 }} \
             {{ ptr {file_name}, i64 {}, i32 {}, i32 {} }}",
            name_bytes.len(),
            position.line,
            position.column
        );
        self.push(key, definition);
        name
    }

    /// The source text of `span` with its whitespace runs made single spaces, so a statement
    /// written over several lines reads as one.
    pub(crate) fn source(&self, span: Span) -> String {
        let text = self.sources.file(span.file()).text();
        text.get(span.start() as usize..span.end() as usize)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Records a new constant.
    fn push(&mut self, key: Key, definition: String) {
        self.numbers.insert(key.clone(), self.definitions.len());
        self.definitions.push((key, definition));
    }
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
        let _ = write!(self.failures, "{label}:\n  {instruction}\n  unreachable\n");
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
            Check::Assertion => {
                let source = self.constants.source(self.span);
                let condition = source.strip_prefix("assert ").unwrap_or(&source);
                Failure::Message(format!("assertion failed: {condition}"))
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
                    "call void @npl_rt_panic(ptr {text}, i64 {}, ptr {location})",
                    message.len()
                ),
                "npl_rt_panic",
                "declare void @npl_rt_panic(ptr, i64, ptr) cold noreturn nounwind",
            )
        }
        Failure::Index {
            index,
            dimension,
            length,
        } => (
            format!(
                "call void @npl_rt_panic_index(i64 {index}, i32 {dimension}, i64 {length}, \
                 ptr {location})"
            ),
            "npl_rt_panic_index",
            "declare void @npl_rt_panic_index(i64, i32, i64, ptr) cold noreturn nounwind",
        ),
        Failure::Length {
            dimension,
            expected,
            found,
        } => (
            format!(
                "call void @npl_rt_panic_length_mismatch(i32 {dimension}, i64 {expected}, \
                 i64 {found}, ptr {location})"
            ),
            "npl_rt_panic_length_mismatch",
            "declare void @npl_rt_panic_length_mismatch(i32, i64, i64, ptr) cold noreturn nounwind",
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
