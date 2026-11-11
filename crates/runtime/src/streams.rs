//! The standard streams (decision 0036, part 6): what `sys.stdin`, `sys.stdout` and `sys.stderr`
//! read and write, and `input`.
//!
//! The process has one of each stream, however many handles a program makes, so their state
//! lives here and not in a file: Rust's standard input holds the bytes read ahead, and Rust's
//! standard output holds a line until it ends, the same buffer `print` writes to, so text from
//! `print` and from `sys.stdout.write` comes out in the order the program wrote it. Standard
//! error is not buffered. Each read or write locks its stream for that one call, so threads of a
//! `prange` loop can share them.
//!
//! [`line_bytes`] is how every text file, a standard stream included, finds the end of a line.

use std::io::{self, BufRead, Write};
use std::sync::Mutex;

/// One of the three streams a process starts with, numbered as POSIX numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Standard {
    /// Standard input, `sys.stdin`: read only.
    Input = 0,
    /// Standard output, `sys.stdout`: where `print` writes.
    Output = 1,
    /// Standard error, `sys.stderr`.
    Error = 2,
}

impl Standard {
    /// The stream with the number compiled code passes, or `None` for any other number.
    pub(crate) fn from_number(number: u8) -> Option<Self> {
        [Self::Input, Self::Output, Self::Error]
            .into_iter()
            .find(|&standard| standard as u8 == number)
    }

    /// The name Python gives the stream, `sys.stdout.name`, which an `OSError` names as the
    /// file.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Input => "<stdin>",
            Self::Output => "<stdout>",
            Self::Error => "<stderr>",
        }
    }

    /// Writes `bytes` to standard output or standard error.
    ///
    /// # Errors
    ///
    /// Fails when the operating system refuses the write, and for standard input, which is
    /// not writable.
    pub(crate) fn write(self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Input => Err(io::Error::other("not writable")),
            Self::Output => io::stdout().lock().write_all(bytes),
            Self::Error => io::stderr().lock().write_all(bytes),
        }
    }

    /// Writes out what the stream holds back: the unended line of standard output. Standard
    /// error holds nothing, and standard input has nothing to write.
    ///
    /// # Errors
    ///
    /// Fails when the operating system refuses the write.
    pub(crate) fn flush(self) -> io::Result<()> {
        match self {
            Self::Input | Self::Error => Ok(()),
            Self::Output => io::stdout().lock().flush(),
        }
    }
}

/// How many bytes the program has read from standard input, for the offset an `INVALID_DATA`
/// error names. One count for the process, as there is one standard input.
static INPUT_OFFSET: Mutex<u64> = Mutex::new(0);

/// Runs `read` on standard input, locked, with the count of bytes read from it so far, which
/// `read` advances.
pub(crate) fn with_input<T>(read: impl FnOnce(&mut dyn BufRead, &mut u64) -> T) -> T {
    // A thread that panicked while reading left the count as it was, which is still a count.
    let mut offset = INPUT_OFFSET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    read(&mut io::stdin().lock(), &mut offset)
}

/// The bytes of the next line of `reader`, through its line end, or no bytes at the end. A line
/// ends at `\n`, `\r\n` or `\r`, as Python's universal newlines read it; the bytes come back as
/// they are, so a caller turns `\r\n` and `\r` into `\n` after decoding.
///
/// A `\r` that ends a buffer waits for the next byte, to see whether a `\n` belongs to it.
///
/// # Errors
///
/// Fails when the read fails.
pub(crate) fn line_bytes(reader: &mut dyn BufRead) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        let Some(&first) = buffer.first() else {
            break;
        };
        if bytes.last() == Some(&b'\r') {
            // A `\r` ended the line; a `\n` right after it belongs to it.
            if first == b'\n' {
                reader.consume(1);
                bytes.push(b'\n');
            }
            break;
        }
        match buffer
            .iter()
            .position(|&byte| byte == b'\n' || byte == b'\r')
        {
            Some(end) if buffer[end] == b'\n' => {
                bytes.extend_from_slice(&buffer[..=end]);
                reader.consume(end + 1);
                break;
            }
            Some(end) => {
                // A `\r`: look at the next byte, which may be in the next buffer.
                bytes.extend_from_slice(&buffer[..=end]);
                reader.consume(end + 1);
            }
            None => {
                let length = buffer.len();
                bytes.extend_from_slice(buffer);
                reader.consume(length);
            }
        }
    }
    Ok(bytes)
}

/// `input(prompt)`: writes `prompt` to standard output and writes standard output out, so the
/// prompt shows before the program waits, then gives the next line of standard input without
/// its line end. `None` at the end of the input, where Python raises `EOFError`.
pub(crate) fn input(prompt: &str) -> Option<String> {
    // A prompt that cannot be written is ignored, as a failed `print` is.
    let _ = Standard::Output.write(prompt.as_bytes());
    let _ = Standard::Output.flush();
    with_input(|reader, offset| {
        let line = input_line(reader)?;
        *offset += u64::try_from(line.len()).unwrap_or(u64::MAX);
        Some(line)
    })
    .map(|line| String::from_utf8_lossy(without_line_end(&line)).into_owned())
}

/// The bytes of the next line of `reader` for `input`, or `None` at the end. A read that
/// fails ends the input: `input` raises only `EOFError` (decision 0036, part 6).
fn input_line(reader: &mut dyn BufRead) -> Option<Vec<u8>> {
    line_bytes(reader).ok().filter(|line| !line.is_empty())
}

/// `line` without the `\n`, `\r\n` or `\r` that ends it.
fn without_line_end(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::{Standard, input_line, line_bytes, without_line_end};
    use std::io::{BufReader, Cursor};

    /// The lines `line_bytes` finds in `text`, read through a buffer of `capacity` bytes.
    fn lines(text: &str, capacity: usize) -> Vec<String> {
        let mut reader = BufReader::with_capacity(capacity, Cursor::new(text.as_bytes()));
        std::iter::from_fn(|| {
            let line = line_bytes(&mut reader).unwrap();
            (!line.is_empty()).then(|| String::from_utf8(line).unwrap())
        })
        .collect()
    }

    #[test]
    fn a_line_ends_at_any_line_end_wherever_the_buffer_splits_it() {
        for capacity in 1..8 {
            assert_eq!(
                lines("a\r\nbc\r\rd\n\ne", capacity),
                ["a\r\n", "bc\r", "\r", "d\n", "\n", "e"],
                "capacity {capacity}"
            );
        }
        assert!(lines("", 4).is_empty());
    }

    #[test]
    fn input_reads_a_line_without_its_end_and_stops_at_the_end() {
        let mut reader = Cursor::new(b"64\r\n\nlast".as_slice());
        let mut read = || input_line(&mut reader).map(|line| without_line_end(&line).to_vec());
        assert_eq!(read().as_deref(), Some(b"64".as_slice()));
        assert_eq!(read().as_deref(), Some(b"".as_slice()));
        assert_eq!(read().as_deref(), Some(b"last".as_slice()));
        assert_eq!(read(), None);
    }

    #[test]
    fn streams_are_numbered_and_named_as_posix_and_python_do() {
        assert_eq!(Standard::from_number(0), Some(Standard::Input));
        assert_eq!(Standard::from_number(2), Some(Standard::Error));
        assert_eq!(Standard::from_number(3), None);
        assert_eq!(Standard::Output.name(), "<stdout>");
        assert_eq!(
            Standard::Input.write(b"x").unwrap_err().to_string(),
            "not writable"
        );
        assert!(Standard::Input.flush().is_ok());
        assert!(Standard::Error.flush().is_ok());
    }
}
