//! The functions compiled code calls for what a program learns from, and gives back to, the
//! process that runs it (decision 0036): its command-line arguments, its environment and its exit
//! status.
//!
//! Arguments and environment come from Rust's standard library, which reads them on Linux and
//! macOS without help from the C `main`. Both may hold bytes that are not UTF-8; decision 0036
//! reads them with each bad sequence replaced by U+FFFD rather than failing, so `sys.argv` and
//! `os.environ` cannot raise. Each call returns a new owned `str`, which compiled code frees as
//! it frees any other.

use super::text::{OwnedText, owned};
use crate::location::Location;
use crate::panic::{Failure, panic};
use crate::print;

/// How many command-line arguments the program was started with, the program itself included:
/// `len(sys.argv)`.
///
/// # Example
///
/// ```
/// assert!(runtime::baryon_rt_argument_count() >= 1);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_argument_count() -> i64 {
    i64::try_from(std::env::args_os().len()).unwrap_or(i64::MAX)
}

/// Command-line argument `index` as a new owned `str`: `sys.argv[index]`. Argument 0 is the
/// program as it was started. Compiled code asks only for an index below
/// [`baryon_rt_argument_count`], and an index past the end gives the empty string.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_argument, baryon_rt_deallocate};
///
/// let program = baryon_rt_argument(0);
/// assert!(program.length > 0);
/// // SAFETY: freed once, with its length, as an owned `str` is.
/// unsafe { baryon_rt_deallocate(program.bytes, program.length as usize, 1) };
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_argument(index: i64) -> OwnedText {
    let argument = usize::try_from(index)
        .ok()
        .and_then(|index| std::env::args_os().nth(index))
        .unwrap_or_default();
    owned(&argument.to_string_lossy())
}

/// The value of the environment variable whose name is the `length` bytes at `name`, as a new
/// owned `str`, with `*found` set to whether it is set: `os.environ.get(name)`. A variable that
/// is not set gives the empty string and `false`. A name that is empty or holds `=` or a zero
/// byte is never set.
///
/// The environment is only read: decision 0036 makes it read-only, since changing it while
/// another thread reads it is a data race in the C library.
///
/// # Safety
///
/// `name` points to `length` readable bytes of UTF-8, and `found` points to a writable `bool`.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_deallocate, baryon_rt_environment_get};
///
/// let name = "BARYON_NOT_SET_ANYWHERE";
/// let mut found = true;
/// // SAFETY: the pointer and length are the constant's, and `found` is a local.
/// let value = unsafe { baryon_rt_environment_get(name.as_ptr(), name.len(), &raw mut found) };
/// assert!(!found);
/// assert_eq!(value.length, 0);
/// // SAFETY: freed once, with its length, as an owned `str` is.
/// unsafe { baryon_rt_deallocate(value.bytes, value.length as usize, 1) };
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_environment_get(
    name: *const u8,
    length: usize,
    found: *mut bool,
) -> OwnedText {
    // SAFETY: the caller guarantees `length` readable bytes at `name`.
    let name = unsafe { super::strings::bytes(name, length) };
    let value = std::str::from_utf8(name)
        .ok()
        .filter(|name| !name.is_empty() && !name.contains(['=', '\0']))
        .and_then(std::env::var_os);
    // SAFETY: the caller guarantees `found` is writable.
    unsafe { found.write(value.is_some()) };
    owned(&value.unwrap_or_default().to_string_lossy())
}

/// Ends the program with exit status `status`: `sys.exit(status)`. It first writes out what
/// `print` holds back, as the end of `main` does, and runs nothing else on the way out.
///
/// # Panics
///
/// Ends the program with a panic, reporting `location`, when `status` is outside 0 to 255, the
/// statuses Linux and macOS can report to the program's parent.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_exit(status: i64, location: *const Location) -> ! {
    let Ok(status @ 0..=255) = i32::try_from(status) else {
        let message = format!("`sys.exit` status {status} is outside 0 to 255");
        // SAFETY: the caller's promise about `location`, passed on.
        panic(Failure::Message(&message), unsafe {
            super::location(location)
        });
    };
    print::flush();
    std::process::exit(status)
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests {
    use super::*;
    use crate::baryon_rt_deallocate;
    use std::process::Command;

    /// Set in the environment of a child process the tests start, to the case it runs.
    const CHILD: &str = "BARYON_RUNTIME_TEST_EXIT_CHILD";

    /// The location a child's panic reports.
    static HERE: Location = Location::new("main.byn", 4, 5);

    /// The text of an owned `str`, which it frees.
    fn take(text: &OwnedText) -> String {
        let length = usize::try_from(text.length).unwrap();
        // SAFETY: the runtime made `text` with `length` initialized bytes.
        let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) };
        let copy = String::from_utf8(bytes.to_vec()).unwrap();
        // SAFETY: freed once, with its length.
        unsafe { baryon_rt_deallocate(text.bytes, length, 1) };
        copy
    }

    /// Runs this test binary again as a child that runs `case`, and gives its exit status and
    /// standard error.
    fn child(test: &str, case: &str) -> (Option<i32>, String) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--test-threads=1"])
            .env(CHILD, case)
            .output()
            .unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    #[test]
    fn reads_every_argument_and_nothing_past_the_end() {
        let count = baryon_rt_argument_count();
        let expected: Vec<String> = std::env::args().collect();
        assert_eq!(count, i64::try_from(expected.len()).unwrap());
        for (index, argument) in expected.iter().enumerate() {
            assert_eq!(
                &take(&baryon_rt_argument(i64::try_from(index).unwrap())),
                argument
            );
        }
        assert_eq!(take(&baryon_rt_argument(count)), "");
        assert_eq!(take(&baryon_rt_argument(-1)), "");
    }

    #[test]
    fn reads_a_set_variable_and_rejects_names_that_cannot_be_set() {
        let mut found = false;
        let name = "PATH";
        // SAFETY: the pointer and length are the constant's, and `found` is a local.
        let value = unsafe { baryon_rt_environment_get(name.as_ptr(), name.len(), &raw mut found) };
        assert!(found);
        assert_eq!(take(&value), std::env::var("PATH").unwrap());
        for name in ["", "A=B", "A\0B"] {
            let mut found = true;
            // SAFETY: as above.
            let value =
                unsafe { baryon_rt_environment_get(name.as_ptr(), name.len(), &raw mut found) };
            assert!(!found, "{name:?}");
            assert_eq!(take(&value), "");
        }
    }

    #[test]
    fn exits_with_the_status_given() {
        const TEST: &str = "abi::process::tests::exits_with_the_status_given";
        match std::env::var(CHILD).as_deref() {
            // SAFETY: `HERE` is a static location.
            Ok("3") => unsafe { baryon_rt_exit(3, &raw const HERE) },
            // SAFETY: as above.
            Ok("256") => unsafe { baryon_rt_exit(256, &raw const HERE) },
            _ => {}
        }
        assert_eq!(child(TEST, "3"), (Some(3), String::new()));
        assert_eq!(
            child(TEST, "256"),
            (
                Some(101),
                "panic: `sys.exit` status 256 is outside 0 to 255\n  --> main.byn:4:5\n".to_owned()
            )
        );
    }
}
