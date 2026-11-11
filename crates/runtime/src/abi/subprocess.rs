//! The function compiled code calls for `subprocess.run` (decision 0036, part 8).
//!
//! It can fail, so it follows the convention of the file functions (the `files` module): its last
//! parameter is the address of the caller's `Raises[None, OSError]`, it returns `true` with the
//! error written there when it fails, and writes its result through `out` and returns `false`
//! when it succeeds. The result is a [`CompletedProcess`], laid out as
//! `class CompletedProcess` in `<std>/subprocess.byn` declares it (decision 0038).

use super::files::{RaisedError, finish};
use super::string_lists::{OwnedList, TextValue, write_list};
use super::strings::text;
use super::text::{OwnedText, owned};
use crate::dict::Table;
use crate::location::Location;
use crate::panic::{Failure, panic};
use crate::subprocess::{self, Completed, Request};

/// The bit of `given` that says `input=` was written.
const GIVEN_INPUT: u8 = 1;
/// The bit of `given` that says `cwd=` was written.
const GIVEN_CWD: u8 = 2;
/// The bit of `given` that says `env=` was written.
const GIVEN_ENV: u8 = 4;
/// The bit of `given` that says `timeout=` was written.
const GIVEN_TIMEOUT: u8 = 8;

/// The 64-bit words in one entry of a `dict[str, str]`: the hash, then two words each for the
/// key and the value.
const ENVIRONMENT_ENTRY_WORDS: usize = 5;

/// A `CompletedProcess` as compiled code lays it out: `class CompletedProcess(Struct)` in
/// `crates/driver/library/subprocess.byn`, its fields in declaration order with C alignment.
#[repr(C)]
#[derive(Debug)]
pub struct CompletedProcess {
    /// `args`, an owned `list[str]`: a copy of the argument list the program was run with.
    pub args: OwnedList,
    /// `returncode`: the exit status, or minus the number of the signal that ended the program.
    pub returncode: i64,
    /// `stdout`, an owned `str`: what the program wrote to standard output, `""` when it was not
    /// captured.
    pub stdout: OwnedText,
    /// `stderr`, an owned `str`: what the program wrote to standard error, `""` when it was not
    /// captured.
    pub stderr: OwnedText,
}

/// The characters of the `str` value `value`, whose top length bit may flag bytes it does not
/// own (decision 0022).
///
/// # Safety
///
/// `value` points to as many readable bytes of UTF-8 as its length word says, and nothing
/// writes them while the result lives.
#[allow(unsafe_code, reason = "reads text compiled code points to")]
unsafe fn characters<'a>(value: TextValue) -> &'a str {
    let length = usize::try_from(value.word & (u64::MAX >> 1)).unwrap_or(usize::MAX);
    // SAFETY: the caller guarantees `length` readable bytes of UTF-8 at `value.bytes`.
    unsafe { text(value.bytes, length) }
}

/// The `str`s of the `list[str]` whose header `list` points to.
///
/// # Safety
///
/// `list` points to a `list[str]` header whose elements are readable `str` values that nothing
/// writes while the result lives.
#[allow(unsafe_code, reason = "reads a list compiled code owns")]
unsafe fn strings<'a>(list: *const OwnedList) -> Vec<&'a str> {
    // SAFETY: the caller guarantees `list` points to a readable header.
    let list = unsafe { &*list };
    let (elements, length) = (list.elements, list.length);
    let length = usize::try_from(length).unwrap_or(usize::MAX);
    if length == 0 {
        return Vec::new();
    }
    // SAFETY: a `list[str]` with elements points to `length` `str` values, each a pointer and
    // a length word, which is what a `TextValue` is.
    let values = unsafe { std::slice::from_raw_parts(elements.cast::<TextValue>(), length) };
    values
        .iter()
        // SAFETY: the caller guarantees each element is a readable `str` value.
        .map(|&value| unsafe { characters(value) })
        .collect()
}

/// The names and values of the `dict[str, str]` whose table is `table`, in insertion order;
/// none for a null table, which is an empty `dict`.
///
/// # Safety
///
/// `table` is null or the live table of a `dict[str, str]` that nothing writes while the result
/// lives.
#[allow(unsafe_code, reason = "reads a dict compiled code owns")]
unsafe fn environment<'a>(table: *const Table) -> Vec<(&'a str, &'a str)> {
    // SAFETY: the caller guarantees `table` is null or a live table.
    let Some(table) = (unsafe { table.as_ref() }) else {
        return Vec::new();
    };
    table
        .entries(ENVIRONMENT_ENTRY_WORDS)
        .map(|entry| {
            let pair = entry[1..].as_ptr().cast::<[TextValue; 2]>();
            // SAFETY: an entry of a `dict[str, str]` is the hash and then two `str` values,
            // four words aligned to 8, which is what two `TextValue`s are.
            let [name, value] = unsafe { pair.read() };
            // SAFETY: the caller guarantees the entries hold readable `str` values.
            let name = unsafe { characters(name) };
            // SAFETY: as for the name.
            let value = unsafe { characters(value) };
            (name, value)
        })
        .collect()
}

