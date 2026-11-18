//! Choosing elements of an array with missing values: `bids[mask]`, `bids[positions]` and
//! `bids[1:3]`, where `bids` is an `Array[T | None, 1]` (decision 0045, part 3).
//!
//! Each gives a new array with missing values that holds a copy of the elements chosen, a
//! `None` staying a `None`. The values are chosen by the library function that chooses
//! those of a plain array, `masked` or `taken`, or `text_masked` or `text_taken` for text,
//! and the validity bitmap by `masked_bits` or `taken_bits`, so the two stay in step. A
//! slice is the positions it names, taken: a copy and not a view, as a slice of a text array
//! is, since a bitmap cannot be viewed from the middle of a byte.
//!
//! `bids[mask] = value` stores one value, or `None`, in the elements chosen, in place.

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

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

    /// Whether the subscript `node` chooses several elements of an array with missing
    /// values: by a slice, a mask, positions or a list.
    pub(crate) fn is_optional_selection(&self, node: NodeId) -> bool {
        self.is_selection(node)
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
            let (by, by_type) = self.chooser(index);
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

    /// `values[index] = value`, where `index` chooses several elements of the array of
    /// numbers or `bool`s with missing values that `target` names: every chosen element
    /// becomes the one `value`, a number or `None`, which is computed first. A slice is
    /// turned into its positions, and a mask that may hold `None` into a plain mask that
    /// is true where it is `True`; then the library's `set_optional_where` or
    /// `set_optional_at` stores the value and the validity bits in place. A mask of another
    /// length and a position out of range panic. Cost: one pass over the mask or the
    /// positions.
    pub(crate) fn optional_selection_store(&mut self, target: NodeId, value: NodeId) {
        let (Some(array), Some(index)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        let Some(element) = self.optional_array_element(array) else {
            return;
        };
        let Some(filler) = self.filler(element) else {
            return self.unsupported(target, "assigning elements of this array");
        };
        let optional = Type::new(self.db, TypeKind::Optional(element));
        let item = self.temporary(optional, value);
        self.write(Place::from(item), optional, value);
        let Some(base) = self.changed_place(array) else {
            return;
        };
        let length = self.optional_array_length(&base, element, target);
        let (function, by) = if self.tree.kind(index) == SyntaxKind::Slice {
            let count = self.held(self.int_type(), length, target);
            let Some(positions) = self.slice_positions(count, index, target) else {
                return;
            };
            ("set_optional_at", Place::from(positions))
        } else if self.optional_array_element(index).is_some() {
            let Some(mask) = self.chosen_mask(length, index) else {
                return;
            };
            ("set_optional_where", Place::from(mask))
        } else {
            let (by, by_type) = self.chooser(index);
            if self.element(by_type) == self.boolean_type() {
                let chosen = self.length(&by, by_type, 0, index);
                self.same_length(0, length, chosen, index);
                ("set_optional_where", by)
            } else {
                ("set_optional_at", by)
            }
        };
        let filler = Place::from(self.held(element, Operand::Constant(filler), target));
        let changed = vec![
            base.clone().project(Projection::Field(VALUES)),
            base.project(Projection::Field(VALIDITY)),
        ];
        let read = vec![by, Place::from(item), filler];
        self.frame_update(function, element, (changed, read), target);
    }
}
