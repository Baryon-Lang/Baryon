//! The functions compiled code calls for heap memory: allocating a value, allocating an array
//! with its length and size checked, and freeing either.

use super::location;
use crate::Location;
use crate::memory;
use crate::panic::panic;
use std::ptr::NonNull;

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

#[cfg(test)]
#[allow(
    unsafe_code,
    reason = "the tests call the exported functions as compiled code does"
)]
mod tests {
    use super::*;

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
