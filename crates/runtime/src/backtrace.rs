//! Backtraces: the chain of calls that led to a panic, as the source functions and lines a
//! programmer wrote.
//!
//! The backend writes DWARF line tables (roadmap 1.10.9) and the driver links with them, so the
//! standard library's backtrace can name each frame of compiled code and its `.byn` line, inlined
//! calls included. The capture also holds frames that are not the program's: the runtime's own
//! functions, the C `main` and the C library. Only the frames whose location is a `.byn` file are
//! the programmer's, so the report keeps those, most recent call last as Python's traceback
//! lists them (decision 0004):
//!
//! ```text
//! backtrace, most recent call last:
//!   main.main at /home/ada/heat/main.byn:11:11
//!   main.pick at /home/ada/heat/main.byn:2:19
//! ```
//!
//! A program built without debug information has no `.byn` frames, and its report has no
//! backtrace. Capturing and resolving one reads the program's debug information from disk, which
//! takes milliseconds; it happens once, as the program ends.

use std::fmt::Write as _;

/// One frame of compiled code: the function and where in the source it was.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame<'text> {
    /// The function's symbol, such as `main.pick`.
    function: &'text str,
    /// The file, line and column, such as `/home/ada/heat/main.byn:2:19`.
    location: &'text str,
}

/// The backtrace of the calling thread, as the lines a panic report ends with, or nothing when
/// no frame is the program's.
pub(crate) fn capture() -> String {
    render(&std::backtrace::Backtrace::force_capture().to_string())
}

/// The report's lines for the `.byn` frames of `backtrace`, the standard library's text of a
/// captured backtrace, or nothing when there are none.
///
/// That text gives each frame's function on one line, `  2: main.pick`, and its location on the
/// next, `             at /home/ada/heat/main.byn:2:19`. A function inlined into another is a
/// line of its own, with or without a number, so every function line is read the same way.
fn render(backtrace: &str) -> String {
    let frames = frames(backtrace);
    if frames.is_empty() {
        return String::new();
    }
    let mut text = String::from("backtrace, most recent call last:\n");
    for frame in frames.iter().rev() {
        let _ = writeln!(text, "  {} at {}", frame.function, frame.location);
    }
    text
}

/// Each frame of `backtrace` whose location is in a `.byn` file, in order.
fn frames(backtrace: &str) -> Vec<Frame<'_>> {
    let mut frames = Vec::new();
    let mut function = None;
    for line in backtrace.lines() {
        let line = line.trim();
        if let Some(location) = line.strip_prefix("at ") {
            if let Some(function) = function.take()
                && is_source(location)
            {
                frames.push(Frame { function, location });
            }
        } else {
            function = Some(match line.split_once(": ") {
                Some((number, name)) if number.bytes().all(|byte| byte.is_ascii_digit()) => name,
                _ => line,
            });
        }
    }
    frames
}

/// Whether `location`, written `file:line:column`, is in a `.byn` file.
fn is_source(location: &str) -> bool {
    let mut parts = location.rsplitn(3, ':');
    let (Some(_column), Some(_line), Some(file)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    std::path::Path::new(file)
        .extension()
        .is_some_and(|extension| extension == "byn")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard library's text for a panic in `main.pick`, which `main.total` called and
    /// which was inlined into `main.main`.
    const CAPTURED: &str = "   0: runtime::panic::panic
             at ./crates/runtime/src/panic.rs:138:36
   1: baryon_rt_panic_index
             at ./crates/runtime/src/abi.rs:238:5
   2: main.pick
             at /home/ada/heat/main.byn:2:19
      main.total
             at /home/ada/heat/main.byn:7:12
   3: main.main
             at /home/ada/heat/main.byn:11:11
   4: main
   5: __libc_start_main
";

    #[test]
    fn keeps_the_frames_of_baryon_source_most_recent_last() {
        assert_eq!(
            render(CAPTURED),
            "backtrace, most recent call last:\n  \
             main.main at /home/ada/heat/main.byn:11:11\n  \
             main.total at /home/ada/heat/main.byn:7:12\n  \
             main.pick at /home/ada/heat/main.byn:2:19\n"
        );
    }

    #[test]
    fn a_program_without_debug_information_has_no_backtrace() {
        let stripped = "   0: runtime::panic::panic\n   1: main.pick\n   2: main\n";
        assert_eq!(render(stripped), "");
    }

    #[test]
    fn a_location_needs_a_line_and_a_column() {
        assert!(is_source("main.byn:2:19"));
        assert!(is_source("C:/heat/main.byn:2:19"));
        assert!(!is_source("main.byn"));
        assert!(!is_source("./crates/runtime/src/panic.rs:138:36"));
    }

    #[test]
    fn a_frame_without_a_location_is_not_given_the_next_one() {
        let text = "   0: baryon_rt_panic\n   1: main\n             at /heat/main.byn:3:1\n";
        assert_eq!(
            frames(text),
            [Frame {
                function: "main",
                location: "/heat/main.byn:3:1"
            }]
        );
    }
}
