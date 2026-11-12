//! Splitting a program's functions into parts, so each part can be its own LLVM module and the
//! driver can compile the parts at once, one `clang` per core (roadmap 2.6).
//!
//! `clang` optimizes and generates code one module at a time, on one core. A program written as
//! one module therefore uses one core however many the machine has. Split into four, the
//! 4,706-line benchmark program of roadmap 2.0 compiled in 0.46 s instead of 1.6 s at `-O0`, and
//! 2.7 s instead of 9.6 s at `-O2`, on four cores.
//!
//! Splitting never changes what a function computes: each function's text is the same in any
//! part, and LLVM optimizes one function's arithmetic without looking at which module holds its
//! callers, so float sums keep their order and results keep their bits. What a split can change
//! is inlining across parts: a call to a function in another part is never inlined, which only
//! matters at `-O2`.

/// The smallest part worth a `clang` of its own, in bytes of function text.
///
/// Every part carries the program's constants, declarations and line-table metadata, and every
/// `clang` costs about 20 ms to start, so a small program compiles fastest as one part. 200 KB is
/// about 4,000 lines of LLVM IR, some 50 lines of Baryon.
pub(crate) const MINIMUM_PART: usize = 200_000;

/// Assigns the definitions whose text lengths are `sizes` to at most `parts` parts, each at
/// least [`MINIMUM_PART`] long where there is that much text, and returns each part's definition
/// indices in increasing order.
///
/// Each definition goes whole to one part, largest first, into the part with the least text so
/// far, the lowest-numbered on a tie: the classic greedy split, within a third of the best
/// balance and the same on every run. Empty parts are left out, so there is always at least one
/// part when there is a definition, and none when there are none.
pub(crate) fn assign(sizes: &[usize], parts: usize) -> Vec<Vec<usize>> {
    let total: usize = sizes.iter().sum();
    let count = parts.min(total / MINIMUM_PART).max(1);
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&index| (std::cmp::Reverse(sizes[index]), index));
    let mut assigned = vec![Vec::new(); count];
    let mut loads = vec![0; count];
    for index in order {
        let lightest = (0..count)
            .min_by_key(|&part| (loads[part], part))
            .unwrap_or(0);
        loads[lightest] += sizes[index];
        assigned[lightest].push(index);
    }
    for part in &mut assigned {
        part.sort_unstable();
    }
    assigned.retain(|part| !part.is_empty());
    assigned
}

#[cfg(test)]
mod tests {
    use super::{MINIMUM_PART, assign};

    #[test]
    fn a_small_program_stays_one_part() {
        assert_eq!(assign(&[10, 20, 30], 8), [vec![0, 1, 2]]);
    }

    #[test]
    fn no_definitions_give_no_parts() {
        assert!(assign(&[], 4).is_empty());
    }

    #[test]
    fn large_definitions_spread_evenly_and_keep_their_order() {
        let sizes = [MINIMUM_PART; 8];
        assert_eq!(
            assign(&sizes, 4),
            [vec![0, 4], vec![1, 5], vec![2, 6], vec![3, 7]]
        );
    }

    #[test]
    fn the_largest_definitions_are_placed_first() {
        let sizes = [MINIMUM_PART, 3 * MINIMUM_PART, MINIMUM_PART, MINIMUM_PART];
        assert_eq!(assign(&sizes, 2), [vec![1], vec![0, 2, 3]]);
    }

    #[test]
    fn parts_never_outnumber_what_the_text_fills() {
        let sizes = [MINIMUM_PART; 2];
        assert_eq!(assign(&sizes, 16).len(), 2);
    }
}
