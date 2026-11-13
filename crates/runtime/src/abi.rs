//! The functions compiled code calls, exported with C names and the C calling convention.
//!
//! Every name starts with `baryon_rt_`, so the runtime's symbols never collide with a program's. The
//! backend declares each function with the signature written here and calls it by name; nothing
//! else about the runtime is visible to compiled code. Each function is a thin wrapper: it turns
//! raw pointers into references, under the promise in its `# Safety` section, and calls the safe
//! code in the other modules.

mod accumulate;
mod array_files;
mod complex;
mod dict;
mod files;
mod fits;
mod hdf5;
mod memory;
mod netcdf;
mod npy;
mod numbers;
mod output;
mod pairwise;
mod panics;
mod parallel;
mod paths;
mod philox;
mod print_array;
mod process;
mod round;
mod sort;
mod streams;
mod string_lists;
mod string_methods;
mod strings;
mod subprocess;
mod sum;
mod text;

pub use accumulate::{
    baryon_rt_accumulator_chunk_done, baryon_rt_accumulator_finish, baryon_rt_accumulator_new,
    baryon_rt_accumulator_slot, baryon_rt_accumulator_table,
};
pub use array_files::{
    ArrayHeader, ElementLayout, baryon_rt_array_from_file, baryon_rt_array_to_file,
};
pub use complex::{
    baryon_rt_power_complex64, baryon_rt_power_complex128, baryon_rt_print_complex64,
    baryon_rt_print_complex128,
};
pub use dict::{
    baryon_rt_dict_find, baryon_rt_dict_free, baryon_rt_dict_insert, baryon_rt_dict_length,
    baryon_rt_dict_next, baryon_rt_dict_remove, baryon_rt_panic_key,
};
pub use files::{
    OsError, RaisedError, baryon_rt_file_close, baryon_rt_file_drop, baryon_rt_file_flush,
    baryon_rt_file_open, baryon_rt_file_read, baryon_rt_file_read_line, baryon_rt_file_read_lines,
    baryon_rt_file_write,
};
pub use fits::{baryon_rt_fits_cards, baryon_rt_fits_read, baryon_rt_fits_write};
pub use hdf5::{baryon_rt_hdf5_chunks, baryon_rt_hdf5_open, baryon_rt_hdf5_read};
pub use memory::{baryon_rt_allocate, baryon_rt_allocate_array, baryon_rt_deallocate};
pub use netcdf::{
    baryon_rt_netcdf_chunks, baryon_rt_netcdf_define, baryon_rt_netcdf_open, baryon_rt_netcdf_read,
    baryon_rt_netcdf_write,
};
pub use npy::{baryon_rt_array_load, baryon_rt_array_save};
pub use numbers::{
    RaisedValueError, ValueError, baryon_rt_str_parse_float, baryon_rt_str_parse_int,
};
pub use output::{
    baryon_rt_flush, baryon_rt_print_bool, baryon_rt_print_f32, baryon_rt_print_f64,
    baryon_rt_print_i64, baryon_rt_print_str, baryon_rt_print_str_quoted, baryon_rt_print_u64,
};
pub use pairwise::{
    baryon_rt_pairwise_combine_f32, baryon_rt_pairwise_combine_f64, baryon_rt_pairwise_leaf_end,
    baryon_rt_pairwise_leaves,
};
pub use panics::{
    baryon_rt_panic, baryon_rt_panic_exponent, baryon_rt_panic_index,
    baryon_rt_panic_length_mismatch, baryon_rt_panic_slice,
};
pub use parallel::{baryon_rt_chunk_count, baryon_rt_parallel_for};
pub use paths::{
    baryon_rt_current_directory, baryon_rt_path_is, baryon_rt_path_join, baryon_rt_path_list,
    baryon_rt_path_make_directory, baryon_rt_path_part, baryon_rt_path_remove,
    baryon_rt_path_rename,
};
pub use philox::{baryon_rt_philox_new, baryon_rt_philox_uniform};
pub use print_array::{
    ELEMENT_BOOL, ELEMENT_COMPLEX64, ELEMENT_COMPLEX128, ELEMENT_F32, ELEMENT_F64, ELEMENT_I8,
    ELEMENT_I16, ELEMENT_I32, ELEMENT_I64, ELEMENT_U8, ELEMENT_U16, ELEMENT_U32, ELEMENT_U64,
    baryon_rt_print_array,
};
pub use process::{
    baryon_rt_argument, baryon_rt_argument_count, baryon_rt_environment_entries,
    baryon_rt_environment_get, baryon_rt_exit,
};
pub use round::{baryon_rt_round_f32, baryon_rt_round_f64, baryon_rt_round_i64};
pub use sort::{baryon_rt_sort, baryon_rt_sort_str};
pub use streams::{
    RaisedEndOfInput, baryon_rt_file_print, baryon_rt_file_standard, baryon_rt_input,
};
pub use string_lists::{
    OwnedList, TextValue, baryon_rt_str_join, baryon_rt_str_split, baryon_rt_str_split_whitespace,
};
pub use string_methods::{
    baryon_rt_str_ends_with, baryon_rt_str_find, baryon_rt_str_lower, baryon_rt_str_replace,
    baryon_rt_str_starts_with, baryon_rt_str_strip, baryon_rt_str_strip_characters,
    baryon_rt_str_take, baryon_rt_str_upper,
};
pub use strings::{baryon_rt_str_compare, baryon_rt_str_contains, baryon_rt_str_length};
pub use subprocess::{CompletedProcess, baryon_rt_subprocess_run};
pub use sum::{
    baryon_rt_dot_f32, baryon_rt_dot_f64, baryon_rt_dot_strided_f32, baryon_rt_dot_strided_f64,
    baryon_rt_sum_f32, baryon_rt_sum_f64, baryon_rt_sum_strided_f32, baryon_rt_sum_strided_f64,
};
pub use text::{
    OwnedText, TextBuilder, baryon_rt_text_bool, baryon_rt_text_complex64,
    baryon_rt_text_complex128, baryon_rt_text_f32, baryon_rt_text_f64, baryon_rt_text_finish,
    baryon_rt_text_i64, baryon_rt_text_new, baryon_rt_text_str, baryon_rt_text_u64,
};

use crate::Location;

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
