//! The allocator shim: heap memory for arrays and other owned values, from the system allocator.
//!
//! The design plan asks for "an allocator shim over the system `malloc`". Rust's
//! [`System`] allocator is exactly that: `malloc` and `free`, or `posix_memalign` when the
//! alignment is larger than `malloc` guarantees. Going through it rather than calling `malloc`
//! directly keeps the shim free of hand-written foreign declarations, and leaves one place to
//! swap in a different allocator, with a benchmark to justify it.
//!
//! Large arrays need nothing special: the system allocator maps big requests straight from the
//! operating system, whose pages are only backed by memory once written.

use crate::panic::Failure;
use std::alloc::{GlobalAlloc, Layout, System};
use std::num::NonZeroUsize;
use std::ptr::NonNull;

/// The layout of an array of `count` elements of `element_size` bytes each, aligned to `align`.
///
/// # Errors
///
/// [`Failure::NegativeLength`] for a negative `count`, [`Failure::ArrayTooLarge`] when the size in
/// bytes overflows, and [`Failure::InvalidLayout`] for an alignment that is not a power of two.
pub(crate) fn array_layout(
    count: i64,
    element_size: usize,
    align: usize,
) -> Result<Layout, Failure<'static>> {
    let Ok(elements) = usize::try_from(count) else {
        return Err(Failure::NegativeLength { count });
    };
    let too_large = Failure::ArrayTooLarge {
        count,
        element_size,
    };
    let bytes = elements.checked_mul(element_size).ok_or(too_large)?;
    Layout::from_size_align(bytes, align).map_err(|_| {
        if align.is_power_of_two() {
            too_large
        } else {
            Failure::InvalidLayout { bytes, align }
        }
    })
}

/// The layout of a value of `bytes` bytes aligned to `align`.
///
/// # Errors
///
/// [`Failure::InvalidLayout`] for an alignment that is not a power of two, or a size that
/// overflows once rounded up to it.
pub(crate) fn layout(bytes: usize, align: usize) -> Result<Layout, Failure<'static>> {
    Layout::from_size_align(bytes, align).map_err(|_| Failure::InvalidLayout { bytes, align })
}

/// Allocates uninitialized memory for `layout`.
///
/// A zero-sized layout allocates nothing and returns a pointer whose address is the alignment:
/// never null, suitably aligned, and never read or written through. [`deallocate`] knows not to
/// free it. An empty array therefore costs no system call.
///
/// # Errors
///
/// [`Failure::OutOfMemory`] when the system allocator returns null.
pub(crate) fn allocate(layout: Layout) -> Result<NonNull<u8>, Failure<'static>> {
    if layout.size() == 0 {
        return Ok(dangling(layout));
    }
    // SAFETY: `GlobalAlloc::alloc` requires a layout of non-zero size, checked above.
    #[allow(unsafe_code, reason = "calls the system allocator")]
    let pointer = unsafe { System.alloc(layout) };
    NonNull::new(pointer).ok_or(Failure::OutOfMemory {
        bytes: layout.size(),
    })
}

/// Frees memory [`allocate`] returned.
///
/// # Safety
///
/// `pointer` came from [`allocate`] with this same `layout` and has not been freed since.
#[allow(unsafe_code, reason = "frees memory through the system allocator")]
pub(crate) unsafe fn deallocate(pointer: NonNull<u8>, layout: Layout) {
    if layout.size() == 0 {
        return;
    }
    // SAFETY: the caller guarantees `pointer` came from `allocate`, which got it from `System`
    // with this layout, and that it is still allocated.
    unsafe { System.dealloc(pointer.as_ptr(), layout) }
}

/// The non-null, aligned pointer a zero-sized allocation stands for.
fn dangling(layout: Layout) -> NonNull<u8> {
    // `Layout` guarantees a non-zero power-of-two alignment, so the fallback is never taken.
    let align = NonZeroUsize::new(layout.align()).unwrap_or(NonZeroUsize::MIN);
    NonNull::without_provenance(align)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_aligned_writable_memory() {
        let layout = array_layout(1000, 8, 64).unwrap();
        let pointer = allocate(layout).unwrap();
        assert_eq!(pointer.as_ptr().addr() % 64, 0);
        // SAFETY: the allocation holds 1000 `f64`s, aligned to 64 bytes, and nothing else refers
        // to it while the slice lives. Floats have no invalid bit patterns.
        #[allow(unsafe_code, reason = "views the allocation under test as a slice")]
        let values =
            unsafe { std::slice::from_raw_parts_mut(pointer.cast::<f64>().as_ptr(), 1000) };
        values.fill(1.5);
        assert!(
            values
                .iter()
                .all(|&value| (value - 1.5).abs() < f64::EPSILON)
        );
        // SAFETY: `pointer` came from `allocate` with `layout` and is freed once.
        #[allow(unsafe_code, reason = "frees the allocation under test")]
        unsafe {
            deallocate(pointer, layout);
        }
    }

    #[test]
    fn an_empty_array_is_an_aligned_pointer_and_no_allocation() {
        let layout = array_layout(0, 8, 32).unwrap();
        let pointer = allocate(layout).unwrap();
        assert_eq!(pointer.as_ptr().addr(), 32);
        // SAFETY: a zero-sized allocation from `allocate`; freeing it does nothing.
        #[allow(unsafe_code, reason = "frees the empty allocation under test")]
        unsafe {
            deallocate(pointer, layout);
        }
    }

    #[test]
    fn rejects_negative_overflowing_and_misaligned_arrays() {
        assert_eq!(
            array_layout(-1, 8, 8),
            Err(Failure::NegativeLength { count: -1 })
        );
        assert_eq!(
            array_layout(i64::MAX, 8, 8),
            Err(Failure::ArrayTooLarge {
                count: i64::MAX,
                element_size: 8
            })
        );
        assert_eq!(
            array_layout(4, 8, 3),
            Err(Failure::InvalidLayout {
                bytes: 32,
                align: 3
            })
        );
        assert_eq!(
            layout(8, 0),
            Err(Failure::InvalidLayout { bytes: 8, align: 0 })
        );
    }

    #[test]
    fn reports_memory_the_system_cannot_provide() {
        let layout = layout(isize::MAX.cast_unsigned() - 64, 8).unwrap();
        assert_eq!(
            allocate(layout),
            Err(Failure::OutOfMemory {
                bytes: layout.size()
            })
        );
    }
}
