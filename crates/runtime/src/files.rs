//! Text files (decision 0036, parts 3 and 4): what `open` returns and what its methods do, in
//! safe code the exported functions in `abi::files` call.
//!
//! A file is open for reading (`"r"`) or for writing (`"w"`, `"a"` and `"x"`), never both. Reading
//! goes through a 64 KiB buffer and decodes UTF-8, translating `\r\n` and `\r` to `\n` as
//! Python's universal newlines do; writing goes through a 64 KiB buffer and writes `\n` as it is.
//! Every failure is an [`OsFailure`], which compiled code receives as an `OSError`.
//!
//! An open file that is written to is listed in a registry, so `sys.exit`, which runs no drops,
//! can still write out every file's buffer first ([`flush_all`]).
//!
//! `sys.stdin`, `sys.stdout` and `sys.stderr` are files too (decision 0036, part 6): an
//! [`OpenFile`] over a [`Standard`] stream, which the `streams` module reads and writes. Such a
//! file is a handle on a stream the process owns, so closing or dropping it leaves the stream
//! open.

use crate::streams::{self, Standard};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::sync::Mutex;

pub(crate) mod arrays;
pub(crate) mod fits;
pub(crate) mod npy;

/// The size of a file's buffer: one system call per 64 KiB read or written.
const BUFFER: usize = 64 * 1024;

/// What went wrong: the position of a member of `os.ErrorKind`, in the order
/// `crates/driver/library/os.byn` declares them. Compiled code reads it as that enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ErrorKind {
    /// `FILE_NOT_FOUND`: no file or directory at the path.
    FileNotFound = 0,
    /// `FILE_EXISTS`: mode `"x"` found a file already there.
    FileExists = 1,
    /// `PERMISSION`: the operating system refused.
    Permission = 2,
    /// `IS_A_DIRECTORY`: the path is a directory.
    IsADirectory = 3,
    /// `NOT_A_DIRECTORY`: a directory in the path is a file.
    NotADirectory = 4,
    /// `BROKEN_PIPE`: the reader at the other end of a pipe has gone.
    BrokenPipe = 5,
    /// `INTERRUPTED`: a signal interrupted the call.
    Interrupted = 6,
    /// `TIMEOUT`: the operation took too long.
    Timeout = 7,
    /// `INVALID_DATA`: text that is not UTF-8.
    InvalidData = 8,
    /// `OTHER`: anything else, such as a full disk.
    Other = 9,
}

/// One failure, as an `OSError` holds it: the kind, the operating system's error number and
/// message, and the path involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OsFailure {
    /// What went wrong.
    pub(crate) kind: ErrorKind,
    /// The operating system's error number, 0 when there is none.
    pub(crate) errno: i32,
    /// The message, such as `No such file or directory`.
    pub(crate) strerror: String,
    /// The path involved.
    pub(crate) filename: String,
}

impl OsFailure {
    /// The failure an I/O error from the standard library describes, for the file at `path`.
    pub(crate) fn from_io(error: &io::Error, path: &str) -> Self {
        let errno = error.raw_os_error().unwrap_or(0);
        let kind = match error.kind() {
            io::ErrorKind::NotFound => ErrorKind::FileNotFound,
            io::ErrorKind::AlreadyExists => ErrorKind::FileExists,
            io::ErrorKind::PermissionDenied => ErrorKind::Permission,
            io::ErrorKind::IsADirectory => ErrorKind::IsADirectory,
            io::ErrorKind::NotADirectory => ErrorKind::NotADirectory,
            io::ErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
            io::ErrorKind::Interrupted => ErrorKind::Interrupted,
            io::ErrorKind::TimedOut => ErrorKind::Timeout,
            io::ErrorKind::InvalidData => ErrorKind::InvalidData,
            _ => ErrorKind::Other,
        };
        // The standard library writes "No such file or directory (os error 2)"; Python's
        // `strerror` is the message alone.
        let message = error.to_string();
        let strerror = match message.rfind(" (os error ") {
            Some(end) if errno != 0 => message[..end].to_owned(),
            _ => message,
        };
        Self {
            kind,
            errno,
            strerror,
            filename: path.to_owned(),
        }
    }

    /// A failure with no error number, such as reading a file opened for writing.
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>, path: &str) -> Self {
        Self {
            kind,
            errno: 0,
            strerror: message.into(),
            filename: path.to_owned(),
        }
    }
}

/// How `open` opens a file: its `mode` argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Mode {
    /// `"r"`: read an existing file.
    Read = 0,
    /// `"w"`: write a new file, or empty an existing one.
    Write = 1,
    /// `"a"`: write at the end, making the file if it is missing.
    Append = 2,
    /// `"x"`: write a new file, failing if one is there.
    Create = 3,
}

impl Mode {
    /// The mode with the number compiled code passes, or `None` for any other number.
    pub(crate) fn from_number(number: u8) -> Option<Self> {
        [Self::Read, Self::Write, Self::Append, Self::Create]
            .into_iter()
            .find(|&mode| mode as u8 == number)
    }
}

