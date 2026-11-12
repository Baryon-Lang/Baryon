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
//! | [`baryon_rt_panic_slice`] | End the program for a slice out of range |
//! | [`baryon_rt_panic_length_mismatch`] | End the program for arrays of different lengths combined element by element |
//! | [`baryon_rt_chunk_count`], [`baryon_rt_parallel_for`] | Run a `prange` loop on the thread pool, in fixed chunks |
//! | [`baryon_rt_accumulator_new`], [`baryon_rt_accumulator_slot`], [`baryon_rt_accumulator_chunk_done`], [`baryon_rt_accumulator_finish`] | Give each chunk of a `prange` loop private blocks of an array listed in `accumulate=`, and combine them into it in the fixed tree |
//! | [`baryon_rt_print_i64`], [`baryon_rt_print_u64`], [`baryon_rt_print_f64`], [`baryon_rt_print_f32`], [`baryon_rt_print_bool`], [`baryon_rt_print_str`] | Print one argument of `print` to standard output, or a field of an error that ends `main` to standard error, as Python writes it |
//! | [`baryon_rt_text_new`], [`baryon_rt_text_str`], [`baryon_rt_text_i64`], [`baryon_rt_text_u64`], [`baryon_rt_text_f64`], [`baryon_rt_text_f32`], [`baryon_rt_text_bool`], [`baryon_rt_text_finish`] | Build an owned `str` from pieces, each value formatted with a [`FormatSpec`] as Python's `format` writes it: an f-string, a `copy()` or a `+` |
//! | [`baryon_rt_sum_f64`], [`baryon_rt_sum_f32`] | Add a float array's elements in `NumPy`'s pairwise order, for `a.sum()` |
//! | [`baryon_rt_sum_strided_f64`], [`baryon_rt_sum_strided_f32`] | The same for an `Array` that may be a view, with any strides |
//! | [`baryon_rt_pairwise_leaves`], [`baryon_rt_pairwise_leaf_end`], [`baryon_rt_pairwise_combine_f64`], [`baryon_rt_pairwise_combine_f32`] | Sum a whole-array expression one pairwise leaf at a time without storing it, with the bits of `a.sum()` (decision 0041) |
//! | [`baryon_rt_dot_f64`], [`baryon_rt_dot_f32`] | Add the products of two float arrays' elements in the same order, for `a.dot(b)` |
//! | [`baryon_rt_dot_strided_f64`], [`baryon_rt_dot_strided_f32`] | The same for two rank-1 `Array`s that may be views, each with its stride |
//! | [`baryon_rt_print_complex128`], [`baryon_rt_print_complex64`] | Print a complex number as Python's `repr` writes it, such as `(1+2j)` |
//! | [`baryon_rt_power_complex128`], [`baryon_rt_power_complex64`] | `z ** w` on complex numbers, as Python computes it |
//! | [`baryon_rt_str_compare`], [`baryon_rt_str_contains`], [`baryon_rt_str_length`] | Compare two `str`s by code point, find one inside another, and count characters, for `==`, `<`, `in` and `len` of `str` |
//! | [`baryon_rt_print_array`] | Print an array as `NumPy`'s `print` writes an `ndarray`, such as `[1.  2.5]`, its element type one of the `ELEMENT_*` constants (decision 0033) |
//! | [`baryon_rt_print_str_quoted`] | Print a `str` inside a tuple or struct, or a field of an error that ends `main`, as Python's `repr` writes it, quoted |
//! | [`baryon_rt_dict_find`], [`baryon_rt_dict_insert`], [`baryon_rt_dict_remove`], [`baryon_rt_dict_next`], [`baryon_rt_dict_length`], [`baryon_rt_dict_free`] | The hash table behind `dict`: find a key, add one, remove an entry, step through the entries in insertion order, count and free (decision 0035) |
//! | [`baryon_rt_panic_key`] | End the program for `d[k]` of a key that is not in the `dict` |
//! | [`baryon_rt_argument_count`], [`baryon_rt_argument`], [`baryon_rt_environment_get`], [`baryon_rt_exit`] | Read the command-line arguments and an environment variable as owned `str`s, and end the program with an exit status, for `sys.argv`, `os.environ` and `sys.exit` (decision 0036) |
//! | [`baryon_rt_file_open`], [`baryon_rt_file_read`], [`baryon_rt_file_read_line`], [`baryon_rt_file_read_lines`], [`baryon_rt_file_write`], [`baryon_rt_file_flush`], [`baryon_rt_file_close`], [`baryon_rt_file_drop`] | Open a text file and read, write, flush and close it, each failure written as an `OSError` the way a compiled function raises one (decision 0036) |
//! | [`baryon_rt_environment_entries`] | List every environment variable's name and value, for `os.environ.copy()` |
//! | [`baryon_rt_path_part`], [`baryon_rt_path_join`], [`baryon_rt_path_is`] | The parts of a `pathlib.Path` as CPython's `pathlib` gives them, joining with `/`, and whether a path names a file or a directory (decision 0036) |
//! | [`baryon_rt_path_list`], [`baryon_rt_path_remove`], [`baryon_rt_path_rename`], [`baryon_rt_path_make_directory`], [`baryon_rt_current_directory`] | List a directory, remove and rename a file, make directories and read the current directory, each failure written as an `OSError` |
//! | [`baryon_rt_array_from_file`], [`baryon_rt_array_to_file`] | Read a file of raw elements into a new array and write an array's elements to a file, as `NumPy`'s `fromfile` and `tofile` do, each failure an `OSError` (decision 0036) |
//! | [`baryon_rt_array_load`], [`baryon_rt_array_save`] | Read a `.npy` file into a new array of the element type and rank the program names, and write an array as one, as `NumPy`'s `load` and `save` do, each failure an `OSError` (decision 0043) |
//! | [`baryon_rt_fits_cards`], [`baryon_rt_fits_read`], [`baryon_rt_fits_write`] | Read the header cards and the image of one HDU of a FITS file, and write an array as a FITS image with a program's cards, as `AstroPy`'s `getheader`, `getdata` and `writeto` do, each failure an `OSError` (decision 0044) |
//! | [`baryon_rt_netcdf_open`], [`baryon_rt_netcdf_read`] | Describe the header of a `NetCDF` classic file and read one of its variables into a new array, scaled as `netCDF4` scales it, for the `netCDF4` module's `Dataset`, each failure an `OSError` (decision 0047) |
//! | [`baryon_rt_netcdf_define`], [`baryon_rt_netcdf_write`] | Apply one change (a new file, a dimension, a variable or an attribute) to a `NetCDF` classic file, and write an array as all the values of one of its variables, packed as `netCDF4` packs them, each failure an `OSError` (decision 0047) |
//! | [`baryon_rt_file_standard`], [`baryon_rt_file_print`], [`baryon_rt_input`] | Make the `File` that `sys.stdin`, `sys.stdout` or `sys.stderr` gives, write a `print(..., file=f)` to its file, and read a line for `input`, raising `EOFError` at the end of the input (decision 0036) |
//! | [`baryon_rt_subprocess_run`] | Run another program from an argument list, with no shell, wait for it and write a [`CompletedProcess`]: its exit status and, when asked, its output, for `subprocess.run` (decision 0036) |
//! | [`baryon_rt_flush`] | Write out a line `print(..., end="")` left unended, before the C `main` returns |
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
mod complex;
mod dict;
mod files;
mod format;
mod location;
mod memory;
mod numbers;
mod panic;
mod parallel;
mod paths;
mod philox;
mod print;
mod print_array;
mod streams;
mod strings;
mod subprocess;
mod sum;

