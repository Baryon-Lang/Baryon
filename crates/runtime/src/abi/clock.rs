//! The functions compiled code calls to read a clock, for Python's `time.perf_counter`,
//! `time.monotonic` and `time.time` (roadmap 1.13.19).
//!
//! Both read Rust's standard library. A monotonic reading is an [`Instant`], which is
//! `CLOCK_MONOTONIC` on Linux, the clock Python reads for `perf_counter` and `monotonic` there,
//! and `CLOCK_UPTIME_RAW` on macOS. An `Instant` has no number of its own, only differences, so
//! the first monotonic reading of the program fixes the zero and every reading is the seconds
//! since it. Python leaves the zero of these clocks undefined too: only a difference means
//! anything. Counting from the program's first reading keeps an `f64` exact to well under a
//! nanosecond for the first few days of a run, where seconds since boot would lose digits on a
//! machine that has been up for months.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The program's first monotonic reading, the zero of every later one.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Seconds on a clock that never goes back and that setting the system's date does not move:
/// `time.perf_counter()` and `time.monotonic()`. The first call of the program gives 0.0 or a
/// little more; only the difference of two readings means anything. Neither Linux's nor
/// macOS's clock counts time the machine sleeps.
///
/// # Example
///
/// ```
/// let start = runtime::baryon_rt_monotonic_clock();
/// let end = runtime::baryon_rt_monotonic_clock();
/// assert!(end >= start && start >= 0.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_monotonic_clock() -> f64 {
    let now = Instant::now();
    now.duration_since(*ORIGIN.get_or_init(|| now))
        .as_secs_f64()
}

/// Seconds since 1970-01-01 00:00:00 UTC on the system's clock, with a fraction: `time.time()`.
/// That clock moves when the system's date is set, so it can go back; time a computation with
/// [`baryon_rt_monotonic_clock`] instead. A clock set before 1970 gives a negative number, as
/// in Python.
///
/// # Example
///
/// ```
/// // 2026-01-01 00:00:00 UTC.
/// assert!(runtime::baryon_rt_wall_clock() > 1_767_225_600.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_wall_clock() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    }
}
