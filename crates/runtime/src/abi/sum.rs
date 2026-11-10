//! The functions compiled code calls for `a.sum()` and `a.dot(b)` of floats (the `sum` module):
//! one per float width, each taking the address of the first element of each array and the
//! element count, and `strided` forms that also take strides.
//!
//! A `Fixed` array is one value, so its elements are one run whatever the rank. A new `Array`
//! keeps its elements contiguous in row-major order (decision 0008), but a view of one, a
//! slice, may skip elements or walk backwards, so an `Array` is reduced through its lengths and
//! strides by the `strided` functions. Integer sums never call
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

/// The dot product of the `count` `f64` values at `left` and at `right`: the products added in
/// `NumPy`'s pairwise order, so the result has the same bits as `numpy.sum(left * right)`. Zero
/// values give `0.0`.
///
/// # Safety
///
/// `left` and `right` each point to `count` initialized `f64` values that nothing writes during
/// the call, or `count` is 0 or less.
///
/// # Example
///
/// ```
/// let (left, right) = ([1.0_f64, 2.0, 3.0], [4.0_f64, 5.0, 6.0]);
/// // SAFETY: the pointers and count are two arrays'.
/// let product = unsafe { runtime::baryon_rt_dot_f64(left.as_ptr(), right.as_ptr(), 3) };
/// assert_eq!(product, 32.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dot_f64(left: *const f64, right: *const f64, count: i64) -> f64 {
    // SAFETY: the caller guarantees `count` initialized values at `left`.
    let left = unsafe { elements(left, count) };
    // SAFETY: and at `right`.
    let right = unsafe { elements(right, count) };
    sum::dot(left, right, 0.0, -0.0)
}

/// The dot product of the `count` `f32` values at `left` and at `right`, in `f32` arithmetic and
/// `NumPy`'s pairwise order, as [`baryon_rt_dot_f64`] computes it for `f64`.
///
/// # Safety
///
/// `left` and `right` each point to `count` initialized `f32` values that nothing writes during
/// the call, or `count` is 0 or less.
///
/// # Example
///
/// ```
/// let (left, right) = ([1.5_f32, 2.0], [2.0_f32, 0.25]);
/// // SAFETY: the pointers and count are two arrays'.
/// assert_eq!(unsafe { runtime::baryon_rt_dot_f32(left.as_ptr(), right.as_ptr(), 2) }, 3.5);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dot_f32(left: *const f32, right: *const f32, count: i64) -> f32 {
    // SAFETY: the caller guarantees `count` initialized values at `left`.
    let left = unsafe { elements(left, count) };
    // SAFETY: and at `right`.
    let right = unsafe { elements(right, count) };
    sum::dot(left, right, 0.0, -0.0)
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

/// The pairwise sum of the `f64` elements of an `Array` of `rank` dimensions that may be a view
/// with any strides: the elements are taken in row-major order as one run, as
/// [`baryon_rt_sum_f64`] takes a contiguous array's, so a view sums to the same bits as a
/// contiguous copy of it. `data` is the header's element pointer, `lengths` and `strides` its
/// `rank` lengths and strides in elements.
///
/// Cost: a contiguous array is summed where it lies; any other is first copied into a buffer
/// of its elements, one allocation and one pass.
///
/// # Safety
///
/// `lengths` and `strides` point to `rank` values each, and every element they reach from
/// `data` is an initialized `f64` that nothing writes during the call.
///
/// # Example
///
/// ```
/// // The view `values[::2]` of five values: 1.0 + 3.0 + 5.0.
/// let values = [1.0_f64, 2.0, 3.0, 4.0, 5.0];
/// let (lengths, strides) = ([3_i64], [2_i64]);
/// // SAFETY: the lengths and strides reach only the five values.
/// let total = unsafe {
///     runtime::baryon_rt_sum_strided_f64(values.as_ptr(), 1, lengths.as_ptr(), strides.as_ptr())
/// };
/// assert_eq!(total, 9.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_sum_strided_f64(
    data: *const f64,
    rank: u32,
    lengths: *const i64,
    strides: *const i64,
) -> f64 {
    // SAFETY: the caller's promise, passed on.
    unsafe {
        strided(data, rank, lengths, strides, |values| {
            sum::sum(values, 0.0, -0.0)
        })
    }
}

