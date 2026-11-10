//! The functions compiled code calls to end the program with a panic: a message it wrote in
//! full, an index or slice out of range, arrays of different lengths, and a negative exponent.

use super::location;
use crate::Location;
use crate::panic::{Failure, panic};

/// Ends the program with a panic that prints `message` and `location`. Compiled code calls it
/// for every check whose message it can write in full: an overflow, a division by zero, a failed
/// `assert` with its condition's source.
///
/// The program prints `panic: <message>`, then `  --> <file>:<line>:<column>`, to standard
/// error, and exits with status 101. When several threads of a parallel loop panic at once, one
/// message is printed. A message that is not UTF-8, which only a compiler bug writes, is printed
/// with replacement characters.
///
/// # Safety
///
/// `message` points to `length` readable bytes, and `location` is null or points to a valid
/// [`Location`] that lives as long as the program.
///
/// # Example
///
/// What compiled code does when `assert n > 0` fails at `heat.byn:12:5`:
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic};
///
/// static HERE: Location = Location::new("heat.byn", 12, 5);
/// let message = "assertion failed: n > 0";
/// // SAFETY: the message and the location are constants that live as long as the program.
/// unsafe { baryon_rt_panic(message.as_ptr(), message.len(), &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic(
    message: *const u8,
    length: usize,
    location: *const Location,
) -> ! {
    // SAFETY: the caller guarantees `message` points to `length` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(message, length) };
    let message = String::from_utf8_lossy(bytes);
    // SAFETY: the caller's promise about `location`, passed on.
    panic(Failure::Message(&message), unsafe {
        self::location(location)
    })
}

/// Ends the program with a panic for an array index out of range: `index` is negative, or not
/// below the `length` of `dimension`, 0 for the outermost (decision 0010). Compiled code calls it on the failing side of
/// an IR `IndexInBounds` check.
///
/// Prints `panic: index <index> is out of range for dimension <dimension> of length <length>` and the location, as
/// [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_index};
///
/// static HERE: Location = Location::new("md.byn", 58, 13);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_index(10, 0, 8, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_index(
    index: i64,
    dimension: u32,
    length: i64,
    location: *const Location,
) -> ! {
    let failure = Failure::IndexOutOfRange {
        index,
        dimension,
        length,
    };
    // SAFETY: the caller's promise about `location`, passed on.
    panic(failure, unsafe { self::location(location) })
}

/// Ends the program with a panic for a slice whose range does not lie within its dimension:
/// `start` or `stop` is negative or above the `length` of `dimension`, 0 for the outermost, or
/// the range takes an element and `start` is not below the length (language reference,
/// "Slicing"). Compiled code calls it on the failing side of an IR `SliceInBounds` check.
///
/// Prints `panic: slice <start>:<stop> is out of range for dimension <dimension> of length
/// <length>` and the location, as [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_slice};
///
/// static HERE: Location = Location::new("heat.byn", 21, 9);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_slice(2, 12, 0, 10, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_slice(
    start: i64,
    stop: i64,
    dimension: u32,
    length: i64,
    location: *const Location,
) -> ! {
    let failure = Failure::SliceOutOfRange {
        start,
        stop,
        dimension,
        length,
    };
    // SAFETY: the caller's promise about `location`, passed on.
    panic(failure, unsafe { self::location(location) })
}

/// Ends the program with a panic for arrays combined element by element whose lengths differ
/// in `dimension`: the first has `expected` elements there, the other `found`. Compiled code
/// calls it on the failing side of an IR `SameLength` check.
///
/// Prints `panic: array lengths differ in dimension <dimension>: <expected> and <found>` and the
/// location, as [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_length_mismatch};
///
/// static HERE: Location = Location::new("heat.byn", 30, 11);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_length_mismatch(0, 64, 63, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_length_mismatch(
    dimension: u32,
    expected: i64,
    found: i64,
    location: *const Location,
) -> ! {
    let failure = Failure::LengthMismatch {
        dimension,
        expected,
        found,
    };
    // SAFETY: the caller's promise about `location`, passed on.
    panic(failure, unsafe { self::location(location) })
}

/// Ends the program with a panic for integer `**` with a negative `exponent`, whose result would
/// be a fraction (language reference, "Errors and panics"). Compiled code calls it on the failing
/// side of an IR `NonNegativeExponent` check.
///
/// Prints ``panic: negative exponent <exponent> in integer `**` `` and the location, as
/// [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// What compiled code does for `2 ** n` with `n` equal to -1:
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_exponent};
///
/// static HERE: Location = Location::new("main.byn", 4, 11);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_exponent(-1, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_exponent(exponent: i64, location: *const Location) -> ! {
    // SAFETY: the caller's promise about `location`, passed on.
    panic(Failure::NegativeExponent { exponent }, unsafe {
        self::location(location)
    })
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Set in the environment of the child process that is meant to panic.
    const CHILD: &str = "BARYON_RUNTIME_TEST_PANIC_CHILD";

    /// The location the child reports.
    static HERE: Location = Location::new("md.byn", 58, 13);

    /// A panic ends the process, so the test runs itself again as a child that panics, and checks
    /// what the child printed and how it exited.
    #[test]
    fn a_panic_prints_its_message_and_exits_with_status_101() {
        if std::env::var_os(CHILD).is_some() {
            // SAFETY: `HERE` is a static location.
            unsafe { baryon_rt_panic_index(10, 0, 8, &raw const HERE) }
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "abi::panics::tests::a_panic_prints_its_message_and_exits_with_status_101",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(101));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "panic: index 10 is out of range for dimension 0 of length 8\n  --> md.byn:58:13\n"
        );
    }
}
