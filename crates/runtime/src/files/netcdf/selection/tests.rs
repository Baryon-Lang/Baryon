//! Tests of the positions a subscript picks, against what `netCDF4` 1.7.2 reads for the same
//! subscripts of a 4 by 3 variable, and of the stretches a selection is read in.

use super::*;

/// The axis names of a 4 by 3 variable, `(x, y)`.
const NAMES: [&str; 2] = ["x", "y"];

/// A slice of `start`, `stop` and `step`.
fn slice(start: Option<i64>, stop: Option<i64>, stride: Option<i64>) -> Item {
    Item::Slice {
        start,
        stop,
        stride,
    }
}

/// The positions along each axis that `items` pick from a fixed 4 by 3 variable.
fn picked(items: &[Item]) -> Vec<Vec<u64>> {
    select("c", items, (&NAMES, &[4, 3]), false, None)
        .unwrap()
        .axes
        .into_iter()
        .map(|axis| axis.positions)
        .collect()
}

#[test]
fn indexes_and_slices_pick_as_python_does() {
    // `c[-1]`, `c[1:3, -1]`, `c[::-2, 0]` and `c[::2, 1:]`.
    assert_eq!(picked(&[Item::Index(-1)]), [vec![3], vec![0, 1, 2]]);
    assert_eq!(
        picked(&[slice(Some(1), Some(3), None), Item::Index(-1)]),
        [vec![1, 2], vec![2]]
    );
    assert_eq!(
        picked(&[slice(None, None, Some(-2)), Item::Index(0)]),
        [vec![3, 1], vec![0]]
    );
    assert_eq!(
        picked(&[slice(None, None, Some(2)), slice(Some(1), None, None)]),
        [vec![0, 2], vec![1, 2]]
    );
    // Bounds past the ends are held to them, and an empty slice picks nothing.
    assert_eq!(
        picked(&[slice(Some(-10), Some(10), None)]),
        [vec![0, 1, 2, 3], vec![0, 1, 2]]
    );
    assert_eq!(
        picked(&[slice(Some(3), Some(1), None)])[0],
        Vec::<u64>::new()
    );
}

#[test]
fn what_netcdf4_refuses_is_refused() {
    let refused = |items: &[Item]| select("c", items, (&NAMES, &[4, 3]), false, None).unwrap_err();
    assert_eq!(
        refused(&[Item::Index(4)]),
        "index 4 is out of bounds for the dimension x of length 4"
    );
    assert_eq!(
        refused(&[Item::Index(0), Item::Index(-4)]),
        "index -4 is out of bounds for the dimension y of length 3"
    );
    assert_eq!(
        refused(&[Item::Index(0); 3]),
        "the NetCDF variable c has 2 dimensions, fewer than the 3 indexes given"
    );
    assert_eq!(
        refused(&[slice(None, None, Some(0))]),
        "slice step cannot be zero"
    );
}

#[test]
fn a_write_goes_past_the_last_record() {
    let names = ["t", "x"];
    let write = |items: &[Item], shape: &[usize]| {
        select("a", items, (&names, &[2, 3]), true, Some(shape))
            .map(|selection| selection.axes[0].positions.clone())
    };
    // `a[5] = row`, `a[2:] = rows` of 3 rows, `a[1:7:3] = rows` and `a[-1] = row`.
    assert_eq!(write(&[Item::Index(5)], &[3]), Ok(vec![5]));
    assert_eq!(
        write(&[slice(Some(2), None, None)], &[3, 3]),
        Ok(vec![2, 3, 4])
    );
    assert_eq!(
        write(&[slice(Some(1), Some(7), Some(3))], &[2, 3]),
        Ok(vec![1, 4])
    );
    assert_eq!(write(&[Item::Index(-1)], &[3]), Ok(vec![1]));
    assert_eq!(
        write(&[slice(None, None, Some(-1))], &[2, 3]),
        Err(
            "the NetCDF variable a is written forwards along its unlimited dimension t, with a \
             positive stride"
                .to_owned()
        )
    );
}

#[test]
fn neighbouring_values_are_read_together() {
    let runs = |items: &[Item], record: bool, lengths: &[u64]| {
        let names = ["t", "y", "x"];
        let selection = select("v", items, (&names, lengths), record, None).unwrap();
        let inner = &lengths[usize::from(record)..];
        let mut found = Vec::new();
        selection
            .runs(record, inner, |at, offset, size| {
                found.push((at, offset, size));
                Ok::<(), ()>(())
            })
            .unwrap();
        found
    };
    // A fixed variable taken whole is one stretch; a record variable, one a record.
    assert_eq!(runs(&[], false, &[2, 2, 3]), [(None, 0, 12)]);
    assert_eq!(
        runs(&[], true, &[2, 2, 3]),
        [(Some(0), 0, 6), (Some(1), 0, 6)]
    );
    // `v[1, :, 1:]`: two stretches of two; `v[:, :, ::2]` of a fixed variable, one value each.
    assert_eq!(
        runs(
            &[
                Item::Index(1),
                slice(None, None, None),
                slice(Some(1), None, None)
            ],
            true,
            &[2, 2, 3]
        ),
        [(Some(1), 1, 2), (Some(1), 4, 2)]
    );
    assert_eq!(
        runs(
            &[
                slice(None, None, None),
                Item::Index(1),
                slice(None, None, Some(2))
            ],
            false,
            &[1, 2, 3]
        ),
        [(None, 3, 1), (None, 5, 1)]
    );
}

#[test]
fn a_slices_count_is_the_number_of_positions_it_lists() {
    let bounds = [None, Some(-7), Some(-2), Some(0), Some(1), Some(4), Some(9)];
    for length in [0, 1, 5] {
        for start in bounds {
            for stop in bounds {
                for stride in [-3, -1, 1, 2] {
                    let listed = indices(start, stop, stride, length).len() as u64;
                    assert_eq!(
                        count(start, stop, stride, length),
                        listed,
                        "{start:?}:{stop:?}:{stride} of {length}"
                    );
                }
            }
        }
    }
    // A huge stop is counted, never listed.
    assert_eq!(count(Some(0), Some(i64::MAX), 1, u64::MAX), i64::MAX.unsigned_abs());
}
