//! The positions a subscript of a dataset picks, `dset[1, ::2, 1:3]`, as h5py picks them, and
//! the errors h5py raises for one that picks nothing.
//!
//! An integer picks one position and drops its axis, counting from the end when negative. A
//! slice picks the positions Python's slices pick, its bounds held to the axis, and keeps its
//! axis. Unlike `NumPy`, h5py takes no negative step. The axes after the last item are taken
//! whole.

use crate::files::netcdf::{Item, SubscriptFailure};

/// The positions picked along one axis, in increasing order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Axis {
    /// The positions.
    pub(crate) positions: Vec<u64>,
    /// Whether the axis stays in the result: false for an integer index.
    pub(crate) kept: bool,
}

/// The positions `items` pick of a dataset of `shape`, axis by axis.
///
/// # Errors
///
/// As h5py: more items than axes or a step that is not positive raise `ValueError`, and an
/// index past its axis `IndexError`, with h5py's messages.
pub(crate) fn select(items: &[Item], shape: &[u64]) -> Result<Vec<Axis>, SubscriptFailure> {
    if items.len() > shape.len() {
        return Err(SubscriptFailure::Value(format!(
            "{} indexing arguments for {} dimensions",
            items.len(),
            shape.len()
        )));
    }
    let mut axes = Vec::with_capacity(shape.len());
    for (axis, &length) in shape.iter().enumerate() {
        let signed = i64::try_from(length).unwrap_or(i64::MAX);
        let picked = match items.get(axis) {
            None => Axis {
                positions: (0..length).collect(),
                kept: true,
            },
            Some(&Item::Index(index)) => {
                let at = if index < 0 {
                    index.saturating_add(signed)
                } else {
                    index
                };
                if at < 0 || at >= signed {
                    return Err(SubscriptFailure::Index(format!(
                        "Index ({at}) out of range for (0-{})",
                        signed - 1
                    )));
                }
                Axis {
                    positions: vec![at.unsigned_abs()],
                    kept: false,
                }
            }
            Some(&Item::Slice {
                start,
                stop,
                stride,
            }) => {
                let step = stride.unwrap_or(1);
                if step == 0 {
                    return Err(SubscriptFailure::Value(
                        "slice step cannot be zero".to_owned(),
                    ));
                }
                if step < 0 {
                    return Err(SubscriptFailure::Value(format!(
                        "Step must be >= 1 (got {step})"
                    )));
                }
                let bound = |value: Option<i64>, default: i64| {
                    value.map_or(default, |value| {
                        let value = if value < 0 {
                            value.saturating_add(signed)
                        } else {
                            value
                        };
                        value.clamp(0, signed)
                    })
                };
                let (first, end) = (bound(start, 0), bound(stop, signed));
                let step = step.unsigned_abs();
                let positions = if first < end {
                    (first.unsigned_abs()..end.unsigned_abs())
                        .step_by(usize::try_from(step).unwrap_or(usize::MAX))
                        .collect()
                } else {
                    Vec::new()
                };
                Axis {
                    positions,
                    kept: true,
                }
            }
        };
        axes.push(picked);
    }
    Ok(axes)
}

/// The shape of what `axes` pick: the number of positions along each axis kept.
pub(crate) fn shape(axes: &[Axis]) -> Vec<u64> {
    axes.iter()
        .filter(|axis| axis.kept)
        .map(|axis| axis.positions.len() as u64)
        .collect()
}

/// For a block of the dataset that begins at `origin` and spans `extent` along each axis,
/// the positions `axes` pick inside it: for each axis, each picked position's index in the
/// result and its offset in the block. `None` when the block holds no position picked.
pub(crate) fn inside(
    axes: &[Axis],
    origin: &[u64],
    extent: &[u64],
) -> Option<Vec<Vec<(u64, u64)>>> {
    let mut found = Vec::with_capacity(axes.len());
    for ((axis, &start), &length) in axes.iter().zip(origin).zip(extent) {
        let end = start.saturating_add(length);
        let first = axis.positions.partition_point(|&position| position < start);
        let last = axis.positions.partition_point(|&position| position < end);
        if first == last {
            return None;
        }
        found.push(
            (first..last)
                .map(|index| (index as u64, axis.positions[index] - start))
                .collect(),
        );
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slice of `start`, `stop` and `stride`.
    fn slice(start: Option<i64>, stop: Option<i64>, stride: Option<i64>) -> Item {
        Item::Slice {
            start,
            stop,
            stride,
        }
    }

    #[test]
    fn subscripts_pick_and_refuse_as_h5py_does() {
        // `d[1, ::2, 1:3]` of a 2 by 3 by 4 dataset.
        let axes = select(
            &[
                Item::Index(1),
                slice(None, None, Some(2)),
                slice(Some(1), Some(3), None),
            ],
            &[2, 3, 4],
        )
        .unwrap();
        let positions: Vec<_> = axes.iter().map(|axis| axis.positions.clone()).collect();
        assert_eq!(positions, [vec![1], vec![0, 2], vec![1, 2]]);
        assert_eq!(shape(&axes), [2, 2]);
        // `d[5:1]` picks nothing.
        assert_eq!(
            shape(&select(&[slice(Some(5), Some(1), None)], &[2, 3]).unwrap()),
            [0, 3]
        );
        // The messages h5py 3.16 gives for `d[5]`, `d[-3]`, `d[::0]`, `d[::-1]` and
        // `d[0, 0, 0, 0]`.
        let refused = |items: &[Item]| match select(items, &[2, 3, 4]).unwrap_err() {
            SubscriptFailure::Index(message) => ("IndexError", message),
            SubscriptFailure::Value(message) => ("ValueError", message),
            SubscriptFailure::Os(failure) => ("OSError", failure.strerror),
        };
        let cases: [(&[Item], &str, &str); 5] = [
            (
                &[Item::Index(5)],
                "IndexError",
                "Index (5) out of range for (0-1)",
            ),
            (
                &[Item::Index(-3)],
                "IndexError",
                "Index (-1) out of range for (0-1)",
            ),
            (
                &[slice(None, None, Some(0))],
                "ValueError",
                "slice step cannot be zero",
            ),
            (
                &[slice(None, None, Some(-1))],
                "ValueError",
                "Step must be >= 1 (got -1)",
            ),
            (
                &[Item::Index(0); 4],
                "ValueError",
                "4 indexing arguments for 3 dimensions",
            ),
        ];
        for (items, kind, message) in cases {
            assert_eq!(refused(items), (kind, message.to_owned()));
        }
    }

    #[test]
    fn a_block_holds_the_positions_inside_it() {
        let axes = select(&[slice(None, None, Some(2))], &[6, 5]).unwrap();
        // The chunk of 4 by 2 at (4, 2) holds rows 4 and 5, of which row 4 is picked, the
        // result's third, and columns 2 and 3.
        assert_eq!(
            inside(&axes, &[4, 2], &[4, 2]),
            Some(vec![vec![(2, 0)], vec![(2, 0), (3, 1)]])
        );
        assert_eq!(
            inside(&axes, &[4, 2], &[1, 2]).map(|found| found.len()),
            Some(2)
        );
        let odd = select(&[slice(Some(1), None, Some(2))], &[6, 5]).unwrap();
        assert_eq!(inside(&odd, &[4, 0], &[1, 5]), None);
    }
}
