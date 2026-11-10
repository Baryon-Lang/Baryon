//! The functions compiled code calls, exported with C names and the C calling convention.
//!
//! Every name starts with `baryon_rt_`, so the runtime's symbols never collide with a program's. The
//! backend declares each function with the signature written here and calls it by name; nothing
//! else about the runtime is visible to compiled code. Each function is a thin wrapper: it turns
//! raw pointers into references, under the promise in its `# Safety` section, and calls the safe
//! code in the other modules.

mod accumulate;
mod output;
mod philox;
mod sum;
mod text;

pub use accumulate::{
    baryon_rt_accumulator_chunk_done, baryon_rt_accumulator_finish, baryon_rt_accumulator_new,
    baryon_rt_accumulator_slot,
};
pub use output::{
    baryon_rt_print_bool, baryon_rt_print_f32, baryon_rt_print_f64, baryon_rt_print_i64,
    baryon_rt_print_str, baryon_rt_print_str_quoted, baryon_rt_print_u64,
};
pub use philox::{baryon_rt_philox_new, baryon_rt_philox_uniform};
pub use sum::{baryon_rt_sum_f32, baryon_rt_sum_f64};
pub use text::{
    OwnedText, TextBuilder, baryon_rt_text_bool, baryon_rt_text_f32, baryon_rt_text_f64,
    baryon_rt_text_finish, baryon_rt_text_i64, baryon_rt_text_new, baryon_rt_text_str,
    baryon_rt_text_u64,
};

use crate::Location;
use crate::memory;
use crate::panic::{Failure, panic};
use crate::parallel::{self, ChunkBody, Task};
use std::ffi::c_void;
use std::ptr::NonNull;

/// The location `location` points to, or `None` for a null pointer.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
#[allow(unsafe_code, reason = "reads a location compiled code points to")]
unsafe fn location<'a>(location: *const Location) -> Option<&'a Location> {
    // SAFETY: the caller guarantees the pointer is null or valid for the program's life.
    unsafe { location.as_ref() }
}

/// Allocates `bytes` bytes aligned to `align`, uninitialized, for a value that lives on the heap.
///
/// Returns a pointer that is never null. Zero bytes allocate nothing and return a pointer whose
/// address is `align`, which [`baryon_rt_deallocate`] accepts. Free the memory with
/// [`baryon_rt_deallocate`] and the same `bytes` and `align`.
///
/// # Panics
///
/// Ends the program with a panic when the system has no memory left, reporting `location`, or
/// when `align` is not a power of two, which only a compiler bug asks for.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_allocate, baryon_rt_deallocate};
///
/// // SAFETY: a null location is allowed; the memory is freed with the size it was allocated with.
/// unsafe {
///     let pointer = baryon_rt_allocate(24, 8, std::ptr::null());
///     pointer.cast::<[f64; 3]>().write([1.0, 2.0, 3.0]);
///     baryon_rt_deallocate(pointer, 24, 8);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_allocate(
    bytes: usize,
    align: usize,
    location: *const Location,
) -> *mut u8 {
    match memory::layout(bytes, align).and_then(memory::allocate) {
        Ok(pointer) => pointer.as_ptr(),
        // SAFETY: the caller's promise about `location`, passed on.
        Err(failure) => panic(failure, unsafe { self::location(location) }),
    }
}

