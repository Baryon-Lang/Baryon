//! `@` of two matrices through the system BLAS, for a program built with `--blas` (decision
//! 0040, roadmap 3.10.2).
//!
//! The runtime never names a BLAS function. Compiled code passes the address of
//! `cblas_dgemm` or `cblas_sgemm` as the first argument, so only a program built with
//! `--blas` refers to the BLAS library and is linked with it; every other program, and the
//! runtime archive itself, builds and runs without one. The runtime's part is to describe each
//! matrix as BLAS wants it: a matrix whose rows are contiguous goes as it lies, with its row
//! stride as BLAS's leading dimension; a transposed one, whose columns are contiguous, goes as
//! it lies too, marked transposed; any other view is first copied into a contiguous buffer.
//! The result is the array `@` just made, contiguous, and BLAS writes every element of it.
//!
//! BLAS takes dimensions as C `int`s. A matrix with a dimension or a stride past 2,147,483,647
//! elements panics instead of being passed cut short.

use crate::panic::{Failure, panic};

/// `CblasRowMajor`: the matrices are stored row by row.
const ROW_MAJOR: i32 = 101;
/// `CblasNoTrans`: use the matrix as it is stored.
const NO_TRANSPOSE: i32 = 111;
/// `CblasTrans`: use the transpose of the matrix as it is stored.
const TRANSPOSE: i32 = 112;

/// The signature of `cblas_dgemm` and `cblas_sgemm` for an element type `T`: order, the two
/// transpose flags, the dimensions `m`, `n` and `k`, `alpha`, `a` with its leading dimension,
/// `b` with its leading dimension, `beta`, and `c` with its leading dimension. It computes
/// `c = alpha a b + beta c`.
pub type Gemm<T> = unsafe extern "C" fn(
    i32,
    i32,
    i32,
    i32,
    i32,
    i32,
    T,
    *const T,
    i32,
    *const T,
    i32,
    T,
    *mut T,
    i32,
);

/// One operand of a matrix product as BLAS takes it: where its first element is, whether it
/// is transposed, and its leading dimension; and the buffer that holds a copy of it, when it
/// had to be copied, which must outlive the call.
struct Operand<T> {
    /// The first element.
    data: *const T,
    /// [`NO_TRANSPOSE`] or [`TRANSPOSE`].
    transpose: i32,
    /// BLAS's leading dimension: the stride between rows as stored, at least 1.
    leading: i32,
    /// The contiguous copy `data` points into, for a view BLAS cannot read where it lies.
    _copy: Vec<T>,
}

/// An offset in elements as a pointer offset. Every offset reaches an element of an array in
/// memory, so it fits; the saturation only keeps the conversion total.
fn offset(elements: i64) -> isize {
    isize::try_from(elements).unwrap_or(isize::MAX)
}

/// A dimension or a stride as a C `int`, or a panic when it does not fit.
fn int(value: i64) -> i32 {
    i32::try_from(value).unwrap_or_else(|_| {
        panic(
            Failure::Message("a matrix too large for BLAS: a dimension past 2147483647"),
            None,
        )
    })
}

/// `matrix`, of `rows` × `columns` elements with the strides `strides` in elements, as BLAS
/// takes it.
///
/// # Safety
///
/// Every element the lengths and strides reach from `data` is an initialized `T` that nothing
/// writes during the product.
#[allow(unsafe_code, reason = "reads a matrix compiled code points to")]
unsafe fn operand<T: Copy>(
    data: *const T,
    rows: i64,
    columns: i64,
    strides: [i64; 2],
) -> Operand<T> {
    let [row_stride, column_stride] = strides;
    if column_stride == 1 && row_stride >= columns.max(1) {
        return Operand {
            data,
            transpose: NO_TRANSPOSE,
            leading: int(row_stride),
            _copy: Vec::new(),
        };
    }
    if row_stride == 1 && column_stride >= rows.max(1) {
        return Operand {
            data,
            transpose: TRANSPOSE,
            leading: int(column_stride),
            _copy: Vec::new(),
        };
    }
    let mut copy = Vec::with_capacity(usize::try_from(rows * columns).unwrap_or(0));
    for row in 0..rows {
        for column in 0..columns {
            let offset = offset(row * row_stride + column * column_stride);
            // SAFETY: the caller guarantees every element the strides reach.
            let element = unsafe { data.offset(offset) };
            // SAFETY: as above, the element is initialized and nothing writes it.
            copy.push(unsafe { *element });
        }
    }
    Operand {
        data: copy.as_ptr(),
        transpose: NO_TRANSPOSE,
        leading: int(columns.max(1)),
        _copy: copy,
    }
}

