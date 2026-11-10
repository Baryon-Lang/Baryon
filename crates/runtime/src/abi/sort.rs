//! The functions compiled code calls for `sorted(values)` and `round(x, digits)` (roadmap
//! 1.13.13).
//!
//! Sorting takes the list's elements in place, as their address and count. Numbers and `bool`s
//! go through one function that takes the element type as one of the `ELEMENT_*` constants of
//! the `print_array` module; `str`s have their own, since each is two words that point to its
//! bytes. Every sort is stable, as Python's is: equal elements keep their order, also when
//! `reverse` is set, which compares the other way round rather than reversing the result.
//!
//! Floats sort with every NaN last, in either direction, as `numpy.sort` puts them (Collins,
//! 2026-11-10). Python's order with a NaN depends on where the NaN starts. `-0.0` and `0.0` are
//! equal, so they keep their order.

use super::print_array::{
    ELEMENT_BOOL, ELEMENT_F32, ELEMENT_F64, ELEMENT_I8, ELEMENT_I16, ELEMENT_I32, ELEMENT_I64,
    ELEMENT_U8, ELEMENT_U16, ELEMENT_U32, ELEMENT_U64,
};
use crate::panic::{Failure, panic};
use std::cmp::Ordering;
use std::ffi::c_void;

/// Sorts the `count` elements at `values` in place, from the smallest, or from the largest when
/// `reverse`. `element` is the element type, one of the `ELEMENT_*` constants for a `bool`, an
/// integer or a float; a complex number has no order.
///
/// It is a stable merge sort: O(n log n) comparisons, and a buffer of half the elements.
///
/// # Safety
///
/// `values` points to `count` initialized elements of the type `element` names, which nothing
/// else reads or writes during the call, or `count` is 0 or less.
///
/// # Panics
///
/// When `element` names no type with an order, which is a bug in the compiler.
///
/// # Example
///
/// ```
/// let mut values = [3.0_f64, f64::NAN, -1.0, 2.0];
/// // SAFETY: the pointer and count are an array's.
/// unsafe { runtime::baryon_rt_sort(values.as_mut_ptr().cast(), 4, runtime::ELEMENT_F64, true) };
/// assert_eq!(values[..3], [3.0, 2.0, -1.0]);
/// assert!(values[3].is_nan());
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_sort(
    values: *mut c_void,
    count: i64,
    element: u32,
    reverse: bool,
) {
    /// Sorts the elements as `T`, ordered by `Ord`.
    ///
    /// # Safety
    ///
    /// As for [`baryon_rt_sort`], with `T` the element type.
    unsafe fn sort<T: Ord>(values: *mut c_void, count: i64, reverse: bool) {
        // SAFETY: the caller guarantees `count` initialized `T`s at `values`, unaliased.
        let values = unsafe { elements(values.cast::<T>(), count) };
        if reverse {
            values.sort_by(|a, b| b.cmp(a));
        } else {
            values.sort();
        }
    }
    // Each arm reads the elements as the type `element` names, as the caller guarantees they
    // are; a `bool` is one byte holding 0 or 1, ordered as a `u8`.
    match element {
        // SAFETY: as the comment above says, for each arm.
        ELEMENT_BOOL | ELEMENT_U8 => unsafe { sort::<u8>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_I8 => unsafe { sort::<i8>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_I16 => unsafe { sort::<i16>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_I32 => unsafe { sort::<i32>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_I64 => unsafe { sort::<i64>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_U16 => unsafe { sort::<u16>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_U32 => unsafe { sort::<u32>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_U64 => unsafe { sort::<u64>(values, count, reverse) },
        // SAFETY: as above.
        ELEMENT_F32 => sort_floats(unsafe { elements(values.cast::<f32>(), count) }, reverse),
        // SAFETY: as above.
        ELEMENT_F64 => sort_floats(unsafe { elements(values.cast::<f64>(), count) }, reverse),
        _ => panic(
            Failure::Message("sorting an element type with no order"),
            None,
        ),
    }
}

/// Sorts the `count` `str`s at `values` in place, by code point with a prefix first, as Python
/// orders them, from the smallest or, when `reverse`, from the largest. Each `str` is two words
/// (decision 0022): the address of its bytes, then its length, whose top bit says the bytes are
/// not owned. Only the two words move; the bytes stay where they are.
///
/// # Safety
///
/// `values` points to `count` initialized `str` values, each pointing to as many readable bytes
/// as its length says with the top bit masked off, which nothing else reads or writes during the
/// call, or `count` is 0 or less.
///
/// # Example
///
/// ```
/// let words = ["neon", "argon", "\u{c5}ngstr\u{f6}m", "ar"];
/// let mut values = words.map(|word| [word.as_ptr() as u64, word.len() as u64 | 1 << 63]);
/// // SAFETY: the pointer and count are an array's, and each value borrows a constant.
/// unsafe { runtime::baryon_rt_sort_str(values.as_mut_ptr().cast(), 4, false) };
/// let first: Vec<u64> = values.iter().map(|value| value[1] & !(1 << 63)).collect();
/// assert_eq!(first, [2, 5, 4, 10]);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_sort_str(values: *mut c_void, count: i64, reverse: bool) {
    // SAFETY: the caller guarantees `count` initialized `str` values, unaliased.
    let values = unsafe { elements(values.cast::<Text>(), count) };
    let order = |a: &Text, b: &Text| {
        // SAFETY: the caller guarantees each value's bytes are readable.
        let (a, b) = (unsafe { a.bytes() }, unsafe { b.bytes() });
        a.cmp(b)
    };
    if reverse {
        values.sort_by(|a, b| order(b, a));
    } else {
        values.sort_by(order);
    }
}

/// A `str` value as compiled code holds it (decision 0022).
#[repr(C)]
struct Text {
    /// The address of the UTF-8 bytes.
    bytes: *const u8,
    /// The number of bytes, with the top bit set when they are not owned.
    length: u64,
}

impl Text {
    /// The text's bytes.
    ///
    /// # Safety
    ///
    /// `bytes` points to as many readable bytes as `length` says, with the top bit masked off,
    /// that stay readable while the slice lives.
    #[allow(unsafe_code, reason = "reads memory compiled code passes")]
    unsafe fn bytes(&self) -> &[u8] {
        let length = usize::try_from(self.length & !(1 << 63)).unwrap_or(0);
        if length == 0 {
            return &[];
        }
        // SAFETY: the caller guarantees `length` readable bytes at `bytes`.
        unsafe { std::slice::from_raw_parts(self.bytes, length) }
    }
}

/// Sorts `values` stably from the smallest, or from the largest when `reverse`, with every NaN
/// last either way.
fn sort_floats<T: PartialOrd + Copy>(values: &mut [T], reverse: bool) {
    #[expect(clippy::eq_op, reason = "`x != x` holds only for NaN")]
    let is_nan = |x: &T| x != x;
    values.sort_by(|a, b| match (is_nan(a), is_nan(b)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let order = a.partial_cmp(b).unwrap_or(Ordering::Equal);
            if reverse { order.reverse() } else { order }
        }
    });
}

/// The `count` values at `values` as a mutable slice, empty for a count of 0 or less.
///
/// # Safety
///
/// `values` points to `count` initialized values that nothing else reads or writes while the
/// slice lives, or `count` is 0 or less.
#[allow(unsafe_code, reason = "reads memory compiled code passes")]
unsafe fn elements<'a, T>(values: *mut T, count: i64) -> &'a mut [T] {
    match usize::try_from(count) {
        Ok(count) if count > 0 => {
            // SAFETY: the caller guarantees `count` initialized, unaliased values.
            unsafe { std::slice::from_raw_parts_mut(values, count) }
        }
        _ => &mut [],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sorts `values` of the type `element` names through the exported function.
    #[allow(unsafe_code, reason = "calls the exported function")]
    fn sorted<T: Copy>(mut values: Vec<T>, element: u32, reverse: bool) -> Vec<T> {
        let count = i64::try_from(values.len()).unwrap_or(0);
        // SAFETY: the pointer and count are the vector's, and `T` is the type `element` names.
        unsafe { baryon_rt_sort(values.as_mut_ptr().cast(), count, element, reverse) };
        values
    }

    #[test]
    fn integers_sort_both_ways() {
        assert_eq!(
            sorted(vec![3_i64, -1, 2, -1], ELEMENT_I64, false),
            [-1, -1, 2, 3]
        );
        assert_eq!(sorted(vec![3_u8, 200, 0], ELEMENT_U8, true), [200, 3, 0]);
        assert_eq!(
            sorted(vec![5_i8, -128, 127], ELEMENT_I8, false),
            [-128, 5, 127]
        );
        assert_eq!(
            sorted(Vec::<i32>::new(), ELEMENT_I32, false),
            Vec::<i32>::new()
        );
    }

    #[test]
    fn floats_put_nan_last_and_keep_equal_zeros_in_order() {
        let ascending = sorted(vec![f64::NAN, 1.0, -0.0, 0.0, -2.5], ELEMENT_F64, false);
        assert_eq!(ascending[..4], [-2.5, -0.0, 0.0, 1.0]);
        assert!(ascending[1].is_sign_negative() && ascending[2].is_sign_positive());
        assert!(ascending[4].is_nan());
        let descending = sorted(vec![1.0_f32, f32::NAN, 3.0], ELEMENT_F32, true);
        assert_eq!(descending[..2], [3.0, 1.0]);
        assert!(descending[2].is_nan());
    }
}