/// Allocates an array's elements: `count` elements of `element_size` bytes each, aligned to
/// `align`, uninitialized. This is what an IR `NewArray` becomes.
///
/// Returns a pointer that is never null; an empty array allocates nothing, as in
/// [`baryon_rt_allocate`]. Free the memory with [`baryon_rt_deallocate`], passing `count` times
/// `element_size` as the size, which cannot overflow once this function has returned.
///
/// # Panics
///
/// Ends the program with a panic, reporting `location`, when `count` is negative, when the size
/// in bytes does not fit in memory, or when the system has no memory left.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_allocate_array, baryon_rt_deallocate};
///
/// let count = 1000;
/// // SAFETY: a null location is allowed; every element is written before it is read; the
/// // memory is freed with the size it was allocated with.
/// unsafe {
///     let elements = baryon_rt_allocate_array(count, 8, 8, std::ptr::null()).cast::<f64>();
///     for index in 0..1000 {
///         elements.add(index).write(index as f64);
///     }
///     assert_eq!(elements.add(999).read(), 999.0);
///     baryon_rt_deallocate(elements.cast(), 1000 * 8, 8);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_allocate_array(
    count: i64,
    element_size: usize,
    align: usize,
    location: *const Location,
) -> *mut u8 {
    match memory::array_layout(count, element_size, align).and_then(memory::allocate) {
        Ok(pointer) => pointer.as_ptr(),
        // SAFETY: the caller's promise about `location`, passed on.
        Err(failure) => panic(failure, unsafe { self::location(location) }),
    }
}

/// Frees memory from [`baryon_rt_allocate`] or [`baryon_rt_allocate_array`]. A null pointer does
/// nothing, so a value that never got its memory can be dropped like any other.
///
/// # Panics
///
/// Ends the program with a panic when `bytes` and `align` are not a size and alignment
/// [`baryon_rt_allocate`] could have accepted, which only a compiler bug passes.
///
/// # Safety
///
/// `pointer` is null, or came from [`baryon_rt_allocate`] or [`baryon_rt_allocate_array`] with this
/// size in bytes and alignment and has not been freed since.
///
/// # Example
///
/// ```
/// use runtime::{baryon_rt_allocate, baryon_rt_deallocate};
///
/// // SAFETY: null is always allowed; the empty allocation is freed with its own size.
/// unsafe {
///     baryon_rt_deallocate(std::ptr::null_mut(), 64, 8);
///     let empty = baryon_rt_allocate(0, 8, std::ptr::null());
///     baryon_rt_deallocate(empty, 0, 8);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_deallocate(pointer: *mut u8, bytes: usize, align: usize) {
    let Some(pointer) = NonNull::new(pointer) else {
        return;
    };
    match memory::layout(bytes, align) {
        // SAFETY: the caller guarantees `pointer` came from an allocation with this layout,
        // and both allocation functions get their memory from `memory::allocate`.
        Ok(layout) => unsafe { memory::deallocate(pointer, layout) },
        Err(failure) => panic(failure, None),
    }
}

/// Ends the program with a panic that prints `message` and `location`. Compiled code calls it
/// for every check whose message it can write in full: an overflow, a division by zero, a failed
/// `assert` with its condition's source.
///
/// The program prints `panic: <message>`, then `  --> <file>:<line>:<column>`, to standard
/// error, and exits with status 101. When several threads of a parallel loop panic at once, one
/// message is printed. A message that is not UTF-8, which only a compiler bug writes, is printed
/// with replacement characters.
///
/// # Safety
///
/// `message` points to `length` readable bytes, and `location` is null or points to a valid
/// [`Location`] that lives as long as the program.
///
/// # Example
///
/// What compiled code does when `assert n > 0` fails at `heat.byn:12:5`:
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic};
///
/// static HERE: Location = Location::new("heat.byn", 12, 5);
/// let message = "assertion failed: n > 0";
/// // SAFETY: the message and the location are constants that live as long as the program.
/// unsafe { baryon_rt_panic(message.as_ptr(), message.len(), &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic(
    message: *const u8,
    length: usize,
    location: *const Location,
) -> ! {
    // SAFETY: the caller guarantees `message` points to `length` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(message, length) };
    let message = String::from_utf8_lossy(bytes);
    // SAFETY: the caller's promise about `location`, passed on.
    panic(Failure::Message(&message), unsafe {
        self::location(location)
    })
}

