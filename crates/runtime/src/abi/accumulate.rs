//! The functions compiled code calls for an array a `prange` loop lists in `accumulate=`
//! (decisions 0013 and 0024).
//!
//! For each accumulated array, the function running the loop calls
//! [`baryon_rt_accumulator_new`] before [`baryon_rt_parallel_for`](crate::baryon_rt_parallel_for)
//! and [`baryon_rt_accumulator_finish`] after it. Inside the loop, each add calls
//! [`baryon_rt_accumulator_slot`] for the address of the element's scalar in the running chunk's
//! private copy and adds there; compiled code skips the call when it finds the block already
//! made in the chunk's table, from [`baryon_rt_accumulator_table`]. Each chunk calls
//! [`baryon_rt_accumulator_chunk_done`] when its last iteration ends. The result is
//! bit-identical on any number of threads.
//!
//! The example on [`baryon_rt_accumulator_new`] runs the four functions every loop calls.

use super::location;
use crate::Location;
use crate::accumulate::{Accumulator, Kind};
use crate::panic::{Failure, panic};

/// The message of an integer overflow while combining, the same as a scalar reduction's.
const OVERFLOW: &str = "integer overflow in `+`";

/// Starts the private copies of one accumulated array for a `prange` loop of `iterations`
/// iterations. The array holds `scalars` scalars of the kind `kind` codes:
///
/// | Code | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |
/// | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
/// | Kind | `i8` | `i16` | `i32` | `i64` | `u8` | `u16` | `u32` | `u64` | `f32` | `f64` |
///
/// A `Fixed[f64, 3]` element is three `f64` scalars. `location` is the loop, reported if an
/// integer sum overflows. No copy is made yet: under 40 KiB for the tree's bookkeeping.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// `forces[j] += f` over a loop of 1000 iterations, for an `f64` array of 4 elements, run here
/// on one thread, chunk by chunk:
///
/// ```
/// use runtime::{
///     baryon_rt_accumulator_chunk_done, baryon_rt_accumulator_finish, baryon_rt_accumulator_new,
///     baryon_rt_accumulator_slot, baryon_rt_chunk_count,
/// };
///
/// let iterations = 1000;
/// let mut forces = [0.5_f64; 4];
/// // SAFETY: each chunk adds and finishes before the next starts, `j` is below 4, and the
/// // array is live, contiguous and holds 4 `f64` scalars.
/// unsafe {
///     let accumulator = baryon_rt_accumulator_new(iterations, 4, 9, std::ptr::null());
///     for chunk in 0..baryon_rt_chunk_count(iterations) {
///         let j = chunk % 4;
///         let slot = baryon_rt_accumulator_slot(accumulator, chunk, j).cast::<f64>();
///         slot.write(slot.read() + 1.0);
///         baryon_rt_accumulator_chunk_done(accumulator, chunk);
///     }
///     baryon_rt_accumulator_finish(accumulator, forces.as_mut_ptr().cast(), 1, &4, &1, 1);
/// }
/// assert_eq!(forces, [250.5; 4]);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_accumulator_new(
    iterations: i64,
    scalars: i64,
    kind: i32,
    location: *const Location,
) -> *mut Accumulator {
    // SAFETY: the caller's promise about `location`, passed on.
    let location = unsafe { self::location(location) };
    let Some(kind) = Kind::from_code(kind) else {
        panic(
            Failure::Message("compiler bug: unknown accumulator kind"),
            location,
        )
    };
    let scalars = usize::try_from(scalars).unwrap_or(0);
    Box::into_raw(Box::new(Accumulator::new(
        iterations, scalars, kind, location,
    )))
}

/// The address of scalar `scalar` in chunk `chunk`'s private copy, made on the chunk's first
/// add there and started at the identity: `-0.0` for floats, `0` for integers. Compiled code
/// adds into it in place.
///
/// Cost: one table lookup; the first add into a block of 4 KiB allocates it.
///
/// # Safety
///
/// `accumulator` came from [`baryon_rt_accumulator_new`] and is not finished; the caller is
/// running chunk `chunk` and has not yet called [`baryon_rt_accumulator_chunk_done`] for it;
/// `scalar` is below the scalar count.
///
/// # Example
///
/// The example on [`baryon_rt_accumulator_new`] calls it.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_accumulator_slot(
    accumulator: *const Accumulator,
    chunk: i64,
    scalar: i64,
) -> *mut u8 {
    // SAFETY: the caller guarantees the accumulator is live.
    let accumulator = unsafe { &*accumulator };
    let (chunk, scalar) = (to_index(chunk), to_index(scalar));
    // SAFETY: the caller's promise about the chunk and the scalar, passed on.
    unsafe { accumulator.slot(chunk, scalar) }
}

