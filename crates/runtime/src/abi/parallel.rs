//! The functions compiled code calls to run a `prange` loop: how many chunks it splits into, and
//! running the chunks on the thread pool.

use crate::parallel::{self, ChunkBody, Task};
use std::ffi::c_void;

/// How many chunks a `prange` loop of `iterations` iterations is split into: one per iteration,
/// up to 1024, and 0 for an empty loop.
///
/// The count depends only on `iterations`, never on the machine. Compiled code calls it before a
/// loop with a reduction, to make room for one partial result per chunk, which it combines in
/// a fixed order after the loop. That is what makes the reduction reproducible.
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_chunk_count;
///
/// assert_eq!(baryon_rt_chunk_count(10), 10);
/// assert_eq!(baryon_rt_chunk_count(1_000_000), 1024);
/// assert_eq!(baryon_rt_chunk_count(0), 0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_chunk_count(iterations: i64) -> i64 {
    parallel::chunk_count(iterations)
}

/// Runs a `prange` loop of `iterations` iterations: calls `body` once per chunk, on the thread
/// pool, and returns when every chunk has finished.
///
/// Each call gets `context`, the chunk's index and its iterations as `start..end`, counted from
/// 0; compiled code maps them onto the loop's own range. The chunks are the ones
/// [`baryon_rt_chunk_count`] counts. Chunks run in any order and at the same time, and every write a
/// chunk makes is visible to the caller when this returns. A loop with one chunk runs on the
/// calling thread and starts no threads.
///
/// # Safety
///
/// Calling `body` with `context` is sound for every chunk, from any thread, with different
/// chunks running at the same time. The compiler's `prange` rules (the reference, "Parallel and
/// vector loops") are what make that true of an outlined loop body.
///
/// # Example
///
/// A sum over 1000 iterations, one partial result per chunk. Compiled code combines the
/// partials in the fixed pairwise tree the reference defines; an integer sum is exact in any
/// order, so the example simply adds them:
///
/// ```
/// use runtime::{baryon_rt_chunk_count, baryon_rt_parallel_for};
/// use std::ffi::c_void;
/// use std::sync::atomic::{AtomicI64, Ordering};
///
/// unsafe extern "C" fn body(context: *mut c_void, chunk: i64, start: i64, end: i64) {
///     // SAFETY: `context` points to the partials below, one per chunk.
///     let partials = unsafe { &*context.cast::<Vec<AtomicI64>>() };
///     let sum: i64 = (start..end).sum();
///     partials[chunk as usize].store(sum, Ordering::Relaxed);
/// }
///
/// let iterations = 1000;
/// let partials: Vec<AtomicI64> =
///     (0..baryon_rt_chunk_count(iterations)).map(|_| AtomicI64::new(0)).collect();
/// let context = std::ptr::from_ref(&partials).cast_mut().cast();
/// // SAFETY: each chunk writes only its own atomic partial, so chunks may run at once.
/// unsafe { baryon_rt_parallel_for(iterations, body, context) };
/// let total: i64 = partials.iter().map(|partial| partial.load(Ordering::Relaxed)).sum();
/// assert_eq!(total, 999 * 1000 / 2);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_parallel_for(
    iterations: i64,
    body: ChunkBody,
    context: *mut c_void,
) {
    let task = Task {
        body,
        context,
        iterations,
    };
    // SAFETY: the caller's promise about `body` and `context`, passed on.
    unsafe { parallel::parallel_for(task) }
}