pub use abi::{
    ArrayHeader, CompletedProcess, ELEMENT_BOOL, ELEMENT_COMPLEX64, ELEMENT_COMPLEX128,
    ELEMENT_F32, ELEMENT_F64, ELEMENT_I8, ELEMENT_I16, ELEMENT_I32, ELEMENT_I64, ELEMENT_U8,
    ELEMENT_U16, ELEMENT_U32, ELEMENT_U64, ElementLayout, OsError, OwnedList, OwnedText,
    RaisedEndOfInput, RaisedError, RaisedValueError, TextBuilder, TextValue, ValueError,
    baryon_rt_accumulator_chunk_done, baryon_rt_accumulator_finish, baryon_rt_accumulator_new,
    baryon_rt_accumulator_slot, baryon_rt_allocate, baryon_rt_allocate_array, baryon_rt_argument,
    baryon_rt_argument_count, baryon_rt_array_from_file, baryon_rt_array_load,
    baryon_rt_array_save, baryon_rt_array_to_file, baryon_rt_chunk_count,
    baryon_rt_current_directory, baryon_rt_deallocate, baryon_rt_dict_find, baryon_rt_dict_free,
    baryon_rt_dict_insert, baryon_rt_dict_length, baryon_rt_dict_next, baryon_rt_dict_remove,
    baryon_rt_dot_f32, baryon_rt_dot_f64, baryon_rt_dot_strided_f32, baryon_rt_dot_strided_f64,
    baryon_rt_environment_entries, baryon_rt_environment_get, baryon_rt_exit, baryon_rt_file_close,
    baryon_rt_file_drop, baryon_rt_file_flush, baryon_rt_file_open, baryon_rt_file_print,
    baryon_rt_file_read, baryon_rt_file_read_line, baryon_rt_file_read_lines,
    baryon_rt_file_standard, baryon_rt_file_write, baryon_rt_fits_cards, baryon_rt_fits_read,
    baryon_rt_fits_write, baryon_rt_flush, baryon_rt_input, baryon_rt_netcdf_define,
    baryon_rt_netcdf_open, baryon_rt_netcdf_read, baryon_rt_netcdf_write,
    baryon_rt_pairwise_combine_f32, baryon_rt_pairwise_combine_f64, baryon_rt_pairwise_leaf_end,
    baryon_rt_pairwise_leaves, baryon_rt_panic, baryon_rt_panic_exponent, baryon_rt_panic_index,
    baryon_rt_panic_key, baryon_rt_panic_length_mismatch, baryon_rt_panic_slice,
    baryon_rt_parallel_for, baryon_rt_path_is, baryon_rt_path_join, baryon_rt_path_list,
    baryon_rt_path_make_directory, baryon_rt_path_part, baryon_rt_path_remove,
    baryon_rt_path_rename, baryon_rt_philox_new, baryon_rt_philox_uniform,
    baryon_rt_power_complex64, baryon_rt_power_complex128, baryon_rt_print_array,
    baryon_rt_print_bool, baryon_rt_print_complex64, baryon_rt_print_complex128,
    baryon_rt_print_f32, baryon_rt_print_f64, baryon_rt_print_i64, baryon_rt_print_str,
    baryon_rt_print_str_quoted, baryon_rt_print_u64, baryon_rt_round_f32, baryon_rt_round_f64,
    baryon_rt_round_i64, baryon_rt_sort, baryon_rt_sort_str, baryon_rt_str_compare,
    baryon_rt_str_contains, baryon_rt_str_ends_with, baryon_rt_str_find, baryon_rt_str_join,
    baryon_rt_str_length, baryon_rt_str_lower, baryon_rt_str_parse_float, baryon_rt_str_parse_int,
    baryon_rt_str_replace, baryon_rt_str_split, baryon_rt_str_split_whitespace,
    baryon_rt_str_starts_with, baryon_rt_str_strip, baryon_rt_str_strip_characters,
    baryon_rt_str_take, baryon_rt_str_upper, baryon_rt_subprocess_run, baryon_rt_sum_f32,
    baryon_rt_sum_f64, baryon_rt_sum_strided_f32, baryon_rt_sum_strided_f64, baryon_rt_text_bool,
    baryon_rt_text_complex64, baryon_rt_text_complex128, baryon_rt_text_f32, baryon_rt_text_f64,
    baryon_rt_text_finish, baryon_rt_text_i64, baryon_rt_text_new, baryon_rt_text_str,
    baryon_rt_text_u64,
};
pub use accumulate::Accumulator;
pub use dict::{DictLayout, KeyPart, STR_PART, Table};
pub use files::OpenFile;
pub use format::FormatSpec;
pub use location::Location;
pub use parallel::ChunkBody;
pub use philox::Philox;
