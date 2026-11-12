//! Which values of a `NetCDF` variable a subscript picks, `var[0, :, ::2]`, as `netCDF4` picks
//! them (decision 0047): an integer takes one position along its axis and drops the axis, a
//! slice takes the positions Python's `slice.indices` gives and keeps it, and the axes after
//! the last item are taken whole.
//!
//! A read takes positions within the dimensions' lengths, the records written for the unlimited
//! one. A write may go past the last record: its positions along the unlimited dimension are
//! not bounded, and a slice there with no end takes as many records as the array has rows.

use crate::files::{ErrorKind, OsFailure};

/// Why a subscript picks no values, sorted by the error `netCDF4` raises for it: an index
/// past its axis, or more indexes than axes, is an `IndexError`; a step of 0 is a
/// `ValueError`, as Python's slices raise; the rest, which the format or the array refuses, is
/// an `OSError` with the kind `INVALID_DATA`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// An `IndexError`, with its message.
    Index(String),
    /// A `ValueError`, with its message.
    Value(String),
    /// An `OSError` with the kind `INVALID_DATA`, with its message.
    Data(String),
}

/// A failed read or write of a subscript of a variable: an `OSError`, or the `IndexError` or
/// `ValueError` of a [`Refusal`], each with its message.
#[derive(Debug)]
pub(crate) enum SubscriptFailure {
    /// An `OSError`.
    Os(OsFailure),
    /// An `IndexError`'s message.
    Index(String),
    /// A `ValueError`'s message.
    Value(String),
}

impl From<OsFailure> for SubscriptFailure {
    fn from(failure: OsFailure) -> Self {
        Self::Os(failure)
    }
}

impl Refusal {
    /// The failure of a read or write of the file at `path` that this refusal stops.
    pub(crate) fn at(self, path: &str) -> SubscriptFailure {
        match self {
            Self::Index(message) => SubscriptFailure::Index(message),
            Self::Value(message) => SubscriptFailure::Value(message),
            Self::Data(message) => {
                SubscriptFailure::Os(OsFailure::new(ErrorKind::InvalidData, message, path))
            }
        }
    }
}

/// The most records a classic file holds: its record count is a 32-bit field, and the C
/// library keeps the all-ones value for a file still being written. A write past it is refused
/// before anything is made for it.
pub(crate) const MOST_RECORDS: u64 = u32::MAX as u64 - 1;

/// The word compiled code passes for a slice's start, stop or step that the program left out.
pub(crate) const LEFT_OUT: i64 = i64::MIN;

/// One item of a subscript, as compiled code passes it: four 64-bit words, `0` and the index,
/// or `1` and the slice's start, stop and step, each [`LEFT_OUT`] when the program left it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    /// An integer index, which may count from the end when negative.
    Index(i64),
    /// A slice, `start:stop:stride`, Python's `start:stop:step`.
    Slice {
        /// Where it starts, or `None` for the axis's first position (last for a negative step).
        start: Option<i64>,
        /// Where it stops, or `None` for past the axis's end.
        stop: Option<i64>,
        /// Its stride, Python's step, or `None` for 1.
        stride: Option<i64>,
    },
}

impl Item {
    /// The items `words` holds, four words each.
    pub(crate) fn from_words(words: &[i64]) -> Vec<Self> {
        let given = |word: i64| (word != LEFT_OUT).then_some(word);
        words
            .chunks_exact(4)
            .map(|item| match *item {
                [0, index, _, _] => Self::Index(index),
                [_, start, stop, stride] => Self::Slice {
                    start: given(start),
                    stop: given(stop),
                    stride: given(stride),
                },
                _ => Self::Index(0),
            })
            .collect()
    }

    /// Whether the item takes a whole axis, `:`.
    fn is_whole(self) -> bool {
        matches!(
            self,
            Self::Slice {
                start: None,
                stop: None,
                stride: None | Some(1),
            }
        )
    }
}

/// Whether every item takes a whole axis, as `var[:]` does.
pub(crate) fn is_whole(items: &[Item]) -> bool {
    items.iter().all(|item| item.is_whole())
}

/// The positions along one axis of a variable that a subscript picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Axis {
    /// The positions, in the order the values are read or written.
    pub(crate) positions: Vec<u64>,
    /// Whether the result keeps the axis: a slice keeps it, an integer drops it.
    pub(crate) kept: bool,
}

/// The positions a subscript picks along each axis of a variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    /// One for each of the variable's axes, slowest first.
    pub(crate) axes: Vec<Axis>,
}

impl Selection {
    /// The shape of the values picked: the number of positions along each kept axis.
    pub(crate) fn shape(&self) -> Vec<usize> {
        self.axes
            .iter()
            .filter(|axis| axis.kept)
            .map(|axis| axis.positions.len())
            .collect()
    }

    /// The number of values picked.
    pub(crate) fn count(&self) -> usize {
        self.axes.iter().map(|axis| axis.positions.len()).product()
    }