/// The pairwise sum of the `f32` elements of an `Array` that may be a view, in `f32`
/// arithmetic, as [`baryon_rt_sum_strided_f64`] sums `f64` ones.
///
/// # Safety
///
/// As for [`baryon_rt_sum_strided_f64`], with `f32` elements.
///
/// # Example
///
/// ```
/// // The view `values[::-1]` of two values.
/// let values = [1.5_f32, 2.5];
/// let (lengths, strides) = ([2_i64], [-1_i64]);
/// // SAFETY: from the last value back, the lengths and strides reach only the two values.
/// let total = unsafe {
///     runtime::baryon_rt_sum_strided_f32(values.as_ptr().add(1), 1, lengths.as_ptr(), strides.as_ptr())
/// };
/// assert_eq!(total, 4.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_sum_strided_f32(
    data: *const f32,
    rank: u32,
    lengths: *const i64,
    strides: *const i64,
) -> f32 {
    // SAFETY: the caller's promise, passed on.
    unsafe {
        strided(data, rank, lengths, strides, |values| {
            sum::sum(values, 0.0, -0.0)
        })
    }
}

/// The dot product of two rank-1 `f64` arrays of `count` elements that may be views, each
/// given by its element pointer and its stride in elements: the products added in `NumPy`'s
/// pairwise order, as [`baryon_rt_dot_f64`] adds a contiguous pair's, so a view gives the same
/// bits as a contiguous copy of it.
///
/// Cost: a stride of 1 is read where it lies; any other is first copied into a buffer of its
/// elements, one allocation and one pass per such operand.
///
/// # Safety
///
/// `count` elements from each pointer, each `stride` elements after the last, are initialized
/// `f64` values that nothing writes during the call, or `count` is 0 or less.
///
/// # Example
///
/// ```
/// // `values[::2]` dotted with `values[2::-1]`: 1*3 + 3*2 + 5*1.
/// let values = [1.0_f64, 2.0, 3.0, 4.0, 5.0];
/// // SAFETY: both views reach only the five values.
/// let product = unsafe {
///     runtime::baryon_rt_dot_strided_f64(values.as_ptr(), 2, values.as_ptr().add(2), -1, 3)
/// };
/// assert_eq!(product, 14.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dot_strided_f64(
    left: *const f64,
    left_stride: i64,
    right: *const f64,
    right_stride: i64,
    count: i64,
) -> f64 {
    // SAFETY: the caller's promise, passed on for each operand.
    unsafe {
        strided(left, 1, &raw const count, &raw const left_stride, |left| {
            strided(
                right,
                1,
                &raw const count,
                &raw const right_stride,
                |right| sum::dot(left, right, 0.0, -0.0),
            )
        })
    }
}

/// The dot product of two rank-1 `f32` arrays that may be views, in `f32` arithmetic, as
/// [`baryon_rt_dot_strided_f64`] computes it for `f64`.
///
/// # Safety
///
/// As for [`baryon_rt_dot_strided_f64`], with `f32` elements.
///
/// # Example
///
/// ```
/// // `values[::-1]` dotted with `values`: 2.0*1.5 + 1.5*2.0.
/// let values = [1.5_f32, 2.0];
/// // SAFETY: both views reach only the two values.
/// let product = unsafe {
///     runtime::baryon_rt_dot_strided_f32(values.as_ptr().add(1), -1, values.as_ptr(), 1, 2)
/// };
/// assert_eq!(product, 6.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dot_strided_f32(
    left: *const f32,
    left_stride: i64,
    right: *const f32,
    right_stride: i64,
    count: i64,
) -> f32 {
    // SAFETY: the caller's promise, passed on for each operand.
    unsafe {
        strided(left, 1, &raw const count, &raw const left_stride, |left| {
            strided(
                right,
                1,
                &raw const count,
                &raw const right_stride,
                |right| sum::dot(left, right, 0.0, -0.0),
            )
        })
    }
}