/// What an open file reads from or writes to.
#[derive(Debug)]
enum Stream {
    /// A file opened with `"r"`.
    Reader {
        /// The file, behind its buffer.
        reader: BufReader<fs::File>,
        /// How many bytes have been read, for the offset an `INVALID_DATA` error names.
        offset: u64,
    },
    /// A file opened with `"w"`, `"a"` or `"x"`, behind its buffer.
    Writer(BufWriter<fs::File>),
    /// One of the process's standard streams, which the process buffers, not the file.
    Standard(Standard),
}

/// A file `open` opened: what compiled code's `File` points to. Compiled code holds only a
/// pointer to one and never reads inside it.
#[derive(Debug)]
pub struct OpenFile {
    /// The path as the program gave it, for errors.
    path: String,
    /// The file.
    stream: Stream,
}

/// The files open for writing, by the address of their [`OpenFile`], so [`flush_all`] can write
/// out each buffer. A file is listed when it opens and removed when it closes.
static WRITERS: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());

impl OpenFile {
    /// Opens the file at `path` in `mode`.
    ///
    /// # Errors
    ///
    /// Fails as the operating system does: no such file for `"r"`, a file already there for
    /// `"x"`, no permission, a directory.
    pub(crate) fn open(path: &str, mode: Mode) -> Result<Box<Self>, OsFailure> {
        let mut options = fs::OpenOptions::new();
        match mode {
            Mode::Read => options.read(true),
            Mode::Write => options.write(true).create(true).truncate(true),
            Mode::Append => options.append(true).create(true),
            Mode::Create => options.write(true).create_new(true),
        };
        let file = options
            .open(path)
            .map_err(|error| OsFailure::from_io(&error, path))?;
        // Opening a directory for reading succeeds on Linux and macOS; Python reports it.
        if mode == Mode::Read && file.metadata().is_ok_and(|metadata| metadata.is_dir()) {
            return Err(OsFailure::from_io(
                &io::Error::from_raw_os_error(EISDIR),
                path,
            ));
        }
        let stream = if mode == Mode::Read {
            Stream::Reader {
                reader: BufReader::with_capacity(BUFFER, file),
                offset: 0,
            }
        } else {
            Stream::Writer(BufWriter::with_capacity(BUFFER, file))
        };
        let file = Box::new(Self {
            path: path.to_owned(),
            stream,
        });
        if mode != Mode::Read {
            writers().insert(file.address());
        }
        Ok(file)
    }

    /// A handle on the standard stream `standard`, as `sys.stdin`, `sys.stdout` or `sys.stderr`
    /// gives: its path is the name Python gives the stream, such as `<stdout>`.
    pub(crate) fn standard(standard: Standard) -> Self {
        Self {
            path: standard.name().to_owned(),
            stream: Stream::Standard(standard),
        }
    }

    /// The address the registry lists the file by.
    fn address(&self) -> usize {
        std::ptr::from_ref(self) as usize
    }

    /// Runs `read` on what the file reads from, with the count of bytes read so far and the
    /// path, or gives the failure Python reports for reading a file opened for writing.
    fn reading<T>(
        &mut self,
        read: impl FnOnce(&mut dyn BufRead, &mut u64, &str) -> Result<T, OsFailure>,
    ) -> Result<T, OsFailure> {
        let path = &self.path;
        match &mut self.stream {
            Stream::Reader { reader, offset } => read(reader, offset, path),
            Stream::Standard(Standard::Input) => {
                streams::with_input(|reader, offset| read(reader, offset, path))
            }
            Stream::Writer(_) | Stream::Standard(_) => {
                Err(OsFailure::new(ErrorKind::Other, "not readable", path))
            }
        }
    }

    /// The rest of the file, `f.read()`.
    ///
    /// # Errors
    ///
    /// Fails when the file was opened for writing, the read fails, or the bytes are not UTF-8.
    pub(crate) fn read(&mut self) -> Result<String, OsFailure> {
        self.reading(|reader, offset, path| {
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .map_err(|error| OsFailure::from_io(&error, path))?;
            decode(bytes, offset, path)
        })
    }

    /// The next line with its `\n`, or `""` at the end: `f.readline()`. A line ends at `\n`,
    /// `\r\n` or `\r`, and each ends in `\n` here.
    ///
    /// # Errors
    ///
    /// Fails as [`read`](Self::read) does.
    pub(crate) fn read_line(&mut self) -> Result<String, OsFailure> {
        self.reading(|reader, offset, path| {
            let bytes =
                streams::line_bytes(reader).map_err(|error| OsFailure::from_io(&error, path))?;
            decode(bytes, offset, path)
        })
    }

