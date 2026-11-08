//! Pairwise summation of floats: the order `a.sum()` adds an array's elements in (roadmap I.22,
//! chosen by Collins on 2026-11-08).
//!
//! Adding `n` floats one at a time lets rounding error grow with `n`. Adding them as a balanced
//! tree of pairs lets it grow only with `log2(n)`, at the same cost. This is `NumPy`'s tree, step
//! for step, so `a.sum()` gives the same bits as `numpy.sum` of the same contiguous array:
//!
//! - Fewer than 8 values are added one at a time, starting from `-0.0`.
//! - Up to 128 values are added into 8 running totals, value `i` into total `i % 8`, which are
//!   then combined as `((t0 + t1) + (t2 + t3)) + ((t4 + t5) + (t6 + t7))`; the last `n % 8`
//!   values are added to that one at a time.
//! - More than 128 values split in two at the multiple of 8 nearest below the middle, and the
//!   two halves' sums are added.
//!
//! The tree's result is then added to `0.0`, `NumPy`'s identity for `+`, so an empty array, or one
//! holding only `-0.0`, sums to `0.0`, as `numpy.sum` gives.

use std::ops::Add;

/// The number of running totals in a block, and the step the split point is a multiple of.
const LANES: usize = 8;

/// The largest run of values added as one block of running totals rather than split in two.
const BLOCK: usize = 128;

/// The sum of `values` in `NumPy`'s pairwise order; `0.0` when there are none.
///
/// `zero` is `0.0` and `negative_zero` is `-0.0` of the float type.
pub(crate) fn sum<T: Copy + Add<Output = T>>(values: &[T], zero: T, negative_zero: T) -> T {
    if values.is_empty() {
        return zero;
    }
    zero + pairwise(values, negative_zero)
}

/// The pairwise sum of a non-empty or empty run, as `NumPy`'s `pairwise_sum` computes it.
fn pairwise<T: Copy + Add<Output = T>>(values: &[T], negative_zero: T) -> T {
    let count = values.len();
    if count < LANES {
        return values
            .iter()
            .fold(negative_zero, |total, &value| total + value);
    }
    if count <= BLOCK {
        let whole = count - count % LANES;
        let mut totals = [negative_zero; LANES];
        totals.copy_from_slice(&values[..LANES]);
        for block in values[LANES..whole].chunks_exact(LANES) {
            for (total, &value) in totals.iter_mut().zip(block) {
                *total = *total + value;
            }
        }
        let [t0, t1, t2, t3, t4, t5, t6, t7] = totals;
        let combined = ((t0 + t1) + (t2 + t3)) + ((t4 + t5) + (t6 + t7));
        return values[whole..]
            .iter()
            .fold(combined, |total, &value| total + value);
    }
    let half = count / 2;
    let split = half - half % LANES;
    pairwise(&values[..split], negative_zero) + pairwise(&values[split..], negative_zero)
}

#[cfg(test)]
mod tests {
    use super::sum;

    /// The sum of `f64` values, as `a.sum()` computes it.
    fn sum_f64(values: &[f64]) -> f64 {
        sum(values, 0.0, -0.0)
    }

    #[test]
    fn small_runs_add_in_order() {
        assert_eq!(sum_f64(&[]).to_bits(), 0.0_f64.to_bits());
        assert!(sum_f64(&[-0.0, -0.0]).is_sign_positive());
        assert_eq!(sum_f64(&[1.0, 2.0, 3.5]).to_bits(), 6.5_f64.to_bits());
    }

    /// The pairwise order keeps a small value that a running total would lose: 1.0 plus a
    /// thousand values of 1e-16 is exactly 1.0 one at a time, since each 1e-16 is below half an
    /// ulp of 1.0, but the tree adds the small values to each other first.
    #[test]
    fn the_tree_keeps_small_values_a_running_total_loses() {
        let mut values = vec![1.0];
        values.extend(std::iter::repeat_n(1e-16, 1000));
        let running = values.iter().fold(0.0_f64, |total, value| total + value);
        assert_eq!(running.to_bits(), 1.0_f64.to_bits());
        assert!(sum_f64(&values) > 1.0 + 9e-14);
    }

    /// `NumPy` 2.5's `numpy.sum` of these arrays, computed in Python and copied here bit for bit.
    /// Adding one value at a time gives different bits for both.
    #[test]
    fn sums_match_numpy() {
        let values: Vec<f64> = (0..1000_u32).map(|i| 1.0 / f64::from(i + 1)).collect();
        assert_eq!(sum_f64(&values).to_bits(), NUMPY_HARMONIC_1000.to_bits());
        let small: Vec<f32> = (0..300_u16).map(|i| 1.0 / f32::from(i + 1)).collect();
        assert_eq!(
            sum(&small, 0.0, -0.0).to_bits(),
            NUMPY_HARMONIC_300_F32.to_bits()
        );
    }

    /// `numpy.sum(1.0 / numpy.arange(1, 1001, dtype=numpy.float64))`.
    const NUMPY_HARMONIC_1000: f64 = f64::from_bits(0x401d_f11f_45f4_e61a);

    /// `numpy.sum(numpy.float32(1.0) / numpy.arange(1, 301, dtype=numpy.float32))`.
    const NUMPY_HARMONIC_300_F32: f32 = f32::from_bits(0x40c9_0b97);
}
