//! Tests of accumulated arrays: lazy blocks, the fixed tree in any finishing order, overflow,
//! and adding the result into strided arrays.

use super::*;

/// Replaces scalar `scalar` of chunk `chunk`'s copy with `change` of it, as compiled code adds
/// in place.
#[allow(unsafe_code, reason = "drives the accumulator as compiled code does")]
fn update<T: Scalar>(
    accumulator: &Accumulator,
    chunk: usize,
    scalar: usize,
    change: impl Fn(T) -> T,
) {
    // SAFETY: the tests run each chunk's adds on this thread before its `chunk_done`.
    let slot = unsafe { accumulator.slot(chunk, scalar) };
    #[allow(clippy::cast_ptr_alignment, reason = "a slot is aligned for its kind")]
    let slot = slot.cast::<T>();
    // SAFETY: the slot holds a `T` of this chunk's copy, which only this thread uses.
    let current = unsafe { slot.read() };
    // SAFETY: as above.
    unsafe { slot.write(change(current)) };
}

/// Adds `value` into scalar `scalar` of chunk `chunk`'s copy of an `f64` accumulator.
fn add(accumulator: &Accumulator, chunk: usize, scalar: usize, value: f64) {
    update(accumulator, chunk, scalar, |current: f64| current + value);
}

/// Finishes `accumulator` into a one-dimensional contiguous `array`.
fn finish(accumulator: Accumulator, array: &mut [f64]) -> Result<(), Overflow> {
    let length = i64::try_from(array.len()).unwrap_or(0);
    // SAFETY: the array is live, contiguous and holds exactly the accumulator's scalars.
    #[allow(unsafe_code, reason = "drives the accumulator as compiled code does")]
    unsafe {
        accumulator.finish(array.as_mut_ptr().cast(), &[length], &[1], 1)
    }
}

/// Marks every chunk done, in `order`.
fn done(accumulator: &Accumulator, order: &[usize]) -> Result<(), Overflow> {
    for &chunk in order {
        // SAFETY: each chunk's adds are finished, and each is marked once.
        #[allow(unsafe_code, reason = "drives the accumulator as compiled code does")]
        unsafe {
            accumulator.chunk_done(chunk)?;
        }
    }
    Ok(())
}

/// Five chunks adding values whose float sum depends on grouping, finished in three orders,
/// give the bits the reference's tree gives, and keep the array's own value first.
#[test]
fn every_finishing_order_gives_the_tree_result() {
    let values = [1e16, 1.0, -1e16, 1.0, 3.0];
    // The tree over five partials: ((p0 + p1) + (p2 + p3)) + p4, then the array's 0.5 first.
    let tree = 0.5 + (((-0.0 + values[0]) + values[1]) + (values[2] + values[3]) + values[4]);
    let mut results = Vec::new();
    for order in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0], [2, 4, 0, 3, 1]] {
        let accumulator = Accumulator::new(5, 3, Kind::F64, None);
        for (chunk, value) in values.iter().enumerate() {
            add(&accumulator, chunk, 1, *value);
        }
        assert_eq!(done(&accumulator, &order), Ok(()));
        let mut array = [7.0, 0.5, -0.0];
        assert_eq!(finish(accumulator, &mut array), Ok(()));
        results.push(array);
    }
    for array in results {
        assert_eq!(array[0].to_bits(), 7.0_f64.to_bits());
        assert_eq!(array[1].to_bits(), tree.to_bits());
        assert_eq!(array[2].to_bits(), (-0.0_f64).to_bits());
    }
}

