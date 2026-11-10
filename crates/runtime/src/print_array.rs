//! `print` of an array: the text `NumPy`'s `print` writes for an `ndarray` with the default print
//! options, byte for byte (decision 0033), such as `[1.  2.5]` or a matrix with one row per line.
//!
//! Writing an array takes three steps, each `NumPy`'s own (`numpy/_core/arrayprint.py`, `NumPy`
//! 2.5):
//!
//! 1. **Pick the elements.** An array of more than [`THRESHOLD`] elements is summarized: along
//!    each dimension longer than twice [`EDGE_ITEMS`], only the first and last [`EDGE_ITEMS`]
//!    are kept, with `...` between them. [`Plan`] says which positions are kept; the caller
//!    reads those elements, in row-major order, into an [`Elements`].
//! 2. **Format every element alike.** One format is chosen from all the kept elements, so they
//!    line up: one width for integers, one notation, width and digit count for floats
//!    ([`float`]), and `" True"` beside `"False"` for `bool`.
//! 3. **Lay them out.** Elements of the last dimension are separated by one space and wrapped
//!    at [`LINE_WIDTH`] characters; each row of a matrix starts a line, and each block of a
//!    rank-3 array is followed by a blank line, as [`Layout`] writes them.
//!
//! An array with no elements, whatever its shape, prints `[]`.
//!
//! Cost: reading and formatting at most 1000 elements, or the summary's corners of a larger
//! array, whatever its size: printing an array of 10⁸ elements formats a few dozen.

mod float;
#[cfg(test)]
mod tests;

use float::{Float, FloatFormat, Sign};

/// The most elements an array may have and still print in full; `NumPy`'s `threshold`.
pub(crate) const THRESHOLD: usize = 1000;

/// How many elements a summarized dimension keeps at each end; `NumPy`'s `edgeitems`.
pub(crate) const EDGE_ITEMS: usize = 3;

/// The longest line, in characters, before elements wrap onto the next; `NumPy`'s `linewidth`.
const LINE_WIDTH: usize = 75;

/// The elements of an array that print, read from it in row-major order: every element, or
/// only the corners [`Plan`] keeps. Integers come widened to 64 bits, signed or not; a complex
/// number is its real part, then its imaginary part.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Elements {
    /// `bool` elements.
    Bool(Vec<bool>),
    /// Signed integers of any width.
    Signed(Vec<i64>),
    /// Unsigned integers of any width.
    Unsigned(Vec<u64>),
    /// `f32` elements, which keep their own shortest digits.
    F32(Vec<f32>),
    /// `f64` elements.
    F64(Vec<f64>),
    /// `complex64` elements.
    Complex64(Vec<(f32, f32)>),
    /// `complex128` elements.
    Complex128(Vec<(f64, f64)>),
}

/// Which positions of an array print, from its shape alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Whether the array has more than [`THRESHOLD`] elements, so that each long dimension is
    /// cut to its corners.
    summarized: bool,
    /// For each dimension, outermost first, the positions kept along it, in increasing order.
    pub(crate) kept: Vec<Vec<usize>>,
}

impl Plan {
    /// The plan for an array of shape `lengths`. A summarized dimension longer than twice
    /// [`EDGE_ITEMS`] keeps its first and last [`EDGE_ITEMS`] positions; every other dimension
    /// keeps all of them.
    pub(crate) fn new(lengths: &[usize]) -> Self {
        let count = lengths
            .iter()
            .try_fold(1_usize, |count, &length| count.checked_mul(length));
        let summarized = count.is_none_or(|count| count > THRESHOLD);
        let kept = lengths
            .iter()
            .map(|&length| {
                if summarized && length > 2 * EDGE_ITEMS {
                    (0..EDGE_ITEMS).chain(length - EDGE_ITEMS..length).collect()
                } else {
                    (0..length).collect()
                }
            })
            .collect();
        Self { summarized, kept }
    }

    /// Whether the array has no elements, so that it prints `[]`.
    pub(crate) fn is_empty(&self) -> bool {
        self.kept.iter().any(Vec::is_empty)
    }
}

/// The text `NumPy`'s `print` writes for an array of shape `lengths` whose kept elements, as
/// `plan` picks them from that shape, are `elements`, in row-major order.
pub(crate) fn array(lengths: &[usize], plan: &Plan, elements: &Elements) -> String {
    if plan.is_empty() || lengths.is_empty() {
        return "[]".to_owned();
    }
    let mut layout = Layout {
        lengths,
        summarized: plan.summarized,
        words: words(elements).into_iter(),
    };
    layout.dimension(0, " ", LINE_WIDTH)
}

/// Each element of `elements` written as it appears in the array, all in one format and padded
/// to one width.
fn words(elements: &Elements) -> Vec<String> {
    match elements {
        Elements::Bool(values) => values
            .iter()
            .map(|&value| if value { " True" } else { "False" }.to_owned())
            .collect(),
        Elements::Signed(values) => integers(values),
        Elements::Unsigned(values) => integers(values),
        Elements::F32(values) => floats(values),
        Elements::F64(values) => floats(values),
        Elements::Complex64(values) => complexes(values),
        Elements::Complex128(values) => complexes(values),
    }
}