/// `result = left @ right` through `gemm`, for either float type: the work of
/// [`baryon_rt_matrix_product_f64`] and [`baryon_rt_matrix_product_f32`].
///
/// # Safety
///
/// As for [`baryon_rt_matrix_product_f64`], with elements of type `T`.
#[allow(
    unsafe_code,
    reason = "reads and writes matrices compiled code points to"
)]
unsafe fn product<T: Copy + Default>(
    gemm: Gemm<T>,
    one: T,
    left: (*const T, *const i64, *const i64),
    right: (*const T, *const i64, *const i64),
    result: *mut T,
    result_lengths: *const i64,
    result_strides: *const i64,
) {
    // SAFETY: the caller guarantees two lengths and two strides for each matrix.
    let pair = |values: *const i64| unsafe { std::ptr::read(values.cast::<[i64; 2]>()) };
    let ([rows, inner], [_, columns]) = (pair(left.1), pair(right.1));
    let ([result_rows, result_columns], result_strides) =
        (pair(result_lengths), pair(result_strides));
    assert!(
        result_rows == rows && result_columns == columns && result_strides[1] == 1,
        "the result of a BLAS matrix product is a new contiguous array of its shape"
    );
    if rows <= 0 || columns <= 0 {
        return;
    }
    if inner <= 0 {
        for row in 0..rows {
            for column in 0..columns {
                // SAFETY: the result's `rows` × `columns` elements are the caller's to write.
                let element = unsafe { result.offset(offset(row * result_strides[0] + column)) };
                // SAFETY: as above.
                unsafe { element.write(T::default()) };
            }
        }
        return;
    }
    // SAFETY: the caller guarantees every element of `left`.
    let a = unsafe { operand(left.0, rows, inner, pair(left.2)) };
    // SAFETY: the caller guarantees every element of `right`.
    let b = unsafe { operand(right.0, inner, columns, pair(right.2)) };
    // SAFETY: `gemm` is a BLAS `gemm` for `T`, the caller's promise; `a` and `b` describe
    // `rows` × `inner` and `inner` × `columns` matrices it may read, and `result` the
    // `rows` × `columns` one it writes, with `beta` zero so its old contents are never read.
    unsafe {
        gemm(
            ROW_MAJOR,
            a.transpose,
            b.transpose,
            int(rows),
            int(columns),
            int(inner),
            one,
            a.data,
            a.leading,
            b.data,
            b.leading,
            T::default(),
            result,
            int(result_strides[0].max(1)),
        );
    }
}

/// `result = left @ right` for two `f64` matrices, through `gemm`, which is `cblas_dgemm` of
/// the BLAS the program was linked with. Each matrix is given by its element pointer and the
/// addresses of its header's two lengths and two strides, in elements: `left` is `m` × `k`,
/// `right` `k` × `n`, and `result` the new `m` × `n` array the product goes into. Lowering has
/// checked the inner lengths.
///
/// Cost: BLAS's, about `2mkn` operations on as many threads as the BLAS uses; a view whose rows
/// and columns are both strided is first copied, one allocation and one pass.
///
/// # Safety
///
/// `gemm` is `cblas_dgemm`. Each lengths and strides pointer points to two values. Every
/// element they reach from `left` and `right` is an initialized `f64` that nothing writes
/// during the call, and `result` is a contiguous row-major array of `m` × `n` elements that
/// nothing else reads or writes during it.
///
/// # Example
///
/// ```
/// # #[allow(clippy::too_many_arguments)]
/// // A stand-in `gemm` for row-major matrices that are not transposed, `beta` zero.
/// unsafe extern "C" fn gemm(
///     _: i32, _: i32, _: i32, m: i32, n: i32, k: i32, alpha: f64,
///     a: *const f64, lda: i32, b: *const f64, ldb: i32, _: f64, c: *mut f64, ldc: i32,
/// ) {
///     for i in 0..m as usize {
///         for j in 0..n as usize {
///             let mut total = 0.0;
///             for p in 0..k as usize {
///                 // SAFETY: the matrices hold these elements.
///                 total += unsafe { *a.add(i * lda as usize + p) * *b.add(p * ldb as usize + j) };
///             }
///             // SAFETY: as above.
///             unsafe { *c.add(i * ldc as usize + j) = alpha * total };
///         }
///     }
/// }
/// let (left, right) = ([1.0_f64, 2.0, 3.0, 4.0], [5.0_f64, 6.0, 7.0, 8.0]);
/// let mut result = [0.0_f64; 4];
/// let (lengths, strides) = ([2_i64, 2], [2_i64, 1]);
/// let (l, s) = (lengths.as_ptr(), strides.as_ptr());
/// // SAFETY: each matrix holds 2 × 2 elements in rows.
/// unsafe {
///     runtime::baryon_rt_matrix_product_f64(
///         gemm, left.as_ptr(), l, s, right.as_ptr(), l, s, result.as_mut_ptr(), l, s,
///     );
/// }
/// assert_eq!(result, [19.0, 22.0, 43.0, 50.0]);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_matrix_product_f64(
    gemm: Gemm<f64>,
    left: *const f64,
    left_lengths: *const i64,
    left_strides: *const i64,
    right: *const f64,
    right_lengths: *const i64,
    right_strides: *const i64,
    result: *mut f64,
    result_lengths: *const i64,
    result_strides: *const i64,
) {
    // SAFETY: the caller's promise, passed on.
    unsafe {
        product(
            gemm,
            1.0,
            (left, left_lengths, left_strides),
            (right, right_lengths, right_strides),
            result,
            result_lengths,
            result_strides,
        );
    }
}