/// Ends the program with a panic for an array index out of range: `index` is negative, or not
/// below the `length` of `dimension`, 0 for the outermost (decision 0010). Compiled code calls it on the failing side of
/// an IR `IndexInBounds` check.
///
/// Prints `panic: index <index> is out of range for dimension <dimension> of length <length>` and the location, as
/// [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_index};
///
/// static HERE: Location = Location::new("md.byn", 58, 13);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_index(10, 0, 8, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_index(
    index: i64,
    dimension: u32,
    length: i64,
    location: *const Location,
) -> ! {
    let failure = Failure::IndexOutOfRange {
        index,
        dimension,
        length,
    };
    // SAFETY: the caller's promise about `location`, passed on.
    panic(failure, unsafe { self::location(location) })
}

/// Ends the program with a panic for arrays combined element by element whose lengths differ
/// in `dimension`: the first has `expected` elements there, the other `found`. Compiled code
/// calls it on the failing side of an IR `SameLength` check.
///
/// Prints `panic: array lengths differ in dimension <dimension>: <expected> and <found>` and the
/// location, as [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_length_mismatch};
///
/// static HERE: Location = Location::new("heat.byn", 30, 11);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_length_mismatch(0, 64, 63, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_length_mismatch(
    dimension: u32,
    expected: i64,
    found: i64,
    location: *const Location,
) -> ! {
    let failure = Failure::LengthMismatch {
        dimension,
        expected,
        found,
    };
    // SAFETY: the caller's promise about `location`, passed on.
    panic(failure, unsafe { self::location(location) })
}

/// Ends the program with a panic for integer `**` with a negative `exponent`, whose result would
/// be a fraction (language reference, "Errors and panics"). Compiled code calls it on the failing
/// side of an IR `NonNegativeExponent` check.
///
/// Prints ``panic: negative exponent <exponent> in integer `**` `` and the location, as
/// [`baryon_rt_panic`] does.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`] that lives as long as the program.
///
/// # Example
///
/// What compiled code does for `2 ** n` with `n` equal to -1:
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_exponent};
///
/// static HERE: Location = Location::new("main.byn", 4, 11);
/// // SAFETY: the location is a constant that lives as long as the program.
/// unsafe { baryon_rt_panic_exponent(-1, &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_exponent(exponent: i64, location: *const Location) -> ! {
    // SAFETY: the caller's promise about `location`, passed on.
    panic(Failure::NegativeExponent { exponent }, unsafe {
        self::location(location)
    })
}

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

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Set in the environment of the child process that is meant to panic.
    const CHILD: &str = "BARYON_RUNTIME_TEST_PANIC_CHILD";

    /// The location the child reports.
    static HERE: Location = Location::new("md.byn", 58, 13);

    /// A panic ends the process, so the test runs itself again as a child that panics, and checks
    /// what the child printed and how it exited.
    #[test]
    fn a_panic_prints_its_message_and_exits_with_status_101() {
        if std::env::var_os(CHILD).is_some() {
            // SAFETY: `HERE` is a static location.
            unsafe { baryon_rt_panic_index(10, 0, 8, &raw const HERE) }
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "abi::tests::a_panic_prints_its_message_and_exits_with_status_101",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(101));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "panic: index 10 is out of range for dimension 0 of length 8\n  --> md.byn:58:13\n"
        );
    }

    #[test]
    fn allocates_and_frees_an_array_through_the_exported_functions() {
        // SAFETY: a null location is allowed.
        let pointer = unsafe { baryon_rt_allocate_array(3, 8, 8, std::ptr::null()) };
        #[allow(
            clippy::cast_ptr_alignment,
            reason = "the allocation is aligned to 8 bytes, as an `i64` needs"
        )]
        let pointer = pointer.cast::<i64>();
        // SAFETY: the allocation holds three `i64`s, aligned to 8 bytes, and nothing else refers
        // to it while the slice lives. Integers have no invalid bit patterns.
        let elements = unsafe { std::slice::from_raw_parts_mut(pointer, 3) };
        elements.copy_from_slice(&[0, 2, 4]);
        assert_eq!(elements, [0, 2, 4]);
        // SAFETY: freed once, with the size and alignment it was allocated with.
        unsafe { baryon_rt_deallocate(pointer.cast(), 24, 8) };
    }
}
