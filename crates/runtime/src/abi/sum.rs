//! The functions compiled code calls for `a.sum()` of floats (the `sum` module): one per float
//! width, each taking the address of the first element and the element count.
//!
//! An `Array` keeps its elements contiguous in row-major order (decision 0008), and a `Fixed`
//! array is one value, so the elements are one run whatever the rank. Integer sums never call
//! these: integer addition is exact, so compiled code adds in a loop.

use crate::sum;

/// The pairwise sum of the `count` `f64` values at `values`, in `NumPy`'s order, so the result has
/// the same bits as `numpy.sum`. Zero values sum to `0.0`.
///
/// # Safety
///
/// `values` points to `count` initialized `f64` values that nothing writes during the call, or
/// `count` is 0 or less.
///
/// # Example
///
/// ```
/// let values = [0.1_f64, 0.2, 0.3];
/// // SAFETY: the pointer and count are an array's.
/// let total = unsafe { runtime::baryon_rt_sum_f64(values.as_ptr(), 3) };
/// assert_eq!(total, 0.1 + 0.2 + 0.3);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_sum_f64(values: *const f64, count: i64) -> f64 {
    // SAFETY: the caller guarantees `count` initialized values at `values`.
    let values = unsafe { elements(values, count) };
    sum::sum(values, 0.0, -0.0)
}

/// The pairwise sum of the `count` `f32` values at `values`, in `NumPy`'s order and in `f32`
/// arithmetic, as `numpy.sum` of a `float32` array adds them. Zero values sum to `0.0`.
///
/// # Safety
///
/// `values` points to `count` initialized `f32` values that nothing writes during the call, or
/// `count` is 0 or less.
///
/// # Example
///
/// ```
/// let values = [1.5_f32, 2.5];
/// // SAFETY: the pointer and count are an array's.
/// assert_eq!(unsafe { runtime::baryon_rt_sum_f32(values.as_ptr(), 2) }, 4.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_sum_f32(values: *const f32, count: i64) -> f32 {
    // SAFETY: the caller guarantees `count` initialized values at `values`.
    let values = unsafe { elements(values, count) };
    sum::sum(values, 0.0, -0.0)
}

/// The `count` values at `values` as a slice; empty when `count` is 0 or less.
///
/// # Safety
///
/// When `count` is above 0, `values` points to `count` initialized values that nothing writes
/// while the slice lives.
#[allow(unsafe_code, reason = "reads an array compiled code points to")]
unsafe fn elements<'a, T>(values: *const T, count: i64) -> &'a [T] {
    match usize::try_from(count) {
        Ok(count) if count > 0 => {
            // SAFETY: the caller guarantees `count` initialized, unaliased values.
            unsafe { std::slice::from_raw_parts(values, count) }
        }
        _ => &[],
    }
}