/// `result = left @ right` for two `f32` matrices, through `gemm`, which is `cblas_sgemm`, as
/// [`baryon_rt_matrix_product_f64`] multiplies `f64` ones.
///
/// # Safety
///
/// As for [`baryon_rt_matrix_product_f64`], with `f32` elements and `cblas_sgemm`.
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_matrix_product_f32(
    gemm: Gemm<f32>,
    left: *const f32,
    left_lengths: *const i64,
    left_strides: *const i64,
    right: *const f32,
    right_lengths: *const i64,
    right_strides: *const i64,
    result: *mut f32,
    result_lengths: *const i64,
    result_strides: *const i64,
) {
    // SAFETY: the caller's promise, passed on.
    unsafe {
        product(
            gemm,
            1.0,
            (left, left_lengths, left_strides),
            (right, right_lengths, right_strides),
            result,
            result_lengths,
            result_strides,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for `cblas_dgemm` with `beta` zero: the textbook product of row-major
    /// matrices, each read transposed when its flag says so.
    #[allow(unsafe_code, reason = "stands in for the C function")]
    unsafe extern "C" fn gemm(
        order: i32,
        transpose_a: i32,
        transpose_b: i32,
        rows: i32,
        columns: i32,
        inner: i32,
        alpha: f64,
        a: *const f64,
        lda: i32,
        b: *const f64,
        ldb: i32,
        _: f64,
        c: *mut f64,
        ldc: i32,
    ) {
        assert_eq!(order, ROW_MAJOR);
        let at = |matrix: *const f64, leading: i32, transposed: bool, row: i32, column: i32| {
            let (row, column) = if transposed {
                (column, row)
            } else {
                (row, column)
            };
            // SAFETY: the test's matrices hold every element BLAS may read.
            unsafe {
                matrix
                    .wrapping_add(usize::try_from(row * leading + column).unwrap_or(0))
                    .read()
            }
        };
        for i in 0..rows {
            for j in 0..columns {
                let total: f64 = (0..inner)
                    .map(|p| {
                        at(a, lda, transpose_a == TRANSPOSE, i, p)
                            * at(b, ldb, transpose_b == TRANSPOSE, p, j)
                    })
                    .sum();
                let index = usize::try_from(i * ldc + j).unwrap_or(0);
                // SAFETY: the result holds `rows` × `columns` elements.
                unsafe { c.wrapping_add(index).write(alpha * total) };
            }
        }
    }

    /// `left @ right` through the stand-in, with each matrix given by its first element,
    /// lengths and strides.
    #[allow(
        unsafe_code,
        reason = "calls the exported function as compiled code does"
    )]
    fn multiply(
        left: (&[f64], [i64; 2], [i64; 2]),
        right: (&[f64], [i64; 2], [i64; 2]),
    ) -> Vec<f64> {
        let shape = [left.1[0], right.1[1]];
        let mut result = vec![-1.0; usize::try_from(shape[0] * shape[1]).unwrap_or(0)];
        let strides = [shape[1], 1];
        // SAFETY: the tests' lengths and strides reach only their slices' elements.
        unsafe {
            baryon_rt_matrix_product_f64(
                gemm,
                left.0.as_ptr(),
                left.1.as_ptr(),
                left.2.as_ptr(),
                right.0.as_ptr(),
                right.1.as_ptr(),
                right.2.as_ptr(),
                result.as_mut_ptr(),
                shape.as_ptr(),
                strides.as_ptr(),
            );
        }
        result
    }

    /// A contiguous matrix, its transpose read in place, and a view strided in both
    /// dimensions, which is copied first, all give the same product.
    #[test]
    fn contiguous_transposed_and_strided_matrices_multiply_alike() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = [7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let expected = [58.0, 64.0, 139.0, 154.0];
        assert_eq!(
            multiply((&a, [2, 3], [3, 1]), (&b, [3, 2], [2, 1])),
            expected
        );
        // `a` stored as its transpose, 3 × 2, read as a 2 × 3 matrix with strides (1, 2).
        let a_transposed = [1.0, 4.0, 2.0, 5.0, 3.0, 6.0];
        assert_eq!(
            multiply((&a_transposed, [2, 3], [1, 2]), (&b, [3, 2], [2, 1])),
            expected
        );
        // `b` as every other element of a 3 × 4 matrix, strides (4, 2).
        let wide = [
            7.0, 0.0, 8.0, 0.0, 9.0, 0.0, 10.0, 0.0, 11.0, 0.0, 12.0, 0.0,
        ];
        assert_eq!(
            multiply((&a, [2, 3], [3, 1]), (&wide, [3, 2], [4, 2])),
            expected
        );
    }

    /// An inner length of zero gives zeros, as `NumPy`'s `@` does, without calling BLAS, and an
    /// empty result is left alone.
    #[test]
    fn empty_products_need_no_blas() {
        assert_eq!(
            multiply((&[], [2, 0], [0, 1]), (&[], [0, 2], [2, 1])),
            [0.0; 4]
        );
        assert!(multiply((&[], [0, 3], [3, 1]), (&[1.0; 6], [3, 2], [2, 1])).is_empty());
    }
}