    /// Calls `run` for each stretch of picked values that lie together in a variable's slot, in
    /// the order the values are picked: the record (`None` for a fixed variable), the position
    /// of the stretch's first value within the record or the variable, counted in values, and
    /// the number of values. `lengths` are the axes' lengths within a record, the axes after the
    /// unlimited one for a record variable, which `record` says this is. Neighbouring values
    /// form one stretch when the last axes are taken whole and the one before them in steps of
    /// 1, so `var[:]` of a fixed variable is one stretch.
    ///
    /// # Errors
    ///
    /// Stops at the first error `run` gives, and gives it.
    pub(crate) fn runs<E>(
        &self,
        record: bool,
        lengths: &[u64],
        mut run: impl FnMut(Option<u64>, u64, u64) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.axes.iter().any(|axis| axis.positions.is_empty()) {
            return Ok(());
        }
        let inner = &self.axes[usize::from(record)..];
        let consecutive = |axis: &Axis| {
            axis.positions
                .windows(2)
                .all(|pair| pair[1] == pair[0].wrapping_add(1))
        };
        // The inner axes from `split` on form the stretches, of `size` values each.
        let mut split = inner.len();
        let mut size = 1_u64;
        for (at, axis) in inner.iter().enumerate().rev() {
            if !consecutive(axis) {
                break;
            }
            split = at;
            size = size.saturating_mul(axis.positions.len() as u64);
            let whole =
                axis.positions.first() == Some(&0) && axis.positions.len() as u64 == lengths[at];
            if !whole {
                break;
            }
        }
        // What one step along each inner axis moves, in values.
        let mut strides = vec![1_u64; inner.len()];
        for at in (0..inner.len().saturating_sub(1)).rev() {
            strides[at] = strides[at + 1].saturating_mul(lengths[at + 1]);
        }
        let first: u64 = inner[split..]
            .iter()
            .zip(&strides[split..])
            .map(|(axis, &stride)| axis.positions[0].saturating_mul(stride))
            .sum();
        // An odometer over the axes outside the stretches, the record axis among them.
        let outer = &self.axes[..self.axes.len() - (inner.len() - split)];
        let mut index = vec![0_usize; outer.len()];
        loop {
            let mut at_record = None;
            let mut offset = first;
            for (axis_number, (axis, &at)) in outer.iter().zip(&index).enumerate() {
                let position = axis.positions[at];
                if record && axis_number == 0 {
                    at_record = Some(position);
                } else {
                    let stride = strides[axis_number - usize::from(record)];
                    offset = offset.saturating_add(position.saturating_mul(stride));
                }
            }
            run(at_record, offset, size)?;
            let mut axis = outer.len();
            loop {
                if axis == 0 {
                    return Ok(());
                }
                axis -= 1;
                index[axis] += 1;
                if index[axis] < outer[axis].positions.len() {
                    break;
                }
                index[axis] = 0;
            }
        }
    }
}

/// The values of the variable `name`, whose axes are the dimensions `names` of the lengths
/// `lengths`, that `items` pick. `writing` is the shape of the array a write takes, which
/// unbounds the record axis when `record` says the first axis is the unlimited one.
///
/// # Errors
///
/// A [`Refusal`] when there are more items than axes, an index is past its axis's bounds, a step
/// is 0, a write steps backwards along the unlimited dimension or past [`MOST_RECORDS`], or a
/// write's slice picks more positions than its array has rows. Each is found from the items'
/// numbers before any list of positions is made, so a huge index or stop costs nothing.
pub(crate) fn select(
    name: &str,
    items: &[Item],
    (names, lengths): (&[&str], &[u64]),
    record: bool,
    writing: Option<&[usize]>,
) -> Result<Selection, Refusal> {
    if items.len() > lengths.len() {
        return Err(Refusal::Index(format!(
            "the NetCDF variable {name} has {} dimensions, fewer than the {} indexes given",
            lengths.len(),
            items.len()
        )));
    }
    let whole = Item::Slice {
        start: None,
        stop: None,
        stride: None,
    };
    let mut kept = 0;
    let mut axes = Vec::with_capacity(lengths.len());
    for (axis, &length) in lengths.iter().enumerate() {
        let item = items.get(axis).copied().unwrap_or(whole);
        let unbounded = record && axis == 0 && writing.is_some();
        let rows = writing.and_then(|shape| shape.get(kept).copied());
        let positions = match item {
            Item::Index(index) => {
                let position = if index < 0 {
                    index.checked_add_unsigned(length)
                } else {
                    Some(index)
                };
                match position.and_then(|position| u64::try_from(position).ok()) {
                    Some(position) if unbounded && position >= MOST_RECORDS => {
                        return Err(too_many_records(name));
                    }
                    Some(position) if unbounded || position < length => vec![position],
                    _ => {
                        return Err(Refusal::Index(format!(
                            "index {index} is out of bounds for the dimension {} of length \
                             {length}",
                            names[axis]
                        )));
                    }
                }
            }
            Item::Slice {
                start,
                stop,
                stride,
            } => {
                let stride = stride.unwrap_or(1);
                if stride == 0 {
                    return Err(Refusal::Value("slice step cannot be zero".to_owned()));
                }
                if unbounded && stride < 0 {
                    return Err(Refusal::Data(format!(
                        "the NetCDF variable {name} is written forwards along its unlimited \
                         dimension {}, with a positive stride",
                        names[axis]
                    )));
                }
                if unbounded && stop.is_none() {
                    let start = resolve(start, length, (0, i64::MAX)).unsigned_abs();
                    let count = rows.unwrap_or(0) as u64;
                    let stride = stride.unsigned_abs();
                    let last = count
                        .checked_sub(1)
                        .map(|steps| steps.checked_mul(stride)?.checked_add(start));
                    if matches!(last, Some(None)) || last.flatten() >= Some(MOST_RECORDS) {
                        return Err(too_many_records(name));
                    }
                    (0..count).map(|row| start + row * stride).collect()
                } else {
                    // A write's explicit stop may pass the records written, up to the most a
                    // file holds.
                    let length = match (unbounded, stop) {
                        (true, Some(stop)) if stop.unsigned_abs() > MOST_RECORDS => {
                            return Err(too_many_records(name));
                        }
                        (true, Some(stop)) => length.max(stop.unsigned_abs()),
                        _ => length,
                    };
                    // A write picks as many positions as its array has rows. Past the records
                    // written, more is an error the shape check would report, found here
                    // before they are listed; elsewhere the file's own length bounds them.
                    let count = count(start, stop, stride, length);
                    if let Some(rows) = rows.filter(|&rows| unbounded && count > rows as u64) {
                        return Err(Refusal::Data(format!(
                            "the values picked of the NetCDF variable {name} have {count} \
                             positions along its dimension {}, which an array of {rows} \
                             rows does not fill",
                            names[axis]
                        )));
                    }
                    indices(start, stop, stride, length)
                }
            }
        };
        let is_kept = matches!(item, Item::Slice { .. });
        kept += usize::from(is_kept);
        axes.push(Axis {
            positions,
            kept: is_kept,
        });
    }
    Ok(Selection { axes })
}