/// Calls `reduce` with the elements of the array at `data` with these lengths and strides, in
/// row-major order: in place when the strides are a contiguous array's, otherwise copied into a
/// buffer first.
///
/// # Safety
///
/// `lengths` and `strides` point to `rank` values each, and every element they reach from
/// `data` is initialized and nothing writes it during the call.
#[allow(unsafe_code, reason = "reads an array compiled code points to")]
unsafe fn strided<T: Copy, R>(
    data: *const T,
    rank: u32,
    lengths: *const i64,
    strides: *const i64,
    reduce: impl FnOnce(&[T]) -> R,
) -> R {
    let rank = rank as usize;
    // SAFETY: the caller guarantees `rank` lengths.
    let lengths = unsafe { std::slice::from_raw_parts(lengths, rank) };
    // SAFETY: the caller guarantees `rank` strides.
    let strides = unsafe { std::slice::from_raw_parts(strides, rank) };
    if lengths.iter().any(|&length| length <= 0) {
        return reduce(&[]);
    }
    let count: i64 = lengths.iter().product();
    // Row-major and contiguous: each stride is the product of the lengths after it.
    let mut expected = 1;
    let mut contiguous = true;
    for (&length, &stride) in lengths.iter().zip(strides).rev() {
        contiguous &= stride == expected;
        expected *= length;
    }
    if contiguous {
        // SAFETY: the caller guarantees the `count` elements from `data`.
        return reduce(unsafe { elements(data, count) });
    }
    let mut values = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    let mut index = vec![0_i64; rank];
    loop {
        let offset: i64 = index
            .iter()
            .zip(strides)
            .map(|(i, stride)| i * stride)
            .sum();
        let offset = isize::try_from(offset).unwrap_or(isize::MAX);
        // SAFETY: the index is below the lengths, so the caller guarantees the element there.
        let element = unsafe { data.offset(offset) };
        // SAFETY: as above, the element is initialized and nothing writes it.
        values.push(unsafe { *element });
        // The next index in row-major order: the last dimension fastest.
        let mut dimension = rank;
        loop {
            if dimension == 0 {
                return reduce(&values);
            }
            dimension -= 1;
            index[dimension] += 1;
            if index[dimension] < lengths[dimension] {
                break;
            }
            index[dimension] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A column of a 3 by 2 matrix, and the matrix reversed in both dimensions, are gathered in
    /// row-major order; an empty view sums to 0.
    #[test]
    #[allow(
        unsafe_code,
        reason = "calls the exported functions as compiled code does"
    )]
    fn views_sum_in_row_major_order() {
        let matrix = [1.0_f64, 2.0, 3.0, 4.0, 5.0, 6.0];
        // SAFETY: each view reaches only the six values.
        let column = unsafe {
            baryon_rt_sum_strided_f64(&raw const matrix[1], 1, [3].as_ptr(), [2].as_ptr())
        };
        assert_eq!(column.to_bits(), 12.0_f64.to_bits());
        // SAFETY: from the last value back, the view reaches only the six values.
        let reversed = unsafe {
            baryon_rt_sum_strided_f64(&raw const matrix[5], 2, [3, 2].as_ptr(), [-2, -1].as_ptr())
        };
        assert_eq!(reversed.to_bits(), 21.0_f64.to_bits());
        // SAFETY: an empty view reads nothing.
        let empty = unsafe {
            baryon_rt_sum_strided_f64(matrix.as_ptr(), 2, [0, 2].as_ptr(), [2, 1].as_ptr())
        };
        assert_eq!(empty.to_bits(), 0.0_f64.to_bits());
    }

    /// A strided dot product equals the contiguous one of the same elements, bit for bit, and
    /// an empty one is 0.
    #[test]
    #[allow(
        unsafe_code,
        reason = "calls the exported functions as compiled code does"
    )]
    fn strided_dot_products_match_contiguous_ones() {
        let values = [0.1_f64, 0.7, 0.2, 0.9, 0.3];
        let (left, right) = ([0.1_f64, 0.2, 0.3], [0.3_f64, 0.9, 0.2]);
        // SAFETY: the arrays hold three values each.
        let contiguous = unsafe { baryon_rt_dot_f64(left.as_ptr(), right.as_ptr(), 3) };
        // `values[::2]` is `[0.1, 0.2, 0.3]` and `values[4:1:-1]` is `[0.3, 0.9, 0.2]`.
        // SAFETY: both views reach only the five values.
        let strided =
            unsafe { baryon_rt_dot_strided_f64(values.as_ptr(), 2, &raw const values[4], -1, 3) };
        assert_eq!(strided.to_bits(), contiguous.to_bits());
        // SAFETY: an empty view reads nothing.
        let empty = unsafe { baryon_rt_dot_strided_f64(values.as_ptr(), 2, values.as_ptr(), 1, 0) };
        assert_eq!(empty.to_bits(), 0.0_f64.to_bits());
    }
}
