//! Running another program (decision 0036, part 8): what `subprocess.run` does, in safe code the
//! exported function in `abi::subprocess` calls.
//!
//! [`run`] starts the program named by the first argument, found on `PATH`, waits for it, and
//! gives its exit status and, when asked, what it wrote. There is no shell: the arguments reach
//! the program exactly as given, so a file name with a space is one argument. Every failure is an
//! [`OsFailure`], which compiled code receives as an `OSError`.
//!
//! Three choices keep a child from stalling or ending the program that runs it:
//!
//! - **Captured output is read on two threads**, one per stream, while the program runs. A child
//!   that fills one pipe while the parent reads the other would otherwise wait forever.
//! - **`input` reaches the child as a file, not a pipe.** The text is written to a temporary file
//!   that is removed at once and stays open as the child's standard input. Writing to a pipe
//!   whose reader has gone raises `SIGPIPE`, which ends a Baryon program (decision 0036, part 6),
//!   so a child that exits without reading its input would end its parent too. A file needs no
//!   thread and has no such signal. The cost is one write of the text to the temporary directory.
//! - **A timeout ends the child**, then waits for it, so no finished child is left behind.

use crate::files::{self, ErrorKind, OsFailure};
use crate::print;
use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// `ENOTDIR`, the error number Linux and macOS both give for a path that is not a directory.
const ENOTDIR: i32 = 20;

/// The longest sleep between two looks at a child that has a timeout: the most a finished child
/// waits to be noticed.
const LONGEST_POLL: Duration = Duration::from_millis(20);

/// One call of `subprocess.run`: the program, its arguments and the keywords that were given.
#[derive(Clone, Debug, Default)]
pub(crate) struct Request<'a> {
    /// The program, then its arguments. Not empty.
    pub(crate) args: Vec<&'a str>,
    /// `capture_output`: whether to collect what the program writes instead of sharing the
    /// parent's standard output and standard error.
    pub(crate) capture: bool,
    /// `input`: text the program reads as its standard input, or `None` to share the parent's.
    pub(crate) input: Option<&'a str>,
    /// `cwd`: the directory the program runs in, or `None` for the parent's.
    pub(crate) cwd: Option<&'a str>,
    /// `env`: the program's whole environment, or `None` for a copy of the parent's.
    pub(crate) env: Option<Vec<(&'a str, &'a str)>>,
    /// `timeout`: how many seconds the program may run, or `None` for no limit.
    pub(crate) timeout: Option<f64>,
}

/// What a finished program left: the fields of a `CompletedProcess` other than `args`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Completed {
    /// The exit status, or minus the signal's number when a signal ended the program.
    pub(crate) returncode: i64,
    /// What the program wrote to standard output, `""` when it was not captured.
    pub(crate) stdout: String,
    /// What the program wrote to standard error, `""` when it was not captured.
    pub(crate) stderr: String,
}