    /// The remaining lines, `f.readlines()`, each as [`read_line`](Self::read_line) gives it.
    ///
    /// # Errors
    ///
    /// Fails as [`read`](Self::read) does.
    pub(crate) fn read_lines(&mut self) -> Result<Vec<String>, OsFailure> {
        let mut lines = Vec::new();
        loop {
            let line = self.read_line()?;
            if line.is_empty() {
                return Ok(lines);
            }
            lines.push(line);
        }
    }

    /// Writes `text` and returns how many characters it holds, `f.write(text)`.
    ///
    /// # Errors
    ///
    /// Fails when the file was opened for reading or the write fails.
    pub(crate) fn write(&mut self, text: &str) -> Result<i64, OsFailure> {
        let written = match &mut self.stream {
            Stream::Writer(writer) => writer.write_all(text.as_bytes()),
            Stream::Standard(standard) if *standard != Standard::Input => {
                standard.write(text.as_bytes())
            }
            Stream::Reader { .. } | Stream::Standard(_) => {
                return Err(OsFailure::new(ErrorKind::Other, "not writable", &self.path));
            }
        };
        written.map_err(|error| OsFailure::from_io(&error, &self.path))?;
        Ok(i64::try_from(text.chars().count()).unwrap_or(i64::MAX))
    }

    /// Writes out the buffer, `f.flush()`. A file opened for reading has nothing to write.
    ///
    /// # Errors
    ///
    /// Fails when the write fails, such as on a full disk.
    pub(crate) fn flush(&mut self) -> Result<(), OsFailure> {
        let flushed = match &mut self.stream {
            Stream::Reader { .. } => Ok(()),
            Stream::Writer(writer) => writer.flush(),
            Stream::Standard(standard) => standard.flush(),
        };
        flushed.map_err(|error| OsFailure::from_io(&error, &self.path))
    }

    /// Writes out the buffer and closes the file, `f.close()` and the end of a `with` block. A
    /// standard stream is written out and stays open: the handle ends, not the stream.
    ///
    /// # Errors
    ///
    /// Fails as [`flush`](Self::flush) does; the file is closed either way.
    #[allow(
        clippy::boxed_local,
        reason = "the registry lists the file by its address inside the box"
    )]
    pub(crate) fn close(mut self: Box<Self>) -> Result<(), OsFailure> {
        let flushed = self.flush();
        writers().remove(&self.address());
        flushed
    }

    /// Ends a file that was dropped, not closed: [`close`](Self::close) for a file `open`
    /// opened. A handle on a standard stream is only freed, with no write: the process writes
    /// its streams out when it ends, and a write for each `sys.stdout.write(...)` statement,
    /// whose handle is dropped at once, would cost a system call each.
    ///
    /// # Errors
    ///
    /// Fails as [`close`](Self::close) does.
    pub(crate) fn release(self: Box<Self>) -> Result<(), OsFailure> {
        if matches!(self.stream, Stream::Standard(_)) {
            return Ok(());
        }
        self.close()
    }

    /// The path, for the panic a failed final write on a drop reports.
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
}

/// `EISDIR`, the error number Linux and macOS both give for a directory where a file is needed.
const EISDIR: i32 = 21;

/// The registry of files open for writing. A thread that panicked while holding it left it as
/// it was, which is still a valid map.
fn writers() -> std::sync::MutexGuard<'static, BTreeSet<usize>> {
    WRITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes out the buffer of every file open for writing, for `sys.exit`, which ends the program
/// without dropping anything. A write that fails here cannot be reported, as at the end of a C
/// program.
pub(crate) fn flush_all() {
    flush_where(|_| true);
}

/// Writes out the buffer of each file open for writing whose address `chosen` accepts.
/// [`flush_all`] chooses every file; a test chooses only its own, because the files other
/// tests write at the same moment are not this thread's to touch.
fn flush_where(chosen: impl Fn(usize) -> bool) {
    for &address in writers().iter().filter(|&&address| chosen(address)) {
        // SAFETY: a listed address is a live `OpenFile`: it is removed before the file is
        // freed, and `sys.exit` runs on a thread that holds no reference to it.
        #[allow(unsafe_code, reason = "flushes files compiled code owns, at exit")]
        let file = unsafe { &mut *(address as *mut OpenFile) };
        let _ = file.flush();
    }
}

/// `bytes` as text, or an `INVALID_DATA` failure naming the byte offset of the first bad
/// sequence. `offset` counts the bytes read before these and is advanced past them; `\r\n` and
/// `\r` become `\n`. Captured output of `subprocess.run` is decoded the same way, with the
/// program as `path`.
pub(crate) fn decode(bytes: Vec<u8>, offset: &mut u64, path: &str) -> Result<String, OsFailure> {
    let start = *offset;
    *offset += u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let text = String::from_utf8(bytes).map_err(|error| {
        let at = start + u64::try_from(error.utf8_error().valid_up_to()).unwrap_or(u64::MAX);
        OsFailure::new(
            ErrorKind::InvalidData,
            format!("invalid UTF-8 at byte {at}"),
            path,
        )
    })?;
    Ok(if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    })
}

#[cfg(test)]
mod tests;
