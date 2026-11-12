//! `--timings`: how long each stage of a build took, printed so a compile-speed change can show
//! its gain in numbers (roadmap 2.0).
//!
//! A build runs four stages one after another, and [`Timings`] holds the wall-clock time of
//! each. Wall-clock time is what a user waits for, so it is what the compile-speed targets in
//! the design plan are written in.

use std::fmt;
use std::time::Duration;

/// The wall-clock time of each stage of one build, and the size of the package it built.
///
/// The stages run in order and do not overlap, so their sum is the build's total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Timings {
    /// The number of `.byn` files in the package.
    pub(crate) files: usize,
    /// The number of lines in those files. Standard library modules the program uses, such as
    /// `linalg`, are compiled too but not counted, so lines per second is a lower bound.
    pub(crate) lines: usize,
    /// Finding and reading the package's files.
    pub(crate) load: Duration,
    /// Checking the package, lowering it to the mid-level IR, the IR passes and writing the LLVM
    /// module as text: everything inside `baryon` itself.
    pub(crate) compile: Duration,
    /// `clang` optimizing the LLVM module and writing machine code into an object file.
    pub(crate) machine_code: Duration,
    /// `clang` linking the object file with the runtime, and on macOS `dsymutil` collecting the
    /// line tables.
    pub(crate) link: Duration,
}

impl Timings {
    /// The whole build: the sum of the stages.
    fn total(&self) -> Duration {
        self.load + self.compile + self.machine_code + self.link
    }
}

impl fmt::Display for Timings {
    /// Writes one line per stage in milliseconds, then the total with the package's lines per
    /// second, which the design plan's target of 100,000 for a debug build is measured in:
    ///
    /// ```text
    /// timings for 106 line(s) in 1 file(s):
    ///   load              1 ms
    ///   compile          84 ms  check, lower, IR passes, LLVM text
    ///   machine code    557 ms  clang: optimize, generate code
    ///   link            156 ms  clang: link with the runtime
    ///   total           798 ms  132 lines/s
    /// ```
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self.total();
        let per_second = if total.is_zero() {
            0
        } else {
            // A package has far fewer lines than an `f64` holds exactly, and a rate is shown
            // rounded down to whole lines anyway.
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let rate = (self.lines as f64 / total.as_secs_f64()) as u64;
            rate
        };
        let row = |formatter: &mut fmt::Formatter<'_>, name: &str, time: Duration, note: &str| {
            write!(formatter, "  {name:<12} {:>6} ms", time.as_millis())?;
            if note.is_empty() {
                writeln!(formatter)
            } else {
                writeln!(formatter, "  {note}")
            }
        };
        writeln!(
            formatter,
            "timings for {} line(s) in {} file(s):",
            self.lines, self.files
        )?;
        row(formatter, "load", self.load, "")?;
        row(
            formatter,
            "compile",
            self.compile,
            "check, lower, IR passes, LLVM text",
        )?;
        row(
            formatter,
            "machine code",
            self.machine_code,
            "clang: optimize, generate code",
        )?;
        row(formatter, "link", self.link, "clang: link with the runtime")?;
        write!(
            formatter,
            "  {:<12} {:>6} ms  {per_second} lines/s",
            "total",
            total.as_millis()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Timings;
    use std::time::Duration;

    #[test]
    fn renders_one_line_per_stage_and_the_rate() {
        let timings = Timings {
            files: 1,
            lines: 106,
            load: Duration::from_millis(1),
            compile: Duration::from_millis(84),
            machine_code: Duration::from_millis(557),
            link: Duration::from_millis(158),
        };
        assert_eq!(
            timings.to_string(),
            "timings for 106 line(s) in 1 file(s):\n  \
             load              1 ms\n  \
             compile          84 ms  check, lower, IR passes, LLVM text\n  \
             machine code    557 ms  clang: optimize, generate code\n  \
             link            158 ms  clang: link with the runtime\n  \
             total           800 ms  132 lines/s"
        );
    }

    #[test]
    fn an_instant_build_reports_no_rate_instead_of_dividing_by_zero() {
        let timings = Timings {
            files: 1,
            lines: 3,
            ..Timings::default()
        };
        assert!(
            timings
                .to_string()
                .ends_with("total             0 ms  0 lines/s")
        );
    }
}