/// Chunk `chunk`'s block table, made if the chunk has none yet: one pointer per block of
/// 4096 bytes of the array's scalars (the runtime's block size), null until the chunk first
/// adds into that block. Compiled code reads it to find a block it has touched
/// without a call: scalar `s` of size `z` bytes is at byte `s × z % 4096` of the block at index
/// `s × z / 4096`. When the pointer it finds is null, it calls [`baryon_rt_accumulator_slot`].
///
/// The table stays at this address until the chunk calls [`baryon_rt_accumulator_chunk_done`];
/// a block, once made, does too.
///
/// # Safety
///
/// As for [`baryon_rt_accumulator_slot`]: `accumulator` came from [`baryon_rt_accumulator_new`]
/// and is not finished, and the caller is running chunk `chunk` and has not yet called
/// [`baryon_rt_accumulator_chunk_done`] for it.
///
/// # Example
///
/// The second add into a block finds it in the table:
///
/// ```
/// use runtime::{
///     baryon_rt_accumulator_chunk_done, baryon_rt_accumulator_finish, baryon_rt_accumulator_new,
///     baryon_rt_accumulator_slot, baryon_rt_accumulator_table,
/// };
///
/// let mut values = [1.0_f64; 600];
/// // SAFETY: one chunk, run on this thread; scalar 513 is below 600, and the array is live,
/// // contiguous and holds 600 `f64` scalars.
/// unsafe {
///     let accumulator = baryon_rt_accumulator_new(1, 600, 9, std::ptr::null());
///     let table = baryon_rt_accumulator_table(accumulator, 0).cast::<*mut f64>();
///     assert!(table.add(1).read().is_null());
///     let slot = baryon_rt_accumulator_slot(accumulator, 0, 513).cast::<f64>();
///     slot.write(slot.read() + 2.0);
///     // Scalar 513 is byte 4104: block 1, byte 8, which is `f64` number 1 in it.
///     let found = table.add(1).read().add(1);
///     assert_eq!(found, slot);
///     found.write(found.read() + 3.0);
///     baryon_rt_accumulator_chunk_done(accumulator, 0);
///     baryon_rt_accumulator_finish(accumulator, values.as_mut_ptr().cast(), 1, &600, &1, 1);
/// }
/// assert_eq!(values[513], 6.0);
/// assert_eq!(values[512], 1.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_accumulator_table(
    accumulator: *const Accumulator,
    chunk: i64,
) -> *mut u8 {
    // SAFETY: the caller guarantees the accumulator is live.
    let accumulator = unsafe { &*accumulator };
    // SAFETY: the caller's promise about the chunk, passed on.
    unsafe { accumulator.table(to_index(chunk)) }
}

/// Records that chunk `chunk` has run its last iteration, and combines every part of the tree
/// this completes. Panics with the loop's location when an integer sum overflows.
///
/// # Safety
///
/// `accumulator` came from [`baryon_rt_accumulator_new`] and is not finished; the caller ran
/// chunk `chunk`, its adds are finished, and it calls this once for the chunk.
///
/// # Example
///
/// The example on [`baryon_rt_accumulator_new`] calls it.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_accumulator_chunk_done(
    accumulator: *const Accumulator,
    chunk: i64,
) {
    // SAFETY: the caller guarantees the accumulator is live.
    let accumulator = unsafe { &*accumulator };
    // SAFETY: the caller's promise about the chunk, passed on.
    if unsafe { accumulator.chunk_done(to_index(chunk)) }.is_err() {
        panic(Failure::Message(OVERFLOW), accumulator.location);
    }
}

