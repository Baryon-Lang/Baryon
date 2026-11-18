//! Comparisons of text arrays with missing values: `names == "ada"` and `names < others`,
//! where a side is an `Array[str | None, 1]` (decision 0045, part 3).
//!
//! The result is an `Array[bool | None, 1]` with `None` wherever a side has `None`, as a
//! comparison of numbers gives ([`crate::optional_arithmetic`]). Text has no fused loop, so
//! the values come from the same library functions a comparison of plain text arrays calls,
//! `text_compared` and `text_compared_to`, run over the stored text. Under a `None` the
//! stored text is empty, and what it compares to is never read.

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use crate::text_arrays::comparison_number;
use mir::{Constant, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::TypeKind;

impl Builder<'_> {
    /// Whether `node` is a text array, with or without missing values.
    fn is_any_text_array(&self, node: NodeId) -> bool {
        self.is_text_array(node)
            || self
                .optional_array_element(node)
                .is_some_and(|element| matches!(element.kind(self.db), TypeKind::Str))
    }

    /// Whether `node` is a comparison of text whose result is an array of `bool`s with
    /// missing values: a side is an `Array[str | None, 1]`.
    pub(crate) fn is_optional_text_comparison(&self, node: NodeId) -> bool {
        self.is_optional_comparison(node)
            && self.compared(node).is_some_and(|(_, left, right)| {
                self.is_any_text_array(left) || self.is_any_text_array(right)
            })
    }

    /// Borrows the text array `node` and gives where its text is stored. The validity
    /// bitmap of an array with missing values is added to `bitmaps`.
    fn stored_text(&mut self, node: NodeId, bitmaps: &mut Vec<Place>) -> Place {
        let base = self.borrow(node);
        if self.optional_array_element(node).is_none() {
            return base;
        }
        bitmaps.push(base.clone().project(Projection::Field(VALIDITY)));
        base.project(Projection::Field(VALUES))
    }

    /// The comparison `node` of text, where a side is an array with missing values, into
    /// `place`: `None` wherever a side is `None`. The other side is a text array, with or
    /// without missing values, or one `str`. Two arrays of different lengths panic. Cost:
    /// one pass comparing text, and one over the validity bitmaps.
    pub(crate) fn optional_text_comparison(&mut self, place: Place, node: NodeId) {
        let Some((operator, left, right)) = self.compared(node) else {
            return;
        };
        let (array, other, turned) = if self.is_any_text_array(left) {
            (left, right, false)
        } else {
            (right, left, true)
        };
        let Some(how) = comparison_number(operator, turned) else {
            return self.unsupported(node, "this comparison of an array with missing values");
        };
        let mut bitmaps = Vec::new();
        let source = self.stored_text(array, &mut bitmaps);
        let (function, with) = if self.is_any_text_array(other) {
            let with = self.stored_text(other, &mut bitmaps);
            let length = self.text_array_length(&source, node);
            let other_length = self.text_array_length(&with, other);
            self.same_length(0, length, other_length, node);
            ("text_compared", with)
        } else {
            ("text_compared_to", self.borrow(other))
        };
        let (int, boolean) = (self.int_type(), self.boolean_type());
        let how = Operand::Constant(Constant::integer(int, how));
        let how = Place::from(self.held(int, how, node));
        let returns = self.array_of(boolean);
        let arguments = vec![source, with, how];
        let Some(values) = self.frame_call(function, Vec::new(), arguments, returns, node) else {
            return;
        };
        let Some(bits) = self.joined_bitmaps(&bitmaps, node) else {
            return;
        };
        let whole = self.with_bits(boolean, values, bits, node);
        self.assign(place, Rvalue::Use(whole), node);
    }
}