/// Runs `request`'s program and waits for it. A program that exits with a status other than 0
/// is not a failure. Before the program starts, what `print` holds back is written out, so the
/// parent's output so far comes before the child's.
///
/// Cost: one new process, two threads while output is captured, and one temporary file when
/// `input` is given. Captured output is held in memory whole.
///
/// # Errors
///
/// Fails when the program cannot be started (`FILE_NOT_FOUND`, `PERMISSION`), when `cwd` is not
/// a directory, when the timeout passes (`TIMEOUT`, after the program is ended), and when
/// captured output is not UTF-8 (`INVALID_DATA`). Each failure names the program, or `cwd` when
/// that is what was wrong.
pub(crate) fn run(request: &Request<'_>) -> Result<Completed, OsFailure> {
    let program = request.args.first().copied().unwrap_or_default();
    let failed = |error: io::Error| OsFailure::from_io(&error, program);
    let mut command = Command::new(program);
    command.args(request.args.iter().skip(1));
    if let Some(cwd) = request.cwd {
        command.current_dir(directory(cwd)?);
    }
    if let Some(env) = &request.env {
        command.env_clear().envs(env.iter().copied());
    }
    if let Some(input) = request.input {
        command.stdin(input_file(input).map_err(failed)?);
    }
    if request.capture {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    print::flush();
    let mut child = command.spawn().map_err(failed)?;
    let readers = [
        child.stdout.take().map(reader).transpose(),
        child.stderr.take().map(reader).transpose(),
    ];
    let status = wait(&mut child, request.timeout, program)?;
    let [stdout, stderr] = readers;
    Ok(Completed {
        returncode: return_code(status),
        stdout: collect(stdout.map_err(failed)?, "standard output", program)?,
        stderr: collect(stderr.map_err(failed)?, "standard error", program)?,
    })
}

/// `cwd` as a path, once it is known to be a directory, so a wrong `cwd` is reported by its own
/// name and not as a missing program.
fn directory(cwd: &str) -> Result<&Path, OsFailure> {
    let metadata = fs::metadata(cwd).map_err(|error| OsFailure::from_io(&error, cwd))?;
    if metadata.is_dir() {
        Ok(Path::new(cwd))
    } else {
        Err(OsFailure::from_io(
            &io::Error::from_raw_os_error(ENOTDIR),
            cwd,
        ))
    }
}

/// How many input files this process has made, so each gets a name of its own.
static INPUT_FILES: AtomicU64 = AtomicU64::new(0);

/// A file holding `input`, open at its start and already removed from its directory, to be the
/// child's standard input. Only this user can read it while its name exists.
fn input_file(input: &str) -> io::Result<fs::File> {
    let number = INPUT_FILES.fetch_add(1, Ordering::Relaxed);
    let name = format!("baryon-input-{}-{number}", std::process::id());
    let path = std::env::temp_dir().join(name);
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&path)?;
    let written = file
        .write_all(input.as_bytes())
        .and_then(|()| file.rewind());
    fs::remove_file(&path)?;
    written.map(|()| file)
}

/// A thread that reads everything `pipe` gives, until the program closes it.
fn reader(mut pipe: impl Read + Send + 'static) -> io::Result<JoinHandle<io::Result<Vec<u8>>>> {
    thread::Builder::new()
        .name("baryon-subprocess-output".to_owned())
        .spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes).map(|_| bytes)
        })
}

/// Waits for `child` to end, for at most `timeout` seconds when one is given. A timeout that is
/// negative or not a number has already passed; one too large to count is no limit.
fn wait(child: &mut Child, timeout: Option<f64>, program: &str) -> Result<ExitStatus, OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, program);
    let Some(seconds) = timeout else {
        return child.wait().map_err(failed);
    };
    let limit = Duration::try_from_secs_f64(seconds.max(0.0));
    let Some(deadline) = limit
        .ok()
        .and_then(|limit| Instant::now().checked_add(limit))
    else {
        if seconds.is_nan() {
            return Err(timed_out(child, seconds, program));
        }
        return child.wait().map_err(failed);
    };
    let mut pause = Duration::from_micros(500);
    loop {
        if let Some(status) = child.try_wait().map_err(failed)? {
            return Ok(status);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(timed_out(child, seconds, program));
        }
        thread::sleep(pause.min(left));
        pause = (pause * 2).min(LONGEST_POLL);
    }
}

/// Ends `child`, waits for it so it is not left behind, and gives the `TIMEOUT` failure. The
/// threads reading its output end by themselves when the last holder of its pipes does.
fn timed_out(child: &mut Child, seconds: f64, program: &str) -> OsFailure {
    // A child that ended in the meantime cannot be killed; either way it is waited for.
    let _ = child.kill();
    let _ = child.wait();
    OsFailure::new(
        ErrorKind::Timeout,
        format!("timed out after {seconds} seconds"),
        program,
    )
}

/// What the thread `reader` collected, as text decoded as a text file is: UTF-8, with `\r\n`
/// and `\r` read as `\n`. `""` when the stream was not captured.
fn collect(
    reader: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stream: &str,
    program: &str,
) -> Result<String, OsFailure> {
    let Some(reader) = reader else {
        return Ok(String::new());
    };
    let bytes = reader
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("the reading thread stopped")))
        .map_err(|error| OsFailure::from_io(&error, program))?;
    files::decode(bytes, &mut 0, program).map_err(|failure| OsFailure {
        strerror: format!("{stream}: {}", failure.strerror),
        ..failure
    })
}

/// `status` as Python's `returncode`: the exit status, or minus the number of the signal that
/// ended the program.
fn return_code(status: ExitStatus) -> i64 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return -i64::from(signal);
    }
    i64::from(status.code().unwrap_or(-1))
}

#[cfg(test)]
mod tests;
