//! The functions compiled code calls to sum a whole-array expression without storing it
//! (decision 0041): `(a * b).sum()` is computed one leaf of `NumPy`'s pairwise tree at a time.
//!
//! A leaf is a run of at most 128 values that the tree adds as one block. Compiled code asks
//! [`baryon_rt_pairwise_leaves`] how many leaves the tree of `count` values has, computes each
//! leaf's values into a buffer, ending each where [`baryon_rt_pairwise_leaf_end`] says, sums the
//! buffer with `baryon_rt_sum_f64`, and passes the leaf sums to
//! [`baryon_rt_pairwise_combine_f64`], which adds them in the tree's order. The result has the
//! bits of summing the stored values.

use super::sum::elements;
use crate::sum;

/// The number of leaves in the pairwise tree of `count` values: 1 for up to 128 values, even
/// none, and about one per 64 to 128 values beyond.
///
/// # Example
///
/// ```
/// assert_eq!(runtime::baryon_rt_pairwise_leaves(100), 1);
/// assert_eq!(runtime::baryon_rt_pairwise_leaves(300), 4);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_pairwise_leaves(count: i64) -> i64 {
    let count = usize::try_from(count).unwrap_or(0);
    i64::try_from(sum::leaves(count)).unwrap_or(i64::MAX)
}

/// The end of the leaf of the pairwise tree of `count` values that starts at `start`, 0 or the
/// end of the leaf before: the index of the first value of the next leaf, or `count` for the
/// last. A negative `start` counts as 0.
///
/// # Example
///
/// ```
/// // 300 values split at 144, then 144 splits at 72 and 156 at 72.
/// assert_eq!(runtime::baryon_rt_pairwise_leaf_end(300, 0), 72);
/// assert_eq!(runtime::baryon_rt_pairwise_leaf_end(300, 72), 144);
/// assert_eq!(runtime::baryon_rt_pairwise_leaf_end(300, 144), 216);
/// assert_eq!(runtime::baryon_rt_pairwise_leaf_end(300, 216), 300);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_pairwise_leaf_end(count: i64, start: i64) -> i64 {
    let count = usize::try_from(count).unwrap_or(0);
    let start = usize::try_from(start).unwrap_or(0);
    i64::try_from(sum::leaf_end(count, start)).unwrap_or(i64::MAX)
}

/// The pairwise sum of `count` `f64` values from the `leaves` sums of its leaves, in order: the
/// same bits as `baryon_rt_sum_f64` of the values. Zero values sum to `0.0`.
///
/// # Safety
///
/// `sums` points to `leaves` initialized `f64` values that nothing writes during the call, or
/// `leaves` is 0 or less.
///
/// # Example
///
/// ```
/// let values: Vec<f64> = (1..=300).map(|i| 1.0 / f64::from(i)).collect();
/// let mut sums = Vec::new();
/// let mut start = 0;
/// while start < 300 {
///     let end = runtime::baryon_rt_pairwise_leaf_end(300, start);
///     let leaf = &values[start as usize..end as usize];
///     // SAFETY: the pointer and count are a slice's.
///     sums.push(unsafe { runtime::baryon_rt_sum_f64(leaf.as_ptr(), end - start) });
///     start = end;
/// }
/// // SAFETY: the pointers and counts are vectors'.
/// let (combined, whole) = unsafe {
///     (
///         runtime::baryon_rt_pairwise_combine_f64(300, sums.as_ptr(), 4),
///         runtime::baryon_rt_sum_f64(values.as_ptr(), 300),
///     )
/// };
/// assert_eq!(combined.to_bits(), whole.to_bits());
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_pairwise_combine_f64(
    count: i64,
    sums: *const f64,
    leaves: i64,
) -> f64 {
    // SAFETY: the caller guarantees `leaves` initialized values at `sums`.
    let sums = unsafe { elements(sums, leaves) };
    sum::combine(usize::try_from(count).unwrap_or(0), sums, 0.0)
}

/// The `f32` form of [`baryon_rt_pairwise_combine_f64`], adding in `f32` arithmetic as
/// `baryon_rt_sum_f32` does.
///
/// # Safety
///
/// `sums` points to `leaves` initialized `f32` values that nothing writes during the call, or
/// `leaves` is 0 or less.
///
/// # Example
///
/// ```
/// let sums = [1.5_f32];
/// // SAFETY: the pointer and count are an array's.
/// assert_eq!(unsafe { runtime::baryon_rt_pairwise_combine_f32(3, sums.as_ptr(), 1) }, 1.5);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_pairwise_combine_f32(
    count: i64,
    sums: *const f32,
    leaves: i64,
) -> f32 {
    // SAFETY: the caller guarantees `leaves` initialized values at `sums`.
    let sums = unsafe { elements(sums, leaves) };
    sum::combine(usize::try_from(count).unwrap_or(0), sums, 0.0)
}
