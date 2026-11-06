//! Compile-time shapes: the dimension lengths of a `Fixed` array, part of its type.

use std::fmt;

/// The shape of a `Fixed` array: one length per dimension, outermost first. `(3,)` is a
/// 3-vector and `(3, 3)` a 3 × 3 matrix (decision 0007). The lengths are known when compiling,
/// so the compiler can unroll loops over the array and check its indices before the program
/// runs.
///
/// ```
/// use types::Shape;
///
/// let matrix = Shape::new(vec![3, 3]).unwrap();
/// assert_eq!(matrix.rank(), 2);
/// assert_eq!(matrix.elements(), 9);
/// assert_eq!(matrix.to_string(), "(3, 3)");
/// assert_eq!(Shape::new(vec![3]).unwrap().to_string(), "(3,)");
/// assert!(Shape::new(vec![]).is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shape {
    /// The length of each dimension, outermost first. Never empty; the product never overflows
    /// `u64`.
    lengths: Vec<u64>,
}

impl Shape {
    /// A shape with the given dimension lengths, or `None` if there are no dimensions or the
    /// element count does not fit in a `u64`. A length of zero is allowed: an empty array.
    #[must_use]
    pub fn new(lengths: Vec<u64>) -> Option<Self> {
        if lengths.is_empty() {
            return None;
        }
        lengths
            .iter()
            .try_fold(1_u64, |count, &length| count.checked_mul(length))?;
        Some(Self { lengths })
    }

    /// The number of dimensions.
    #[must_use]
    pub fn rank(&self) -> usize {
        self.lengths.len()
    }

    /// The length of each dimension, outermost first.
    #[must_use]
    pub fn lengths(&self) -> &[u64] {
        &self.lengths
    }

    /// The number of elements: the product of the lengths.
    #[must_use]
    pub fn elements(&self) -> u64 {
        self.lengths.iter().product()
    }
}

impl fmt::Display for Shape {
    /// Writes the shape as a Python tuple: `(3,)`, `(3, 3)`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lengths: Vec<String> = self.lengths.iter().map(u64::to_string).collect();
        if let [single] = lengths.as_slice() {
            write!(formatter, "({single},)")
        } else {
            write!(formatter, "({})", lengths.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_element_count_past_u64_is_rejected() {
        assert!(Shape::new(vec![u64::MAX, 2]).is_none());
        assert!(Shape::new(vec![u64::MAX, 1]).is_some());
    }

    #[test]
    fn a_zero_length_makes_an_empty_array() {
        assert_eq!(Shape::new(vec![4, 0]).unwrap().elements(), 0);
    }
}
