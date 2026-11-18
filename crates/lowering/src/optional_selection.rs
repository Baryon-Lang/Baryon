//! Choosing elements of an array with missing values: `bids[mask]`, `bids[positions]` and
//! `bids[1:3]`, where `bids` is an `Array[T | None, 1]` (decision 0045, part 3).
//!
//! Each gives a new array with missing values that holds a copy of the elements chosen, a
//! `None` staying a `None`. The values are chosen by the library function that chooses
//! those of a plain array, `masked` or `taken`, or `text_masked` or `text_taken` for text,
//! and the validity bitmap by `masked_bits` or `taken_bits`, so the two stay in step. A
//! slice is the positions it names, taken: a copy and not a view, as a slice of a text array
//! is, since a bitmap cannot be viewed from the middle of a byte.

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};

impl Builder<'_> {
    /// Whether the subscript `node` is a slice of an array with missing values, `bids[1:3]`.
    pub(crate) fn is_optional_slice(&self, node: NodeId) -> bool {
        self.tree
            .child(node, Role::Index)
            .is_some_and(|index| self.tree.kind(index) == SyntaxKind::Slice)
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.optional_array_element(value).is_some())
    }

    /// `bids[mask]`, `bids[positions]` or `bids[i:j:k]`, the subscript `node` of the array
    /// with missing values `value` with the index `index`, into `place`: a new array of the
    /// elements chosen.
    ///
    /// A mask may hold `None` itself, and chooses where it is `True`, as in pandas. A mask
    /// of another length panics, and so does a position outside the array. A slice follows
    /// Python, its bounds clamped to the array. Cost: what the same choice costs a plain
    /// array, and the same again over the bitmap.
    pub(crate) fn optional_selection(
        &mut self,
        place: Place,
        value: NodeId,
        index: NodeId,
        node: NodeId,
    ) {
        let Some(element) = self.optional_array_element(value) else {
            return;
        };
        let source = self.borrow(value);
        let length = self.optional_array_length(&source, element, node);
        let (function, by) = if self.tree.kind(index) == SyntaxKind::Slice {
            let count = self.held(self.int_type(), length, node);
            let Some(positions) = self.slice_positions(count, index, node) else {
                return;
            };
            ("taken", Place::from(positions))
        } else if self.optional_array_element(index).is_some() {
            let Some(mask) = self.chosen_mask(length, index) else {
                return;
            };
            ("masked", Place::from(mask))
        } else {
            let by = self.borrow(index);
            let by_type = self.ty(index);
            if self.element(by_type) == self.boolean_type() {
                let chosen = self.length(&by, by_type, 0, index);
                self.same_length(0, length, chosen, index);
                ("masked", by)
            } else {
                ("taken", by)
            }
        };
        let values = source.clone().project(Projection::Field(VALUES));
        let bits = source.project(Projection::Field(VALIDITY));
        let Some(values) = self.values_call(function, element, vec![values, by.clone()], node)
        else {
            return;
        };
        let name = format!("{function}_bits");
        let bitmap = self.bitmap_type();
        let Some(bits) = self.frame_call(&name, Vec::new(), vec![bits, by], bitmap, node) else {
            return;
        };
        let whole = self.with_bits(element, values, bits, node);
        self.assign(place, Rvalue::Use(whole), node);
    }
}