/// `subprocess.run(args, capture_output, input, cwd, env, timeout)`: runs the program `args[0]`
/// names, found on `PATH`, with the rest of `args` as its arguments and no shell, waits for it,
/// and writes a new [`CompletedProcess`] to `*out`. A program that exits with a status other than
/// 0 is not a failure.
///
/// `given` says which keywords were written, one bit each: 1 `input`, 2 `cwd`, 4 `env`,
/// 8 `timeout`. The parameters of a keyword that was not written are not read.
///
/// - `input`, `input_length`: text the program reads as its standard input; without it the
///   program shares the parent's.
/// - `cwd`, `cwd_length`: the directory the program runs in.
/// - `env`: the table of a `dict[str, str]`, null for an empty one, that is the program's whole
///   environment; without it the program gets a copy of the parent's.
/// - `timeout`: seconds the program may run before it is ended and the call fails.
///
/// Returns `true`, with the error at `error`, when the program cannot be started, the timeout
/// passes, or captured output is not UTF-8.
///
/// # Panics
///
/// Ends the program with a panic, reporting `location`, when `args` is empty, where Python
/// raises `IndexError`, or when the system has no memory left for the result.
///
/// # Safety
///
/// `args` points to the header of a `list[str]`; `input` and `cwd` each point to as many
/// readable bytes of UTF-8 as their length, when `given` names them; `env` is null or the live
/// table of a `dict[str, str]`, when `given` names it; nothing writes any of them during the
/// call; `location` is null or points to a valid [`Location`] that lives as long as the
/// program; `out` points to a writable [`CompletedProcess`] and `error` to a writable
/// [`RaisedError`].
///
/// # Example
///
/// `subprocess.run(["sh", "-c", "exit 3"])` as compiled code calls it:
///
/// ```
/// use runtime::{
///     CompletedProcess, OwnedList, RaisedError, TextValue, baryon_rt_deallocate,
///     baryon_rt_subprocess_run,
/// };
/// use std::mem::MaybeUninit;
///
/// let words = ["sh", "-c", "exit 3"];
/// let mut values = words.map(|word| TextValue { bytes: word.as_ptr(), word: word.len() as u64 });
/// let args = OwnedList { elements: values.as_mut_ptr().cast(), length: 3, capacity: 3 };
/// let mut done = MaybeUninit::<CompletedProcess>::uninit();
/// let mut error = MaybeUninit::<RaisedError>::uninit();
/// // SAFETY: `args` is a list of three readable strings, no keyword is given, and `done` and
/// // `error` are locals.
/// let failed = unsafe {
///     baryon_rt_subprocess_run(
///         &raw const args, false, 0, std::ptr::null(), 0, std::ptr::null(), 0,
///         std::ptr::null(), 0.0, std::ptr::null(), done.as_mut_ptr(), error.as_mut_ptr(),
///     )
/// };
/// assert!(!failed);
/// // SAFETY: success wrote the result; its `args` copy is freed as compiled code frees a list.
/// unsafe {
///     let done = done.assume_init();
///     assert_eq!(done.returncode, 3);
///     assert_eq!((done.stdout.length, done.stderr.length), (0, 0));
///     for index in 0..3 {
///         let word = &*done.args.elements.add(index);
///         baryon_rt_deallocate(word.bytes, word.length as usize, 1);
///     }
///     baryon_rt_deallocate(done.args.elements.cast(), 3 * 16, 8);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per keyword of `subprocess.run`, as compiled code passes them"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_subprocess_run(
    args: *const OwnedList,
    capture: bool,
    given: u8,
    input: *const u8,
    input_length: usize,
    cwd: *const u8,
    cwd_length: usize,
    env: *const Table,
    timeout: f64,
    location: *const Location,
    out: *mut CompletedProcess,
    error: *mut RaisedError,
) -> bool {
    // SAFETY: the caller guarantees `args` is a readable `list[str]`.
    let args = unsafe { strings(args) };
    if args.is_empty() {
        let message = "`subprocess.run` needs a program to run: its argument list is empty";
        // SAFETY: the caller's promise about `location`, passed on.
        panic(Failure::Message(message), unsafe {
            super::location(location)
        });
    }
    let request = Request {
        capture,
        // SAFETY: the caller guarantees readable UTF-8 at `input` when `given` names it.
        input: (given & GIVEN_INPUT != 0).then(|| unsafe { text(input, input_length) }),
        // SAFETY: the caller guarantees readable UTF-8 at `cwd` when `given` names it.
        cwd: (given & GIVEN_CWD != 0).then(|| unsafe { text(cwd, cwd_length) }),
        // SAFETY: the caller guarantees `env` is null or a live table when `given` names it.
        env: (given & GIVEN_ENV != 0).then(|| unsafe { environment(env) }),
        timeout: (given & GIVEN_TIMEOUT != 0).then_some(timeout),
        args,
    };
    let write = |done: Completed| {
        let mut copy = OwnedList {
            elements: std::ptr::null_mut(),
            length: 0,
            capacity: 0,
        };
        // SAFETY: `copy` is a local, writable list header.
        unsafe { write_list(&request.args, &raw mut copy) };
        let value = CompletedProcess {
            args: copy,
            returncode: done.returncode,
            stdout: owned(&done.stdout),
            stderr: owned(&done.stderr),
        };
        // SAFETY: the caller guarantees `out` is writable.
        unsafe { out.write(value) };
    };
    // SAFETY: the caller guarantees `error` is writable.
    unsafe { finish(subprocess::run(&request), write, error) }
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported function as compiled code does"
)]
mod tests;