/// Integers right-aligned to the width of the widest, as `NumPy`'s `IntegerFormat` writes them.
fn integers<T: ToString>(values: &[T]) -> Vec<String> {
    let written: Vec<String> = values.iter().map(ToString::to_string).collect();
    let width = written.iter().map(String::len).max().unwrap_or(0);
    written
        .into_iter()
        .map(|text| format!("{text:>width$}"))
        .collect()
}

/// Floats in one [`FloatFormat`], chosen from all of them.
fn floats<T: Float>(values: &[T]) -> Vec<String> {
    let format = FloatFormat::new(values, Sign::Negative);
    values.iter().map(|&value| format.write(value)).collect()
}

/// Complex numbers as `NumPy`'s `ComplexFloatingFormat` writes them: the real parts in one
/// format, the imaginary parts in another that always shows a sign, and `j` after the
/// imaginary part's digits, before any padding, so `[1.+2.j  0.-0.5j]`.
fn complexes<T: Float>(values: &[(T, T)]) -> Vec<String> {
    let (reals, imaginaries): (Vec<T>, Vec<T>) = values.iter().copied().unzip();
    let real = FloatFormat::new(&reals, Sign::Negative);
    let imaginary = FloatFormat::new(&imaginaries, Sign::Always);
    values
        .iter()
        .map(|&(re, im)| {
            let mut text = real.write(re);
            let part = imaginary.write(im);
            let digits = part.trim_end();
            text.push_str(digits);
            text.push('j');
            text.push_str(&part[digits.len()..]);
            text
        })
        .collect()
}

/// Lays out formatted elements as `NumPy`'s `_formatArray` does, with its default separator, a
/// space.
struct Layout<'a, Words> {
    /// The array's shape, outermost dimension first.
    lengths: &'a [usize],
    /// Whether long dimensions show only their corners, with `...` between.
    summarized: bool,
    /// The formatted elements, in the row-major order of the kept positions.
    words: Words,
}

impl<Words: Iterator<Item = String>> Layout<'_, Words> {
    /// The text of the sub-array at dimension `axis`, in brackets. `hanging` is the indent of
    /// each line after the first, one space per open bracket; `width` is the room left on a
    /// line, one less for each closing bracket that may follow.
    fn dimension(&mut self, axis: usize, hanging: &str, width: usize) -> String {
        let Some(&length) = self.lengths.get(axis) else {
            return self.words.next().unwrap_or_default();
        };
        let inner_hanging = format!("{hanging} ");
        let inner_width = width.saturating_sub(1);
        let summary = self.summarized && length > 2 * EDGE_ITEMS;
        let (leading, trailing) = if summary {
            (EDGE_ITEMS, EDGE_ITEMS)
        } else {
            (0, length)
        };
        let mut text = String::new();
        if axis + 1 == self.lengths.len() {
            // The last dimension: elements on one line, wrapped. The room for an element leaves
            // space for the `]` after the last one.
            let room = width.saturating_sub(1);
            let mut line = hanging.to_owned();
            for _ in 0..leading {
                let word = self.dimension(axis + 1, &inner_hanging, inner_width);
                extend(&mut text, &mut line, &word, room, hanging);
                line.push(' ');
            }
            if summary {
                extend(&mut text, &mut line, "...", room, hanging);
                line.push(' ');
            }
            for position in 0..trailing {
                let word = self.dimension(axis + 1, &inner_hanging, inner_width);
                extend(&mut text, &mut line, &word, room, hanging);
                if position + 1 < trailing {
                    line.push(' ');
                }
            }
            text.push_str(&line);
        } else {
            // An outer dimension: each sub-array on its own line, with one blank line more
            // between them for each dimension below the next.
            let rows_left = self.lengths.len() - axis - 1;
            let separator = "\n".repeat(rows_left);
            let mut first = true;
            for _ in 0..leading {
                let nested = self.dimension(axis + 1, &inner_hanging, inner_width);
                push_row(&mut text, &mut first, hanging, &nested, &separator);
            }
            if summary {
                push_row(&mut text, &mut first, hanging, "...", &separator);
            }
            for _ in 0..trailing {
                let nested = self.dimension(axis + 1, &inner_hanging, inner_width);
                push_row(&mut text, &mut first, hanging, &nested, &separator);
            }
        }
        // The first line carries the opening bracket in place of its indent.
        format!("[{}]", text.get(hanging.len()..).unwrap_or_default())
    }
}

/// Appends a row of an outer dimension to `text`: `separator` before it unless it is the first,
/// then the hanging indent and the row.
fn push_row(text: &mut String, first: &mut bool, hanging: &str, row: &str, separator: &str) {
    if !*first {
        text.push_str(separator);
    }
    *first = false;
    text.push_str(hanging);
    text.push_str(row);
}

/// Adds `word` to the line being built, first moving `line`, without its trailing spaces, into
/// `text` and starting a new line at `hanging` when the word would pass `width`. A line that
/// holds only its indent never wraps, since wrapping would not help: `NumPy`'s `_extendLine`.
fn extend(text: &mut String, line: &mut String, word: &str, width: usize, hanging: &str) {
    if line.len() + word.len() > width && line.len() > hanging.len() {
        text.push_str(line.trim_end());
        text.push('\n');
        hanging.clone_into(line);
    }
    line.push_str(word);
}
