//! The runtime: the code linked into every compiled program, beside the program's own.
//!
//! The compiler turns a program into machine code, but some of what that code does is the same in
//! every program and is written once, here: getting memory from the system, ending the program
//! when a check fails, printing, and running `prange` loops on every core. The design plan asks
//! for a runtime "as small as C's": no garbage collector, no scheduler, no reflection.
//!
//! Where it sits: the runtime is not part of the compiler. It is built as a static library
//! (`libruntime.a`) that the driver links into each program (roadmap 1.11), and the backend emits
//! calls to the functions it exports (roadmap 1.10). It is the one crate of kind `runtime`, which
//! may depend on no other workspace crate, so nothing from the compiler ends up inside a user's
//! program (decision 0021).
//!
//! What compiled code calls, all with C names and the C calling convention:
//!
//! | Function | Job |
//! | --- | --- |
//! | [`baryon_rt_allocate`], [`baryon_rt_deallocate`] | Heap memory for a value, from the system allocator |
//! | [`baryon_rt_allocate_array`] | Memory for an array's elements, checking the length and the size; an IR `NewArray` |
//! | [`baryon_rt_panic`] | End the program with a message the compiler wrote, such as a failed `assert` |
//! | [`baryon_rt_panic_index`] | End the program for an index out of range (decision 0010) |
//! | [`baryon_rt_panic_length_mismatch`] | End the program for arrays of different lengths combined element by element |
//! | [`baryon_rt_chunk_count`], [`baryon_rt_parallel_for`] | Run a `prange` loop on the thread pool, in fixed chunks |
//! | [`baryon_rt_accumulator_new`], [`baryon_rt_accumulator_slot`], [`baryon_rt_accumulator_chunk_done`], [`baryon_rt_accumulator_finish`] | Give each chunk of a `prange` loop private blocks of an array listed in `accumulate=`, and combine them into it in the fixed tree |
//! | [`baryon_rt_print_i64`], [`baryon_rt_print_u64`], [`baryon_rt_print_f64`], [`baryon_rt_print_f32`], [`baryon_rt_print_bool`], [`baryon_rt_print_str`] | Print one argument of `print` to standard output, or a field of an error that ends `main` to standard error, as Python writes it |
//! | [`baryon_rt_text_new`], [`baryon_rt_text_str`], [`baryon_rt_text_i64`], [`baryon_rt_text_u64`], [`baryon_rt_text_f64`], [`baryon_rt_text_f32`], [`baryon_rt_text_bool`], [`baryon_rt_text_finish`] | Build an owned `str` from pieces, each value formatted with a [`FormatSpec`] as Python's `format` writes it: an f-string, a `copy()` or a `+` |
//! | [`baryon_rt_sum_f64`], [`baryon_rt_sum_f32`] | Add a float array's elements in `NumPy`'s pairwise order, for `a.sum()` |
//! | [`baryon_rt_print_str_quoted`] | Print a `str` field of an error that ends `main` as Python's `repr` writes it, quoted |
//!
//! A panic prints its message, the [`Location`] of the check and, when the program has debug
//! information, a backtrace of its `.byn` frames to standard error, then exits with status 101. Panics cannot be caught (decision 0005), so the runtime never unwinds.
//!
//! This crate is the one place outside a backend where `unsafe` code is allowed, under the rules
//! decision 0021 sets: each item that uses it says why, each `unsafe` block carries a
//! `// SAFETY:` comment and does one unsafe thing, and the logic lives in safe functions the
//! exported wrappers call.
//!
//! # Example
//!
//! The calls compiled code makes for `a = zeros(n)` followed by `a[i]` with `i` out of range,
//! written in Rust:
//!
//! ```no_run
//! use runtime::{Location, baryon_rt_allocate_array, baryon_rt_deallocate, baryon_rt_panic_index};
//!
//! static ALLOCATION: Location = Location::new("heat.byn", 4, 9);
//! static INDEX: Location = Location::new("heat.byn", 5, 5);
//!
//! let (n, i) = (8_i64, 10_i64);
//! // SAFETY: the locations are statics; the elements are written before they are read and
//! // freed once with the size they were allocated with.
//! unsafe {
//!     let a = baryon_rt_allocate_array(n, 8, 8, &ALLOCATION).cast::<f64>();
//!     for index in 0..8 {
//!         a.add(index).write(0.0);
//!     }
//!     if !(0..n).contains(&i) {
//!         // Prints "panic: index 10 is out of range for dimension 0 of length 8" and
//!         // "  --> heat.byn:5:5", then exits with status 101.
//!         baryon_rt_panic_index(i, 0, n, &INDEX);
//!     }
//!     baryon_rt_deallocate(a.cast(), 64, 8);
//! }
//! ```

mod abi;
mod accumulate;
mod backtrace;
mod format;
mod location;
mod memory;
mod panic;
mod parallel;
mod print;
mod sum;

pub use abi::{
    OwnedText, TextBuilder, baryon_rt_accumulator_chunk_done, baryon_rt_accumulator_finish,
    baryon_rt_accumulator_new, baryon_rt_accumulator_slot, baryon_rt_allocate,
    baryon_rt_allocate_array, baryon_rt_chunk_count, baryon_rt_deallocate, baryon_rt_panic,
    baryon_rt_panic_index, baryon_rt_panic_length_mismatch, baryon_rt_parallel_for,
    baryon_rt_print_bool, baryon_rt_print_f32, baryon_rt_print_f64, baryon_rt_print_i64,
    baryon_rt_print_str, baryon_rt_print_str_quoted, baryon_rt_print_u64, baryon_rt_sum_f32,
    baryon_rt_sum_f64, baryon_rt_text_bool, baryon_rt_text_f32, baryon_rt_text_f64,
    baryon_rt_text_finish, baryon_rt_text_i64, baryon_rt_text_new, baryon_rt_text_str,
    baryon_rt_text_u64,
};
pub use accumulate::Accumulator;
pub use format::FormatSpec;
pub use location::Location;
pub use parallel::ChunkBody;