/// Only touched blocks are made: a chunk that adds into the last scalar of a large array makes
/// one block, and an untouched scalar keeps its value, `-0.0` included.
#[test]
fn only_touched_blocks_are_made() {
    let scalars = 10 * BLOCK_BYTES / 8 + 3;
    let accumulator = Accumulator::new(2, scalars, Kind::F64, None);
    add(&accumulator, 1, scalars - 1, 2.5);
    // SAFETY: chunk 1's adds are finished; nothing else runs.
    #[allow(unsafe_code, reason = "inspects a chunk's copy")]
    let made = unsafe {
        (*accumulator.copies[1].get())
            .as_ref()
            .map(|table| table.iter().flatten().count())
    };
    assert_eq!(made, Some(1));
    assert_eq!(done(&accumulator, &[1, 0]), Ok(()));
    let mut array = vec![-0.0; scalars];
    assert_eq!(finish(accumulator, &mut array), Ok(()));
    assert_eq!(array[scalars - 1].to_bits(), 2.5_f64.to_bits());
    assert!(
        array[..scalars - 1]
            .iter()
            .all(|value| value.to_bits() == (-0.0_f64).to_bits())
    );
}

/// An integer sum that overflows in the tree is reported, and so is one that overflows only
/// when added into the array.
#[test]
fn integer_overflow_is_reported() {
    let accumulator = Accumulator::new(2, 1, Kind::I8, None);
    for chunk in 0..2 {
        update(&accumulator, chunk, 0, |_: i8| 100);
    }
    assert_eq!(done(&accumulator, &[0, 1]), Err(Overflow));

    let accumulator = Accumulator::new(1, 1, Kind::I8, None);
    update(&accumulator, 0, 0, |_: i8| 100);
    assert_eq!(done(&accumulator, &[0]), Ok(()));
    let mut array = [100_i8];
    // SAFETY: a live one-element `i8` array.
    #[allow(unsafe_code, reason = "drives the accumulator as compiled code does")]
    let finished = unsafe { accumulator.finish(array.as_mut_ptr().cast(), &[1], &[1], 1) };
    assert_eq!(finished, Err(Overflow));
}

/// The result lands in a strided view, lane by lane: a 2 × 2 array of three-lane elements
/// whose rows are 3 elements apart.
#[test]
fn the_result_follows_lengths_strides_and_lanes() {
    let accumulator = Accumulator::new(1, 12, Kind::F64, None);
    for scalar in 0..12 {
        add(
            &accumulator,
            0,
            scalar,
            f64::from(u8::try_from(scalar).unwrap_or(0)),
        );
    }
    assert_eq!(done(&accumulator, &[0]), Ok(()));
    let mut memory = [0.0_f64; 18];
    // SAFETY: rows of 2 elements, 3 elements apart, 3 lanes each: 18 scalars.
    #[allow(unsafe_code, reason = "drives the accumulator as compiled code does")]
    let finished = unsafe { accumulator.finish(memory.as_mut_ptr().cast(), &[2, 2], &[3, 1], 3) };
    assert_eq!(finished, Ok(()));
    let expected = [
        0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 0.0, 0.0, //
        6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 0.0, 0.0, 0.0,
    ];
    assert_eq!(memory.map(f64::to_bits), expected.map(f64::to_bits));
}

/// Kind codes follow the documented order, and an unknown code has no kind.
#[test]
fn kind_codes_follow_the_documented_order() {
    assert_eq!(Kind::from_code(0), Some(Kind::I8));
    assert_eq!(Kind::from_code(9), Some(Kind::F64));
    assert_eq!(Kind::from_code(10), None);
    assert_eq!(Kind::from_code(-1), None);
    assert_eq!(Kind::F32.size(), 4);
}

/// A fresh block holds the additive identity in every scalar: `-0.0` in each `f32` and `f64`,
/// so adding it leaves `-0.0` itself unchanged, and `0` in every integer.
#[test]
fn a_fresh_block_holds_the_identity() {
    let mut block = fresh(Kind::F64);
    assert!(
        scalars::<f64>(&mut block)
            .iter()
            .all(|value| value.to_bits() == (-0.0f64).to_bits())
    );
    let mut block = fresh(Kind::F32);
    assert!(
        scalars::<f32>(&mut block)
            .iter()
            .all(|value| value.to_bits() == (-0.0f32).to_bits())
    );
    for kind in [
        Kind::I8,
        Kind::I16,
        Kind::I32,
        Kind::I64,
        Kind::U8,
        Kind::U16,
        Kind::U32,
        Kind::U64,
    ] {
        assert!(fresh(kind).iter().all(|&word| word == 0));
    }
}
