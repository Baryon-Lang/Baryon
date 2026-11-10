//! Panics: the message a failed check prints, and the one way a program ends because of it.
//!
//! A panic is how a compiled program stops when it finds a bug: an index out of range, arrays of
//! different lengths combined element by element, an `assert` that does not hold, running out of
//! memory. Panics cannot be caught (decision 0005), so ending the process is the whole job: print
//! the message, where it happened and the calls that led there, then exit with [`EXIT_STATUS`].

use crate::{Location, backtrace};
use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

/// The exit status of a program that panicked. It differs from the status 1 of an error that
/// reaches the end of `main` (the reference, "Errors"), so a script can tell a bug from a
/// reported failure. 101 is the status Rust programs use for the same thing.
pub(crate) const EXIT_STATUS: i32 = 101;

/// Why a program panicked: everything the message needs, before it is formatted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure<'text> {
    /// A message compiled code wrote in full, such as an overflow or an `assert`.
    Message(&'text str),
    /// An array index below zero or not below its dimension's length (decision 0010).
    IndexOutOfRange {
        /// The index.
        index: i64,
        /// The dimension indexed, 0 for the outermost.
        dimension: u32,
        /// The dimension's length.
        length: i64,
    },
    /// Two arrays combined element by element differ in one dimension's length.
    LengthMismatch {
        /// The dimension, 0 for the outermost.
        dimension: u32,
        /// The length the first array has.
        expected: i64,
        /// The length the other array has.
        found: i64,
    },
    /// Integer `**` with a negative exponent, whose result would be a fraction.
    NegativeExponent {
        /// The exponent.
        exponent: i64,
    },
    /// An array asked for a negative number of elements.
    NegativeLength {
        /// The number asked for.
        count: i64,
    },
    /// An array whose size in bytes does not fit in the address space.
    ArrayTooLarge {
        /// The number of elements.
        count: i64,
        /// The size of one element in bytes.
        element_size: usize,
    },
    /// The system allocator could not provide the memory.
    OutOfMemory {
        /// The size asked for, in bytes.
        bytes: usize,
    },
    /// A size and alignment no allocation can have: an alignment that is not a power of two, or
    /// a size too large once rounded up to it. Only a compiler bug asks for one.
    InvalidLayout {
        /// The size asked for, in bytes.
        bytes: usize,
        /// The alignment asked for, in bytes.
        align: usize,
    },
}

impl fmt::Display for Failure<'_> {
    /// Writes the message, which names the values the reference's panic table says it names.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Message(text) => formatter.write_str(text),
            Self::IndexOutOfRange {
                index,
                dimension,
                length,
            } => {
                write!(
                    formatter,
                    "index {index} is out of range for dimension {dimension} of length {length}"
                )
            }
            Self::LengthMismatch {
                dimension,
                expected,
                found,
            } => write!(
                formatter,
                "array lengths differ in dimension {dimension}: {expected} and {found}"
            ),
            Self::NegativeExponent { exponent } => {
                write!(formatter, "negative exponent {exponent} in integer `**`")
            }
            Self::NegativeLength { count } => {
                write!(formatter, "array length {count} is negative")
            }
            Self::ArrayTooLarge {
                count,
                element_size,
            } => write!(
                formatter,
                "an array of {count} elements of {element_size} bytes is too large for memory"
            ),
            Self::OutOfMemory { bytes } => {
                write!(formatter, "out of memory: could not allocate {bytes} bytes")
            }
            Self::InvalidLayout { bytes, align } => write!(
                formatter,
                "internal compiler error: no allocation has {bytes} bytes aligned to {align}"
            ),
        }
    }
}

/// The full text a panic prints: the message, then the location on its own line in the
/// `-->` form compiler diagnostics use, so the two read alike.
pub(crate) fn report(failure: Failure<'_>, location: Option<&Location>) -> String {
    match location {
        Some(location) => format!("panic: {failure}\n  --> {location}\n"),
        None => format!("panic: {failure}\n"),
    }
}

/// Set by the first thread that panics. A second thread that panics while the first is still
/// printing waits for the process to end instead of printing too, so messages never interleave
/// and the process exits once.
static PANICKING: AtomicBool = AtomicBool::new(false);

/// Prints the panic's [`report`] and the [`backtrace`] of the calling thread to standard error,
/// and ends the process with [`EXIT_STATUS`].
///
/// This is the runtime's one place that prints and exits: it is the job the language reference
/// gives a panic. When threads of a parallel loop panic at once, the first one reports and the
/// others stop where they are, since the process is about to end.
pub(crate) fn panic(failure: Failure<'_>, location: Option<&Location>) -> ! {
    if PANICKING.swap(true, Ordering::AcqRel) {
        loop {
            std::thread::park();
        }
    }
    let text = report(failure, location) + &backtrace::capture();
    // A failed write cannot be reported anywhere; the exit status still says what happened.
    let _ = std::io::stderr().lock().write_all(text.as_bytes());
    std::process::exit(EXIT_STATUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The location the tests report.
    static HERE: Location = Location::new("heat.byn", 12, 9);

    #[test]
    fn reports_an_index_out_of_range_with_its_location() {
        let failure = Failure::IndexOutOfRange {
            index: 10,
            dimension: 0,
            length: 8,
        };
        assert_eq!(
            report(failure, Some(&HERE)),
            "panic: index 10 is out of range for dimension 0 of length 8\n  --> heat.byn:12:9\n"
        );
    }

    #[test]
    fn reports_each_failure_by_its_full_text() {
        let cases = [
            (
                Failure::Message("assertion failed: n > 0"),
                "assertion failed: n > 0",
            ),
            (
                Failure::LengthMismatch {
                    dimension: 1,
                    expected: 64,
                    found: 63,
                },
                "array lengths differ in dimension 1: 64 and 63",
            ),
            (
                Failure::NegativeExponent { exponent: -2 },
                "negative exponent -2 in integer `**`",
            ),
            (
                Failure::NegativeLength { count: -3 },
                "array length -3 is negative",
            ),
            (
                Failure::ArrayTooLarge {
                    count: i64::MAX,
                    element_size: 8,
                },
                "an array of 9223372036854775807 elements of 8 bytes is too large for memory",
            ),
            (
                Failure::OutOfMemory { bytes: 1 << 40 },
                "out of memory: could not allocate 1099511627776 bytes",
            ),
            (
                Failure::InvalidLayout { bytes: 8, align: 3 },
                "internal compiler error: no allocation has 8 bytes aligned to 3",
            ),
        ];
        for (failure, message) in cases {
            assert_eq!(report(failure, None), format!("panic: {message}\n"));
        }
    }
}