/// Adds the combined copies into the array, the array's own value first in each sum, and frees
/// the accumulator. Panics with the loop's location when an integer sum overflows.
///
/// The array is passed as compiled code holds it (decision 0008): `data`, then `rank` lengths
/// and `rank` strides in elements, outermost first, and `lanes` scalars per element.
///
/// Cost: one pass over the blocks any chunk touched.
///
/// # Safety
///
/// `accumulator` came from [`baryon_rt_accumulator_new`], every chunk has called
/// [`baryon_rt_accumulator_chunk_done`], and it is not used again; `lengths` and `strides` each
/// point to `rank` values describing a live array at `data` of the accumulator's kind, whose
/// lengths multiply out, times `lanes`, to the scalar count; nothing else touches it meanwhile.
///
/// # Example
///
/// The example on [`baryon_rt_accumulator_new`] calls it.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_accumulator_finish(
    accumulator: *mut Accumulator,
    data: *mut u8,
    rank: i64,
    lengths: *const i64,
    strides: *const i64,
    lanes: i64,
) {
    // SAFETY: the caller hands over the accumulator, made by `Box::into_raw`, for the last time.
    let accumulator = *unsafe { Box::from_raw(accumulator) };
    let location = accumulator.location;
    let rank = to_index(rank);
    // SAFETY: the caller guarantees `rank` lengths.
    let lengths = unsafe { std::slice::from_raw_parts(lengths, rank) };
    // SAFETY: the caller guarantees `rank` strides.
    let strides = unsafe { std::slice::from_raw_parts(strides, rank) };
    // SAFETY: the caller's promise about the array, passed on.
    let finished = unsafe { accumulator.finish(data, lengths, strides, to_index(lanes).max(1)) };
    if finished.is_err() {
        panic(Failure::Message(OVERFLOW), location);
    }
}

/// `value` as an index: compiled code passes only values from 0 up, so a negative one, a
/// compiler bug, becomes 0.
fn to_index(value: i64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests {
    use super::*;
    use crate::{baryon_rt_chunk_count, baryon_rt_parallel_for};
    use std::ffi::c_void;

    /// The array length the tests accumulate into: several blocks of `f64`.
    const LENGTH: i64 = 5000;

    /// The loop body: iteration `i` adds a value that depends on `i` into two elements, its own
    /// and one far away, as a pair force does.
    unsafe extern "C" fn body(context: *mut c_void, chunk: i64, start: i64, end: i64) {
        let accumulator = context.cast::<Accumulator>().cast_const();
        for i in start..end {
            let value = 1.0 / f64::from(u32::try_from(i + 1).unwrap_or(1));
            for (index, sign) in [(i % LENGTH, 1.0), ((i * 7919) % LENGTH, -0.5)] {
                // SAFETY: this thread runs `chunk`, and the index is below `LENGTH`.
                let slot = unsafe { baryon_rt_accumulator_slot(accumulator, chunk, index) };
                #[allow(clippy::cast_ptr_alignment, reason = "an `f64` slot is aligned to 8")]
                let slot = slot.cast::<f64>();
                // SAFETY: the slot is this chunk's, holds an `f64`, and only this thread uses it.
                let current = unsafe { slot.read() };
                // SAFETY: as above.
                unsafe { slot.write(current + sign * value) };
            }
        }
        // SAFETY: the chunk's adds are finished.
        unsafe { baryon_rt_accumulator_chunk_done(accumulator, chunk) };
    }

    /// Runs the loop of `iterations` iterations, on the pool when `parallel`, chunk by chunk on
    /// this thread otherwise, and returns the array.
    fn run(iterations: i64, parallel: bool) -> Vec<f64> {
        let mut array = vec![0.25; usize::try_from(LENGTH).unwrap_or(0)];
        // SAFETY: a null location is allowed.
        let accumulator =
            unsafe { baryon_rt_accumulator_new(iterations, LENGTH, 9, std::ptr::null()) };
        if parallel {
            // SAFETY: each chunk runs `body` once and touches only its own copy.
            unsafe { baryon_rt_parallel_for(iterations, body, accumulator.cast()) };
        } else {
            let count = baryon_rt_chunk_count(iterations);
            for chunk in 0..count {
                let (quotient, remainder) = (iterations / count, iterations % count);
                let start = chunk * quotient + chunk.min(remainder);
                let end = start + quotient + i64::from(chunk < remainder);
                // SAFETY: each chunk runs once, in turn, on this thread.
                unsafe { body(accumulator.cast(), chunk, start, end) };
            }
        }
        // SAFETY: every chunk is done, and the array is live and contiguous with `LENGTH`
        // `f64` scalars.
        unsafe {
            baryon_rt_accumulator_finish(accumulator, array.as_mut_ptr().cast(), 1, &LENGTH, &1, 1);
        }
        array
    }

    /// On the thread pool, with chunks finishing in whatever order the threads reach them, the
    /// result has the same bits as running the chunks one by one, run after run.
    #[test]
    fn the_pool_gives_the_same_bits_as_one_thread() {
        let iterations = 200_000;
        let serial: Vec<u64> = run(iterations, false)
            .iter()
            .map(|value| value.to_bits())
            .collect();
        for _ in 0..4 {
            let parallel: Vec<u64> = run(iterations, true)
                .iter()
                .map(|value| value.to_bits())
                .collect();
            assert!(
                parallel == serial,
                "a parallel run differs from the serial chunks"
            );
        }
    }
}