/// The message for a write past [`MOST_RECORDS`] of the variable `name`.
fn too_many_records(name: &str) -> Refusal {
    Refusal::Data(format!(
        "a NetCDF classic file holds at most {MOST_RECORDS} records, fewer than a write of \
         the variable {name} needs"
    ))
}

/// How many positions `start:stop:stride` takes along an axis of `length`, worked out from
/// the bounds, as `len(range(...))` is, without listing them; `stride` is not 0.
fn count(start: Option<i64>, stop: Option<i64>, stride: i64, length: u64) -> u64 {
    let (start, stop) = bounds(start, stop, stride, length);
    let (start, stop, stride) = (i128::from(start), i128::from(stop), i128::from(stride));
    let span = if stride > 0 {
        stop - start
    } else {
        start - stop
    };
    if span <= 0 {
        return 0;
    }
    let count = (span - 1) / stride.abs() + 1;
    u64::try_from(count).unwrap_or(u64::MAX)
}

/// The first position `start:stop:stride` takes along an axis of `length` and the bound it
/// stops before, as Python's `slice.indices` gives them; `stride` is not 0.
fn bounds(start: Option<i64>, stop: Option<i64>, stride: i64, length: u64) -> (i64, i64) {
    let length = i64::try_from(length).unwrap_or(i64::MAX);
    let range = if stride > 0 {
        (0, length)
    } else {
        (-1, length - 1)
    };
    let (first, last) = if stride > 0 {
        (range.0, range.1)
    } else {
        (range.1, range.0)
    };
    let start = start.map_or(first, |start| {
        resolve(Some(start), length.unsigned_abs(), range)
    });
    let stop = stop.map_or(last, |stop| {
        resolve(Some(stop), length.unsigned_abs(), range)
    });
    (start, stop)
}

/// The positions `start:stop:stride` takes along an axis of `length`, as Python's
/// `slice.indices` and `range` give them; `stride` is not 0. Cost: one position each, so the
/// caller checks [`count`] first where the length is not the file's own.
fn indices(start: Option<i64>, stop: Option<i64>, stride: i64, length: u64) -> Vec<u64> {
    let (start, stop) = bounds(start, stop, stride, length);
    let mut positions = Vec::new();
    let mut at = Some(start);
    while let Some(position) =
        at.filter(|&at| (stride > 0 && at < stop) || (stride < 0 && at > stop))
    {
        positions.push(position.unsigned_abs());
        at = position.checked_add(stride);
    }
    positions
}

/// A slice's `bound` along an axis of `length`, counted from the end when negative and held
/// within `(lower, upper)`, as `slice.indices` holds it; a bound left out is `lower`.
fn resolve(bound: Option<i64>, length: u64, (lower, upper): (i64, i64)) -> i64 {
    let length = i64::try_from(length).unwrap_or(i64::MAX);
    match bound {
        None => lower,
        Some(bound) if bound < 0 => (bound + length).max(lower),
        Some(bound) => bound.min(upper),
    }
}

#[cfg(test)]
mod tests;
